//! Binary entry for the `bench-delta` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bench-delta", about = "Criterion benches vs frozen baseline")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("bench-delta", &args.project)?;
    println!("{summary}");
    Ok(())
}
