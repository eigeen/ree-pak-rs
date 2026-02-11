use std::env;

use clap::{Args, Parser, Subcommand, ValueEnum};

mod pack;
mod unpack;

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
    /// Dump PAK information
    DumpInfo(DumpInfoCommand),
    /// Pack files into a PAK file
    Pack(PackCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliPakBackend {
    /// Use `memmap2` memory mapping (default).
    Mmap,
    /// Use regular file I/O (legacy mode).
    #[value(alias = "file")]
    Legacy,
}

impl Default for CliPakBackend {
    fn default() -> Self {
        Self::Mmap
    }
}

#[derive(Debug, Args)]
struct UnpackCommand {
    /// Game project name or list file path, e.g. "MHRS_PC_Demo", "./MHRS_PC_Demo.list"
    #[arg(short, long)]
    project: String,
    /// Input PAK file path
    #[arg(short, long)]
    input: String,
    /// Output directory path
    #[arg(short, long)]
    output: Option<String>,
    /// PAK reading backend. `legacy` uses regular file I/O; `mmap` uses memory mapping.
    #[arg(long, value_enum, default_value_t)]
    backend: CliPakBackend,
    /// Regex patterns to filter files to unpack by file path.
    #[arg(short, long)]
    filter: Vec<String>,
    /// Ignore errors during unpacking files
    #[arg(long, default_value = "false")]
    ignore_error: bool,
    /// Override existing files
    #[arg(long, default_value = "false")]
    r#override: bool,
    /// Skip files with an unknown path while unpacking
    #[arg(long, default_value = "false")]
    r#skip_unknown: bool,
}

#[derive(Debug, Args)]
struct DumpInfoCommand {
    /// Game project name, e.g. "MHRS_PC_Demo"
    #[arg(short, long)]
    project: String,
    /// Input PAK file path
    #[arg(short, long)]
    input: String,
    /// Output file path
    #[arg(short, long)]
    output: Option<String>,
    /// Override existing files
    #[arg(long, default_value = "false")]
    r#override: bool,
}

#[derive(Debug, Args)]
struct PackCommand {
    /// Input directory path.
    #[arg(short, long)]
    input: String,
    /// Output PAK file path
    #[arg(short, long)]
    output: Option<String>,
    /// Override existing file.
    #[arg(long, default_value = "false")]
    r#override: bool,
}

fn main() -> anyhow::Result<()> {
    // direct argumet mode for drap-and-drop
    let args = env::args().skip(1).collect::<Vec<String>>();
    if args.len() == 1 {
        // try to load as pack command with input directory
        let path = std::path::Path::new(&args[0]);
        if path.is_dir() {
            let cmd = PackCommand {
                input: args[0].to_string(),
                output: None,
                r#override: true,
            };
            pack::package(&cmd)?;
            return Ok(());
        }
    }

    let cli = Cli::parse();

    match &cli.command {
        Command::Unpack(cmd) => unpack::unpack_parallel(cmd),
        Command::DumpInfo(cmd) => unpack::dump_info(cmd),
        Command::Pack(cmd) => pack::package(cmd),
    }
}
