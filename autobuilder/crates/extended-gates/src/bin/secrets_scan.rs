//! Binary entry for the `secrets-scan` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "secrets-scan", about = "Scan tracked files for high-confidence secret patterns")]
struct Args {
    /// Project directory to scan.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("secrets-scan", &args.project)?;
    println!("{summary}");
    Ok(())
}
