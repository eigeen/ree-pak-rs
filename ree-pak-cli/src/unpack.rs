use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use parking_lot::Mutex;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ree_pak_core::{
    filename::FileNameTable,
    pak::{PakArchive, PakEntry},
    read::archive::PakArchiveReader,
};
use regex::Regex;
use serde::Serialize;

use crate::{DumpInfoCommand, UnpackCommand};

#[derive(Debug, Serialize)]
struct PakInfo {
    header: ree_pak_core::pak::PakHeader,
    entries: Vec<EntryWithPath>,
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
    let archive = ree_pak_core::read::read_archive(&mut reader)?;

    let info = PakInfo {
        header: archive.header().clone(),
        entries: archive
            .entries()
            .iter()
            .map(|entry| {
                let path = filename_table
                    .get_file_name(entry.hash())
                    .map(|fname| fname.get_name().to_string());
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

    // load PAK file
    let file = std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
    let mut reader = std::io::BufReader::new(file);
    let archive = ree_pak_core::read::read_archive(&mut reader)?;
    let archive = if !cmd.filter.is_empty() || cmd.skip_unknown {
        // apply filter
        let filters = cmd
            .filter
            .iter()
            .map(|f| Regex::new(f))
            .collect::<Result<Vec<_>, _>>()?;
        let entries = archive
            .entries()
            .iter()
            .filter(|&entry| {
                let file_name = file_name_table.get_file_name(entry.hash());
                match file_name {
                    Some(file_name) => {
                        if filters.is_empty() {
                            return true;
                        }
                        let file_name = file_name.get_name();
                        filters.iter().any(|f| f.is_match(file_name))
                    }
                    None => !cmd.skip_unknown,
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        PakArchive::new(archive.header().clone(), entries)
    } else {
        archive
    };

    let archive_reader = Mutex::new(PakArchiveReader::new(reader, &archive));

    // output path
    let output_path = output_path(&cmd.output, &cmd.input);

    // extract files
    let bar = ProgressBar::new(archive.entries().len() as u64);
    bar.set_style(ProgressStyle::default_bar().template("{pos}/{len} files {wide_bar} elapsed: {elapsed} eta: {eta}")?);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.println(format!("Output directory: `{}`", output_path.display()));

    let results: Mutex<Vec<anyhow::Result<()>>> = Mutex::new(vec![]);
    archive
        .entries()
        .par_iter()
        .try_for_each(|entry| -> anyhow::Result<()> {
            let result = process_entry(
                entry,
                &file_name_table,
                &output_path,
                &archive_reader,
                &bar,
                cmd.r#override,
            );
            if let Err(e) = &result {
                bar.println(format!(
                    "Error processing entry: {:#}. Path: {:?}\nEntry: {:?}",
                    e,
                    file_name_table.get_file_name(entry.hash()).unwrap(),
                    entry
                ));
                if cmd.ignore_error {
                    // ignore error and continue, save result
                    results.lock().push(result);
                    return Ok(());
                }
            };
            result
        })?;

    bar.finish();

    let results = results.into_inner();
    if !results.is_empty() {
        let errors = results.iter().filter(|r| r.is_err()).collect::<Vec<_>>();
        println!("Done with {} errors", errors.len());
        if errors.len() < 30 {
            println!("Errors: {:?}", errors);
        } else {
            println!("Errors: {:?}", &errors[0..30]);
            println!(
                "Displaying only the first 30 errors. Too many errors to display ({}).",
                errors.len()
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

fn process_entry(
    entry: &PakEntry,
    file_name_table: &FileNameTable,
    output_path: &Path,
    archive_reader: &Mutex<PakArchiveReader<BufReader<File>>>,
    bar: &ProgressBar,
    r#override: bool,
) -> anyhow::Result<()> {
    let mut entry_reader = {
        let mut r = archive_reader.lock();
        (*r).owned_entry_reader(entry.clone())?
    };

    // output file path
    let relative_path = file_name_table
        .get_file_name(entry.hash())
        .map(|fname| fname.get_name().to_string())
        .unwrap_or_else(|| format!("_Unknown/{:08X}", entry.hash()));
    let file_output_path = output_path.join(relative_path);
    let file_dir = file_output_path.parent().unwrap();

    if !file_dir.exists() {
        std::fs::create_dir_all(file_dir)?;
    }

    let mut data = vec![];
    std::io::copy(&mut entry_reader, &mut data)?;

    let mut open_options = OpenOptions::new();
    if r#override {
        open_options.create(true).write(true).truncate(true);
    } else {
        open_options.create_new(true).write(true);
    }
    let mut file = open_options.open(&file_output_path)?;
    file.write_all(&data)?;

    // guess unknown file extension
    if file_output_path.extension().is_none() {
        if let Some(ext) = entry_reader.determine_extension() {
            let new_path = file_output_path.with_extension(ext);
            std::fs::rename(file_output_path, new_path)?;
        }
    }

    bar.inc(1);
    Ok(())
}
