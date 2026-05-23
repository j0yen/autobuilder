//! Binary entry for the `schema-compat` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "schema-compat", about = "JSON-schema additive-only diff vs HEAD~1")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("schema-compat", &args.project)?;
    println!("{summary}");
    Ok(())
}
