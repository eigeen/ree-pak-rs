use std::io::{Read, Seek, SeekFrom};
use std::{path::Path, path::PathBuf, sync::Arc, time::Duration};

use color_eyre::eyre::{self, Context};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::{Mmap, MmapOptions};
use ree_pak_core::{
    extract::ExtractEvent,
    filename::FileNameTable,
    pak::FeatureFlags,
    pakfile::{PakFile, PakReader},
    read,
};
use regex::Regex;
use serde::Serialize;

use crate::{CliPakBackend, DumpInfoCommand, UnpackCommand};

#[derive(Debug, Serialize)]
struct PakInfo {
    header: ree_pak_core::pak::PakHeader,
    #[serde(skip_serializing_if = "Option::is_none")]
    chunk_table: Option<ChunkTableInfo>,
    entries: Vec<EntryWithPath>,
}

#[derive(Debug, Serialize)]
struct ChunkTableInfo {
    block_size: u32,
    chunks: Vec<ChunkDescInfo>,
}

#[derive(Debug, Serialize)]
struct ChunkDescInfo {
    start: u64,
    meta: u32,
}

#[derive(Debug, Serialize)]
struct EntryWithPath {
    entry: ree_pak_core::pak::PakEntry,
    path: Option<String>,
}

pub fn dump_info(cmd: &DumpInfoCommand) -> color_eyre::Result<()> {
    let filename_table = load_filename_table(&cmd.project)?;

    let file = std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
    let mut reader = std::io::BufReader::new(file);
    let metadata = read::read_metadata(&mut reader)?;

    let chunk_table = if metadata.header().feature().contains(FeatureFlags::CHUNK_TABLE) {
        let table = read::chunk_table::read_chunk_table(&mut reader)?;
        Some(ChunkTableInfo {
            block_size: table.block_size(),
            chunks: table
                .chunks()
                .iter()
                .map(|c| ChunkDescInfo {
                    start: c.start(),
                    meta: c.meta(),
                })
                .collect(),
        })
    } else {
        None
    };

    let info = PakInfo {
        header: metadata.header().clone(),
        chunk_table,
        entries: metadata
            .entries()
            .iter()
            .map(|entry| {
                let path = filename_table
                    .get_file_name(entry.hash())
                    .map(|fname| fname.to_string().unwrap());
                EntryWithPath {
                    entry: entry.clone(),
                    path,
                }
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&info)?;

    let output_path = if let Some(output) = &cmd.output {
        output.into()
    } else {
        let mut path = PathBuf::from(&cmd.input);
        path.set_extension("json");
        path
    };
    std::fs::write(output_path, json)?;

    Ok(())
}

pub fn unpack_parallel(cmd: &UnpackCommand) -> color_eyre::Result<()> {
    // load project file name table
    let file_name_table = Arc::new(load_filename_table(&cmd.project)?);

    // output path
    let output_path = output_path(&cmd.output, &cmd.input);

    // apply filter
    let filters = cmd
        .filter
        .iter()
        .filter(|f| !f.trim().is_empty())
        .map(|f| Regex::new(f))
        .collect::<Result<Vec<_>, _>>()?;
    let filters = Arc::new(filters);

    // progress
    let bar = ProgressBar::new(0);
    bar.set_style(ProgressStyle::default_bar().template("{pos}/{len} files {wide_bar} elapsed: {elapsed} eta: {eta}")?);
    bar.enable_steady_tick(Duration::from_millis(100));
    if cmd.test {
        bar.println(format!(
            "Test mode (in memory): output directory ignored: `{}`",
            output_path.display()
        ));
    } else {
        bar.println(format!("Output directory: `{}`", output_path.display()));
    }

    // open pak (path wrapper kept in CLI)
    let report = if cmd.test {
        match cmd.backend {
            CliPakBackend::Legacy => {
                let file =
                    std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
                let pak = PakFile::from_file(file)?;
                test_with_pak(
                    &pak,
                    Arc::clone(&file_name_table),
                    Arc::clone(&filters),
                    bar.clone(),
                    cmd,
                )?
            }
            CliPakBackend::Mmap => {
                let file =
                    std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
                // SAFETY: read-only mapping.
                let mmap = unsafe { MmapOptions::new().map(&file)? };
                let pak = PakFile::from_reader(MmapReader::new(mmap))?;
                test_with_pak(
                    &pak,
                    Arc::clone(&file_name_table),
                    Arc::clone(&filters),
                    bar.clone(),
                    cmd,
                )?
            }
        }
    } else {
        match cmd.backend {
            CliPakBackend::Legacy => {
                let file =
                    std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
                let pak = PakFile::from_file(file)?;
                unpack_with_pak(
                    pak,
                    &output_path,
                    Arc::clone(&file_name_table),
                    Arc::clone(&filters),
                    bar.clone(),
                    cmd,
                )?
            }
            CliPakBackend::Mmap => {
                let file =
                    std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
                // SAFETY: read-only mapping.
                let mmap = unsafe { MmapOptions::new().map(&file)? };
                let pak = PakFile::from_reader(MmapReader::new(mmap))?;
                unpack_with_pak(
                    pak,
                    &output_path,
                    Arc::clone(&file_name_table),
                    Arc::clone(&filters),
                    bar.clone(),
                    cmd,
                )?
            }
        }
    };

    if report.failed > 0 {
        println!("Done with {} errors", report.failed);
        if report.errors.len() < 30 {
            println!("Errors: {:?}", report.errors);
        } else {
            println!("Errors: {:?}", &report.errors[0..30]);
            println!(
                "Displaying only the first 30 errors. Too many errors to display ({}).",
                report.errors.len()
            );
        }
    } else {
        println!("Done.");
    }

    Ok(())
}

fn test_with_pak<R>(
    pak: &PakFile<R>,
    file_name_table: Arc<FileNameTable>,
    filters: Arc<Vec<Regex>>,
    bar: ProgressBar,
    cmd: &UnpackCommand,
) -> color_eyre::Result<ree_pak_core::extract::ExtractReport>
where
    R: PakReader,
{
    let report = pak
        .extractor_callback()
        .file_name_table_arc(file_name_table)
        .skip_unknown(cmd.skip_unknown)
        .continue_on_error(cmd.ignore_error)
        .filter({
            let filters = Arc::clone(&filters);
            move |_entry, path| {
                if filters.is_empty() {
                    return true;
                }
                let Some(path) = path else { return false };
                filters.iter().any(|f| f.is_match(path))
            }
        })
        .on_event({
            let bar = bar.clone();
            move |event| match event {
                ExtractEvent::Start { total } => bar.set_length(total as u64),
                ExtractEvent::FileDone { error, .. } => {
                    if let Some(error) = error {
                        bar.println(format!("Error: {error}"));
                    }
                    bar.inc(1);
                }
                ExtractEvent::Finish { .. } => bar.finish(),
                _ => {}
            }
        })
        .run_with_bytes({
            let bar = bar.clone();
            move |entry, rel_path, bytes| {
                let determined = read::entry::determine_extension_from_bytes(&bytes);
                if let Some(detected_ext) = determined
                    && let Some(path_ext) = logical_path_extension_for_check(rel_path)
                {
                    let detected_ext = detected_ext.to_ascii_lowercase();
                    if path_ext != detected_ext {
                        bar.println(format!(
                            "Warning: extension mismatch for `{}`: path_ext={} detected=.{} hash={:016X}",
                            rel_path.display(),
                            path_ext,
                            detected_ext,
                            entry.hash()
                        ));
                    }
                }

                Ok(())
            }
        })?;

    Ok(report)
}

fn logical_path_extension_for_check(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy();
    let parts = file_name.split('.').collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }

    let last = parts.len() - 1;
    let mut ext_idx = last;

    // Fixed format support:
    // - `name.suffix.version` (version is digits)
    // - `name.suffix.version.region` (version is digits; region is any non-empty string)
    if parts[last].chars().all(|c| c.is_ascii_digit()) {
        if last == 0 {
            return None;
        }
        ext_idx = last - 1;
    } else if last >= 2 && parts[last - 1].chars().all(|c| c.is_ascii_digit()) {
        ext_idx = last - 2;
    }

    if ext_idx == 0 {
        return None;
    }
    let ext = parts[ext_idx].trim();
    if ext.is_empty() || ext.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

fn unpack_with_pak<R>(
    pak: PakFile<R>,
    output_path: &Path,
    file_name_table: Arc<FileNameTable>,
    filters: Arc<Vec<Regex>>,
    bar: ProgressBar,
    cmd: &UnpackCommand,
) -> color_eyre::Result<ree_pak_core::extract::ExtractReport>
where
    R: PakReader,
{
    let report = pak
        .extractor(output_path)
        .file_name_table_arc(file_name_table)
        .skip_unknown(cmd.skip_unknown)
        .overwrite(cmd.r#override)
        .continue_on_error(cmd.ignore_error)
        .filter({
            let filters = Arc::clone(&filters);
            move |_entry, path| {
                if filters.is_empty() {
                    return true;
                }
                let Some(path) = path else { return false };
                filters.iter().any(|f| f.is_match(path))
            }
        })
        .on_event({
            let bar = bar.clone();
            move |event| match event {
                ExtractEvent::Start { total } => bar.set_length(total as u64),
                ExtractEvent::FileDone { error, .. } => {
                    if let Some(error) = error {
                        bar.println(format!("Error: {error}"));
                    }
                    bar.inc(1);
                }
                ExtractEvent::Finish { .. } => bar.finish(),
                _ => {}
            }
        })
        .run()?;
    Ok(report)
}

#[derive(Clone)]
struct MmapReader {
    mmap: Arc<Mmap>,
    pos: u64,
}

impl MmapReader {
    fn new(mmap: Mmap) -> Self {
        Self {
            mmap: Arc::new(mmap),
            pos: 0,
        }
    }

    fn len(&self) -> u64 {
        self.mmap.len() as u64
    }
}

impl Read for MmapReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = self.len();
        if self.pos >= len {
            return Ok(0);
        }
        let remaining = (len - self.pos) as usize;
        let to_read = buf.len().min(remaining);
        let start = self.pos as usize;
        let end = start + to_read;
        buf[..to_read].copy_from_slice(&self.mmap[start..end]);
        self.pos += to_read as u64;
        Ok(to_read)
    }
}

impl Seek for MmapReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let len = self.len() as i128;
        let cur = self.pos as i128;
        let next = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(off) => len.saturating_add(off as i128),
            SeekFrom::Current(off) => cur.saturating_add(off as i128),
        };
        if next < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek to a negative offset",
            ));
        }
        let next_u64 =
            u64::try_from(next).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek overflow"))?;
        if next_u64 > self.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid seek beyond end of mmap",
            ));
        }
        self.pos = next_u64;
        Ok(self.pos)
    }
}

fn output_path<P: AsRef<Path>>(output: &Option<String>, input: P) -> PathBuf {
    if let Some(output) = &output {
        // specified output directory
        output.into()
    } else if let Some(parent) = input.as_ref().parent() {
        // relative to input directory
        let dir_name = input
            .as_ref()
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or("output".to_string());
        parent.join(dir_name).to_string_lossy().to_string().into()
    } else {
        // current directory
        ".".into()
    }
}

fn load_filename_table(project_name_or_path: &str) -> color_eyre::Result<FileNameTable> {
    // try to load as file path
    let path = Path::new(project_name_or_path);
    if path.exists() {
        let path_abs = path.canonicalize().context("Failed to get absolute path")?;
        return FileNameTable::from_list_file(path_abs).context("Failed to load file name table");
    }

    let parent_paths = [std::env::current_dir()?, std::env::current_exe()?];
    let rel_paths = [
        format!("assets/filelist/{}.list", project_name_or_path),
        format!("assets/filelist/{}.list.zst", project_name_or_path),
    ];

    let mut path_abs = None;
    for parent_path in &parent_paths {
        for rel_path in &rel_paths {
            let p = parent_path.join(rel_path);
            if p.is_file() {
                path_abs = Some(p);
                break;
            }
        }
    }

    if let Some(path_abs) = path_abs {
        FileNameTable::from_list_file(path_abs).context("Failed to load file name table")
    } else {
        eyre::bail!(
            "Project file `{}` not found in assets/filelist, check your project name.",
            project_name_or_path
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_path_extension_for_check_handles_numeric_suffix() {
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex.20260213")),
            Some("tex".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.TEX.20260213")),
            Some("tex".to_string())
        );
        assert_eq!(logical_path_extension_for_check(Path::new("name.20260213")), None);
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex")),
            Some("tex".to_string())
        );
        assert_eq!(logical_path_extension_for_check(Path::new("name")), None);
    }

    #[test]
    fn logical_path_extension_for_check_handles_version_and_optional_region_suffix() {
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex.ru")),
            Some("ru".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex.es419")),
            Some("es419".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.user.251111100.es419")),
            Some("user".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex.251111100.ZhCn")),
            Some("tex".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.user.251111100.enUS")),
            Some("user".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("name.tex.251111100.ru")),
            Some("tex".to_string())
        );
    }
}
