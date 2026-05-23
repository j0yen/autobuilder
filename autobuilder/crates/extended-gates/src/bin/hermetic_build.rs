//! Binary entry for the `hermetic-build` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "hermetic-build", about = "Detect outbound sockets during `cargo build --offline` (Linux)")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("hermetic-build", &args.project)?;
    println!("{summary}");
    Ok(())
}
