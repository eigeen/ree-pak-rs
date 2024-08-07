use std::{path::Path, time::Duration};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use ree_pak_core::{
    filename::FileNameTable,
    pak::{Package, ProgressState},
};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Unpack a PAK file
    Unpack(UnpackCommand),
}

#[derive(Debug, Args)]
struct UnpackCommand {
    /// Game project name, e.g. "MHRS_PC_Demo"
    project: String,
    /// Input PAK file path
    input: String,
    /// Output directory path
    output: Option<String>,
}

fn unpack(cmd: &UnpackCommand) -> anyhow::Result<()> {
    // load project file name table
    let path_str = format!("assets/filelist/{}.list", cmd.project);
    let path = Path::new(&path_str);
    if !path.exists() || !path.is_file() {
        return Err(anyhow::anyhow!(
            "Project file `{}` not found, check your project name.",
            path_str
        ));
    }
    let file_name_table =
        FileNameTable::from_list_file(path).context("Failed to load file name table")?;
    // load PAK file
    let input_path = Path::new(&cmd.input);
    let reader = std::fs::File::open(input_path)
        .context(format!("Input file `{}` not found.", cmd.input))?;
    let mut reader = std::io::BufReader::new(reader);
    let mut pak = Package::from_reader(&mut reader)?;
    pak.set_file_name_table(file_name_table);
    // setup output
    let output_path = if let Some(output) = &cmd.output {
        // specified output directory
        output.clone()
    } else if let Some(parent) = input_path.parent() {
        // relative to input directory
        let dir_name = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or("output".to_string());
        parent.join(dir_name).to_string_lossy().to_string()
    } else {
        // current directory
        ".".to_string()
    };
    // extract files
    let bar = ProgressBar::new(pak.file_count() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{pos}/{len} files written {wide_bar}")
            .unwrap(),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.println(format!("Output directory: `{}`", output_path));
    let bar1 = bar.clone();
    pak.export_files(
        &output_path,
        &mut reader,
        Some(move |state| {
            if let ProgressState::Wrote(i) = state {
                bar1.set_position(i as u64);
            }
        }),
    )?;
    bar.println("Done.");
    bar.finish();

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Unpack(cmd) => unpack(cmd),
    }
}
