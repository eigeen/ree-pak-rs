use clap::{Args, Parser, Subcommand};

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
}

#[derive(Debug, Args)]
struct UnpackCommand {
    /// Game project name, e.g. "MHRS_PC_Demo"
    #[clap(short, long)]
    project: String,
    /// Input PAK file path
    #[clap(short, long)]
    input: String,
    /// Output directory path
    #[clap(short, long)]
    output: Option<String>,
    /// Ignore errors during unpacking files
    #[clap(long, default_value = "false")]
    ignore_error: bool,
    /// Override existing files
    #[clap(long, default_value = "false")]
    r#override: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Unpack(cmd) => unpack::unpack_parallel(cmd),
    }
}
