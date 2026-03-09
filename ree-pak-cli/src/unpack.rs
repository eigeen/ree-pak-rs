use std::io::{Read, Seek, SeekFrom};
use std::{path::Path, path::PathBuf, sync::Arc, time::Duration};

use color_eyre::eyre::{self, Context};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::{Mmap, MmapOptions};
use ree_pak_core::{
    PakReadOptions,
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
    let metadata = read::read_metadata_with_options(
        &mut reader,
        PakReadOptions {
            strict_feature_flags: cmd.strict_feature_flags,
        },
    )?;

    warn_unsupported_feature_flags(metadata.header().feature(), cmd.strict_feature_flags, |s| {
        eprintln!("{s}")
    });

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

    let input_paths = load_input_paths(cmd)?;

    // apply filter
    let filters = cmd
        .filter
        .iter()
        .filter(|f| !f.trim().is_empty())
        .map(|f| Regex::new(f))
        .collect::<Result<Vec<_>, _>>()?;
    let filters = Arc::new(filters);

    let batch_mode = input_paths.len() > 1;
    for (index, input_path) in input_paths.iter().enumerate() {
        if batch_mode {
            println!(
                "[{}/{}] Processing `{}`",
                index + 1,
                input_paths.len(),
                input_path.display()
            );
        }

        let output_path = output_path(&cmd.output, input_path, batch_mode);
        let report = unpack_one(
            cmd,
            input_path,
            &output_path,
            Arc::clone(&file_name_table),
            Arc::clone(&filters),
        )?;
        print_extract_report(&report);
    }

    Ok(())
}

fn unpack_one(
    cmd: &UnpackCommand,
    input_path: &Path,
    output_path: &Path,
    file_name_table: Arc<FileNameTable>,
    filters: Arc<Vec<Regex>>,
) -> color_eyre::Result<ree_pak_core::extract::ExtractReport> {
    // progress
    let bar = ProgressBar::new(0);
    bar.set_style(ProgressStyle::default_bar().template("{pos}/{len} files {wide_bar} elapsed: {elapsed} eta: {eta}")?);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.println(format!("Input file: `{}`", input_path.display()));
    if cmd.test {
        bar.println(format!(
            "Test mode (in memory): output directory ignored: `{}`",
            output_path.display()
        ));
    } else {
        bar.println(format!("Output directory: `{}`", output_path.display()));
    }

    // open pak (path wrapper kept in CLI)
    let read_options = PakReadOptions {
        strict_feature_flags: cmd.strict_feature_flags,
    };
    let report = if cmd.test {
        match cmd.backend {
            CliPakBackend::Legacy => {
                let file = std::fs::File::open(input_path)
                    .context(format!("Input file `{}` not found.", input_path.display()))?;
                let pak = PakFile::from_file_with_options(file, read_options)?;
                warn_unsupported_feature_flags(pak.metadata().header().feature(), cmd.strict_feature_flags, |s| {
                    bar.println(s)
                });
                test_with_pak(&pak, file_name_table, filters, bar.clone(), cmd)?
            }
            CliPakBackend::Mmap => {
                let file = std::fs::File::open(input_path)
                    .context(format!("Input file `{}` not found.", input_path.display()))?;
                // SAFETY: read-only mapping.
                let mmap = unsafe { MmapOptions::new().map(&file)? };
                let pak = PakFile::from_reader_with_options(MmapReader::new(mmap), read_options)?;
                warn_unsupported_feature_flags(pak.metadata().header().feature(), cmd.strict_feature_flags, |s| {
                    bar.println(s)
                });
                test_with_pak(&pak, file_name_table, filters, bar.clone(), cmd)?
            }
        }
    } else {
        match cmd.backend {
            CliPakBackend::Legacy => {
                let file = std::fs::File::open(input_path)
                    .context(format!("Input file `{}` not found.", input_path.display()))?;
                let pak = PakFile::from_file_with_options(file, read_options)?;
                warn_unsupported_feature_flags(pak.metadata().header().feature(), cmd.strict_feature_flags, |s| {
                    bar.println(s)
                });
                unpack_with_pak(pak, output_path, file_name_table, filters, bar.clone(), cmd)?
            }
            CliPakBackend::Mmap => {
                let file = std::fs::File::open(input_path)
                    .context(format!("Input file `{}` not found.", input_path.display()))?;
                // SAFETY: read-only mapping.
                let mmap = unsafe { MmapOptions::new().map(&file)? };
                let pak = PakFile::from_reader_with_options(MmapReader::new(mmap), read_options)?;
                warn_unsupported_feature_flags(pak.metadata().header().feature(), cmd.strict_feature_flags, |s| {
                    bar.println(s)
                });
                unpack_with_pak(pak, output_path, file_name_table, filters, bar.clone(), cmd)?
            }
        }
    };

    Ok(report)
}

fn print_extract_report(report: &ree_pak_core::extract::ExtractReport) {
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
}

fn warn_unsupported_feature_flags(feature: FeatureFlags, strict_feature_flags: bool, mut emit: impl FnMut(String)) {
    if strict_feature_flags {
        return;
    }
    let unsupported = feature.unsupported_bits();
    if unsupported == 0 {
        return;
    }
    let raw = feature.bits();
    emit(format!(
        "Warning: pak contains unsupported feature flags: raw=0x{raw:X} unsupported=0x{unsupported:X} (ignored)"
    ));
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
                    if !extensions_match_for_check(&path_ext, &detected_ext) {
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
    // - `name.suffix.version.arch.lang` (version is digits; arch is `X64`; lang is any non-empty string)
    if parts[last].chars().all(|c| c.is_ascii_digit()) {
        if last == 0 {
            return None;
        }
        ext_idx = last - 1;
    } else if last >= 2 && parts[last - 1].chars().all(|c| c.is_ascii_digit()) {
        ext_idx = last - 2;
    } else if last >= 3
        && parts[last - 2].chars().all(|c| c.is_ascii_digit())
        && parts[last - 1].eq_ignore_ascii_case("X64")
    {
        ext_idx = last - 3;
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

fn extensions_match_for_check(path_ext: &str, detected_ext: &str) -> bool {
    path_ext.eq_ignore_ascii_case(detected_ext)
        || extension_alias_group(path_ext).is_some_and(|group| group.contains(&detected_ext))
        || extension_alias_group(detected_ext).is_some_and(|group| group.contains(&path_ext))
}

fn extension_alias_group(ext: &str) -> Option<&'static [&'static str]> {
    const PCK_ALIASES: &[&str] = &["pck", "spck"];
    const BNK_ALIASES: &[&str] = &["bnk", "sbnk"];
    const SDFT_ALIASES: &[&str] = &["sdft", "sdftex"];
    const MESH_ALIASES: &[&str] = &["mesh", "mply"];

    match ext.to_ascii_lowercase().as_str() {
        "pck" | "spck" => Some(PCK_ALIASES),
        "bnk" | "sbnk" => Some(BNK_ALIASES),
        "sdft" | "sdftex" => Some(SDFT_ALIASES),
        "mesh" | "mply" => Some(MESH_ALIASES),
        _ => None,
    }
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

fn output_path<P: AsRef<Path>>(output: &Option<String>, input: P, batch_mode: bool) -> PathBuf {
    let input = input.as_ref();
    if let Some(output) = &output {
        let base = PathBuf::from(output);
        if batch_mode {
            base.join(default_output_dir_name(input))
        } else {
            // specified output directory
            base
        }
    } else if let Some(parent) = input.parent() {
        // relative to input directory
        parent.join(default_output_dir_name(input))
    } else {
        // current directory
        ".".into()
    }
}

fn default_output_dir_name<P: AsRef<Path>>(input: P) -> String {
    input
        .as_ref()
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or("output".to_string())
}

fn load_input_paths(cmd: &UnpackCommand) -> color_eyre::Result<Vec<PathBuf>> {
    if let Some(input) = &cmd.input {
        return Ok(vec![PathBuf::from(input)]);
    }

    let list_path = PathBuf::from(
        cmd.input_list
            .as_deref()
            .ok_or_else(|| eyre::eyre!("Either --input or --input-list must be provided."))?,
    );
    let content = std::fs::read_to_string(&list_path).context(format!(
        "Failed to read input list file `{}` as UTF-8 text.",
        list_path.display()
    ))?;
    let base_dir = list_path.parent().unwrap_or_else(|| Path::new("."));
    let paths = parse_input_list(&content, base_dir);
    if paths.is_empty() {
        eyre::bail!(
            "Input list file `{}` does not contain any pak paths.",
            list_path.display()
        );
    }
    Ok(paths)
}

fn parse_input_list(content: &str, base_dir: &Path) -> Vec<PathBuf> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix('\u{feff}').unwrap_or(line.trim());
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let path = PathBuf::from(line);
            Some(if path.is_absolute() { path } else { base_dir.join(path) })
        })
        .collect()
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

    #[test]
    fn logical_path_extension_for_check_handles_optional_arch_segment() {
        assert_eq!(
            logical_path_extension_for_check(Path::new("NPC102_00_001_04_00_ev.sbnk.1.Fr")),
            Some("sbnk".to_string())
        );
        assert_eq!(
            logical_path_extension_for_check(Path::new("NPC102_00_001_04_00_ev.sbnk.1.X64.Fr")),
            Some("sbnk".to_string())
        );
    }

    #[test]
    fn parse_input_list_supports_utf8_bom_relative_paths_and_comments() {
        let paths = parse_input_list(
            "\u{feff}a.pak\n\n# comment\nsub/b.pak\nC:\\games\\c.pak\n",
            Path::new("C:/lists"),
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("C:/lists").join("a.pak"),
                PathBuf::from("C:/lists").join("sub/b.pak"),
                PathBuf::from("C:\\games\\c.pak"),
            ]
        );
    }

    #[test]
    fn output_path_uses_subdirectories_in_batch_mode() {
        assert_eq!(
            output_path(
                &Some("C:/output".to_string()),
                Path::new("D:/mods/re_chunk_000.pak"),
                true
            ),
            PathBuf::from("C:/output").join("re_chunk_000")
        );
        assert_eq!(
            output_path(
                &Some("C:/output".to_string()),
                Path::new("D:/mods/re_chunk_000.pak"),
                false
            ),
            PathBuf::from("C:/output")
        );
    }
}
