//! Binary entry for the `semver-check` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "semver-check", about = "Pub-API diff HEAD~1 vs HEAD via syn")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("semver-check", &args.project)?;
    println!("{summary}");
    Ok(())
}
