use std::{path::Path, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use ree_pak_core::{
    extract::ExtractEvent,
    filename::FileNameTable,
    pak::FeatureFlags,
    pakfile::{PakBackend, PakFile},
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

pub fn dump_info(cmd: &DumpInfoCommand) -> anyhow::Result<()> {
    let filename_table = load_filename_table(&cmd.project)?;

    let file = std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
    let mut reader = std::io::BufReader::new(file);
    let archive = read::read_archive(&mut reader)?;

    let chunk_table = if archive.header().feature().contains(FeatureFlags::CHUNK_TABLE) {
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
        header: archive.header().clone(),
        chunk_table,
        entries: archive
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

pub fn unpack_parallel(cmd: &UnpackCommand) -> anyhow::Result<()> {
    // load project file name table
    let file_name_table = load_filename_table(&cmd.project)?;

    // output path
    let output_path = output_path(&cmd.output, &cmd.input);

    // open pak
    let backend = match cmd.backend {
        CliPakBackend::Mmap => PakBackend::Mmap,
        CliPakBackend::Legacy => PakBackend::File,
    };
    let pak = PakFile::builder()
        .backend(backend)
        .open(&cmd.input)
        .context(format!("Input file `{}` not found.", &cmd.input))?;

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
    bar.println(format!("Output directory: `{}`", output_path.display()));

    let report = pak
        .extractor(&output_path)
        .file_name_table(file_name_table)
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

fn load_filename_table(project_name_or_path: &str) -> anyhow::Result<FileNameTable> {
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
        anyhow::bail!(
            "Project file `{}` not found in assets/filelist, check your project name.",
            project_name_or_path
        );
    }
}
