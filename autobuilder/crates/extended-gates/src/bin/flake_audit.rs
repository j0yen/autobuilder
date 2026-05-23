//! Binary entry for the `flake-audit` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "flake-audit", about = "cargo test rerun K times produces identical outcomes")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("flake-audit", &args.project)?;
    println!("{summary}");
    Ok(())
}
