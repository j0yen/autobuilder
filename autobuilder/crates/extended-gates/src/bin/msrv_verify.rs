//! Binary entry for the `msrv-verify` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "msrv-verify", about = "Declared rust-version actually cargo-checks clean")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("msrv-verify", &args.project)?;
    println!("{summary}");
    Ok(())
}
