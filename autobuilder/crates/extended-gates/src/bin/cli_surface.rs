//! Binary entry for the `cli-surface` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cli-surface", about = "Every bin --help matches its snapshot")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("cli-surface", &args.project)?;
    println!("{summary}");
    Ok(())
}
