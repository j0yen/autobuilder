//! Binary entry for the `supply-audit` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "supply-audit", about = "Scan Cargo.lock against vendored RUSTSEC advisories")]
struct Args {
    /// Project directory containing Cargo.lock.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("supply-audit", &args.project)?;
    println!("{summary}");
    Ok(())
}
