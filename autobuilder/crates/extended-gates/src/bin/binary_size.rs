//! Binary entry for the `binary-size` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "binary-size", about = "target/release/* under per-bin byte budgets")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("binary-size", &args.project)?;
    println!("{summary}");
    Ok(())
}
