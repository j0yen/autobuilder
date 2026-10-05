//! Binary entry for the `flake-audit` producer.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "flake-audit", about = "cargo test rerun K times produces identical outcomes")]
struct Args {
    /// Project directory to audit.
    #[arg(long, default_value = ".")]
    project: PathBuf,

    /// Override the configured run count (1..=20); runs exactly this many
    /// `cargo test` passes instead of the `extended-gates.toml`
    /// `flake_audit_runs` default.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=20))]
    passes: Option<u8>,

    /// Restrict the run to the test binaries whose sources changed in
    /// `<base>..HEAD` inside `--project`. Falls back to the full suite if
    /// any changed path lies outside `tests/`.
    #[arg(long)]
    only_changed_since: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let spec = autobuilder_extended_gates::ProducerSpec::lookup("flake-audit")
        .ok_or_else(|| anyhow!("flake-audit is not registered in PRODUCER_SPECS"))?;
    let summary = autobuilder_extended_gates::producers::flake_audit::run_with_options(
        spec,
        &args.project,
        args.passes,
        args.only_changed_since.as_deref(),
    )?;
    println!("{summary}");
    Ok(())
}
