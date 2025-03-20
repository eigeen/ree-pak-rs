use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use indexmap::IndexSet;
use ree_pak_core::{
    filename::FileNameExt,
    write::{FileOptions, PakWriter},
};

use crate::PackCommand;

#[derive(Debug, Clone)]
enum FileName {
    Full(String),
    Hash(u64),
}

impl FileName {
    fn hash(&self) -> u64 {
        match self {
            FileName::Full(name) => name.hash_mixed(),
            FileName::Hash(hash) => hash.hash_mixed(),
        }
    }
}

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
        let file_name: FileName = if !input_path.starts_with("natives/") {
            if let Some(index) = input_path.find("natives/") {
                FileName::Full(input_path[index..].to_string())
            } else if let Some(unk_index) = input_path.find("_Unknown/") {
                get_unknown_file_name_hash(&input_path[unk_index..])
                    .map(FileName::Hash)
                    .unwrap_or_else(|| {
                        println!(
                            "Warning: failed to get hash of unknown file '{}', using full name instead",
                            input_path
                        );
                        FileName::Full(input_path.to_string())
                    })
            } else {
                println!(
                    "Warning: input file '{}' does not contain 'natives/', check if it is a valid input file",
                    input_path
                );
                FileName::Full(input_path.to_string())
            }
        } else {
            FileName::Full(input_path.to_string())
        };

        println!("Packing file: {:?}", file_name);
        let data = std::fs::read(&input_path)?;
        pak_writer.start_file(file_name.hash(), FileOptions::default())?;
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

fn get_unknown_file_name_hash(file_path: &str) -> Option<u64> {
    let file_stem = Path::new(file_path).file_stem()?.to_str()?;
    if let Some(stem) = file_stem.strip_prefix("0x") {
        u64::from_str_radix(stem, 16).ok()
    } else {
        u64::from_str_radix(file_stem, 16).ok()
    }
}
