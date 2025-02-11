use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use ree_pak_core::{filename::FileNameTable, pak::PakEntry, read::archive::PakArchiveReader};

use crate::UnpackCommand;

pub fn unpack_parallel(cmd: &UnpackCommand) -> anyhow::Result<()> {
    if cmd.ignore_error {
        unpack_parallel_error_continue(cmd)
    } else {
        unpack_parallel_error_terminate(cmd)
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

fn load_filename_table(project_name: &str) -> anyhow::Result<FileNameTable> {
    let parent_paths = [std::env::current_dir()?, std::env::current_exe()?];
    let rel_paths = [
        format!("assets/filelist/{}.list", project_name),
        format!("assets/filelist/{}.list.zst", project_name),
    ];

    let mut path_abs = None;
    for parent_path in &parent_paths {
        for rel_path in &rel_paths {
            let p = parent_path.join(rel_path);
            if p.exists() && p.is_file() {
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
            project_name
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
    let mut r = archive_reader.lock().unwrap();
    let mut entry_reader = (*r).owned_entry_reader(entry.clone())?;
    drop(r);

    // output file path
    let file_relative_path: PathBuf = file_name_table
        .get_file_name(entry.hash())
        .map(|fname| fname.get_name().to_string())
        .unwrap_or_else(|| format!("_Unknown/{:08X}", entry.hash()))
        .into();
    let filepath = output_path.join(file_relative_path);
    let filedir = filepath.parent().unwrap();

    if !filedir.exists() {
        std::fs::create_dir_all(filedir)?;
    }

    let mut data = vec![];
    std::io::copy(&mut entry_reader, &mut data)?;

    let mut file = if r#override {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&filepath)?
    } else {
        OpenOptions::new().create_new(true).write(true).open(&filepath)?
    };
    file.write_all(&data)?;

    // guess unknown file extension
    if filepath.extension().is_none() {
        if let Some(ext) = entry_reader.determine_extension() {
            let new_path = filepath.with_extension(ext);
            std::fs::rename(filepath, new_path)?;
        }
    }

    bar.inc(1);
    Ok(())
}

fn unpack_parallel_error_terminate(cmd: &UnpackCommand) -> anyhow::Result<()> {
    // load project file name table
    let file_name_table = load_filename_table(&cmd.project)?;

    // load PAK file
    let file = std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
    let mut reader = std::io::BufReader::new(file);
    let archive = ree_pak_core::read::read_archive(&mut reader)?;
    let archive_reader = Mutex::new(PakArchiveReader::new(reader, &archive));

    // output path
    let output_path = output_path(&cmd.output, &cmd.input);

    // extract files
    let bar = ProgressBar::new(archive.entries().len() as u64);
    bar.set_style(
        ProgressStyle::default_bar().template("{pos}/{len} files written {wide_bar} elapsed: {elapsed} eta: {eta}")?,
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.println(format!("Output directory: `{}`", output_path.display()));
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
                    "Error processing entry: {}. Path: {:?}\nEntry: {:?}",
                    e,
                    file_name_table.get_file_name(entry.hash()).unwrap(),
                    entry
                ))
            };
            result
        })?;

    bar.finish();
    println!("Done.");

    Ok(())
}

fn unpack_parallel_error_continue(cmd: &UnpackCommand) -> anyhow::Result<()> {
    // load project file name table
    let file_name_table = load_filename_table(&cmd.project)?;

    // load PAK file
    let file = std::fs::File::open(&cmd.input).context(format!("Input file `{}` not found.", &cmd.input))?;
    let mut reader = std::io::BufReader::new(file);
    let archive = ree_pak_core::read::read_archive(&mut reader)?;
    let archive_reader = Mutex::new(PakArchiveReader::new(reader, &archive));

    // output path
    let output_path = output_path(&cmd.output, &cmd.input);

    // extract files
    let bar = ProgressBar::new(archive.entries().len() as u64);
    bar.set_style(
        ProgressStyle::default_bar().template("{pos}/{len} files written {wide_bar} elapsed: {elapsed} eta: {eta}")?,
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.println(format!("Output directory: `{}`", output_path.display()));
    let results: Vec<anyhow::Result<()>> = archive
        .entries()
        .par_iter()
        .map(|entry| -> anyhow::Result<()> {
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
            };
            result
        })
        .collect();

    bar.finish();

    if !results.is_empty() {
        let errors = results.iter().filter(|r| r.is_err()).collect::<Vec<_>>();
        println!("Done with {} errors", errors.len());
        if errors.len() < 30 {
            println!("Errors: {:?}", errors);
        } else {
            println!("Too many errors to display.");
        }
    } else {
        println!("Done.");
    }

    Ok(())
}
