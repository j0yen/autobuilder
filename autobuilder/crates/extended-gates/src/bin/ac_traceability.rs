//! Binary entry for the `ac-traceability` producer.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ac-traceability", about = "Every PRD AC id has \u{2265}1 test fn referencing it")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let summary = autobuilder_extended_gates::run_producer("ac-traceability", &args.project)?;
    println!("{summary}");
    Ok(())
}
