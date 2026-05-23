//! Binary entry for the `cold-build-time` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cold-build-time", about = "Clean cargo build --release wall-time under budget")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("cold-build-time", &args.project)?;
    println!("{summary}");
    Ok(())
}
