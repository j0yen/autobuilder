//! Binary entry for the `sbom` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "sbom", about = "Emit a CycloneDX-shape SBOM JSON of workspace deps")]
struct Args {
    /// Project directory containing Cargo.lock.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("sbom", &args.project)?;
    println!("{summary}");
    Ok(())
}
