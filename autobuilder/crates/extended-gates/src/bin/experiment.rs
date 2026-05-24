//! Binary entry for the `experiment` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "experiment", about = "Roll up a multi-slice campaign's outcomes into a digest-bound receipt")]
struct Args {
    /// Project directory containing target/autobuilder/experiment-outcomes.json.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("experiment", &args.project)?;
    println!("{summary}");
    Ok(())
}
