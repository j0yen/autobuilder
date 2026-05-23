//! Binary entry for the `license-audit` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "license-audit", about = "Check every dep license is in the allowlist")]
struct Args {
    /// Project directory containing Cargo.lock.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("license-audit", &args.project)?;
    println!("{summary}");
    Ok(())
}
