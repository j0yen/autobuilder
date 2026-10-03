//! Binary entry for the `secrets-scan` producer.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "secrets-scan", about = "Scan tracked files for high-confidence secret patterns")]
struct Args {
    /// Project directory to scan.
    #[arg(long, default_value = ".")]
    project: PathBuf,

    /// Optional path to a manifest listing the files to scan, one path per
    /// line, relative to `--project` (blank lines and `#` comments
    /// ignored). When given, only the listed files are scanned instead of
    /// the whole project tree — directory-pruning (`target/`, `.git/`,
    /// `.venv/`, etc.) still applies to each listed path's components, and
    /// a listed path that doesn't exist is skipped rather than erroring.
    #[arg(long)]
    files_from: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let spec = autobuilder_extended_gates::ProducerSpec::lookup("secrets-scan")
        .context("secrets-scan is not registered in PRODUCER_SPECS")?;
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;
    let summary = autobuilder_extended_gates::producers::secrets_scan::run_with_files_from(
        spec,
        &project,
        args.files_from.as_deref(),
    )?;
    println!("{summary}");
    Ok(())
}
