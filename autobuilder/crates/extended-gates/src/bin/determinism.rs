//! Binary entry for the `determinism` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "determinism", about = "Two cold cargo builds produce identical artifact sha256")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("determinism", &args.project)?;
    println!("{summary}");
    Ok(())
}
