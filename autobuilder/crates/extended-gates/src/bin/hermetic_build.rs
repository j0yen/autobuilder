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

    /// Also attribute loopback connections from the build's own process
    /// tree (ignored by default because e.g. sccache's local protocol is
    /// not network egress). Unix-domain sockets are never examined, with
    /// or without this flag.
    #[arg(long)]
    strict: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    autobuilder_extended_gates::producers::hermetic_build::set_strict(args.strict);
    let summary = autobuilder_extended_gates::run_producer("hermetic-build", &args.project)?;
    println!("{summary}");
    Ok(())
}
