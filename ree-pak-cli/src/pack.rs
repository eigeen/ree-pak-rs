use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use indexmap::IndexSet;
use ree_pak_core::write::{FileOptions, PakWriter};

use crate::PackCommand;

pub fn package(cmd: &PackCommand) -> anyhow::Result<()> {
    let input_paths = collect_inputs(&cmd.input)?;
    if input_paths.is_empty() {
        anyhow::bail!("No input files found");
    }

    // create output writer
    let output_path = cmd.output.as_ref().map(PathBuf::from).unwrap_or_else(|| {
        let input_dir = Path::new(&cmd.input);
        let input_dir_parent = Path::new(input_dir).parent().unwrap_or(Path::new("."));
        Path::new(input_dir_parent).join("re_chunk_000.pak.patch_999.pak")
    });
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut output_option = OpenOptions::new();
    if cmd.r#override {
        output_option.create(true).truncate(true);
    } else {
        output_option.create_new(true);
    }
    output_option.write(true);
    let output_file = output_option.open(&output_path)?;

    // package files
    let mut pak_writer = PakWriter::new(output_file, input_paths.len() as u64);
    for input_path in input_paths {
        // strip root dir before `natives/`
        let file_name = if !input_path.starts_with("natives/") {
            match input_path.find("natives/") {
                Some(index) => &input_path[index..],
                None => {
                    println!(
                        "Warning: input file '{}' does not contain 'natives/', check if it is a valid input file",
                        input_path
                    );
                    &input_path
                }
            }
        } else {
            &input_path
        };

        println!("Packing file: {}", file_name);
        let data = std::fs::read(&input_path)?;
        pak_writer.start_file(file_name, FileOptions::default())?;
        pak_writer.write_all(&data)?;
    }
    pak_writer.finish()?;

    println!("Output file: {}", output_path.display());
    println!("Done!");

    Ok(())
}

/// Collect input files in input directory into a single list of files.
fn collect_inputs(input_dir: impl AsRef<Path>) -> anyhow::Result<Vec<String>> {
    let mut files = IndexSet::new();

    let input_dir = input_dir.as_ref();
    if !input_dir.exists() {
        anyhow::bail!("Input directory does not exist: {}", input_dir.display());
    }

    for entry in walkdir::WalkDir::new(input_dir) {
        let entry = entry?;
        if entry.path().is_file() {
            files.insert(entry.path().to_string_lossy().replace('\\', "/"));
        }
    }

    Ok(files.into_iter().collect())
}
