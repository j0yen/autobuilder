//! Binary entry for the `mutation-kill` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "mutation-kill", about = "Trivial mutation operators caught by test suite")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("mutation-kill", &args.project)?;
    println!("{summary}");
    Ok(())
}
