//! Metric harness wrapper. Runs the per-project `scripts/run-metrics.sh`,
//! captures its output, normalizes against `autobuilder.metrics.v1`, and
//! writes `target/autobuilder/metrics.json`.
//!
//! This subcommand is the **first meta-PRD target** (PLAN.md Phase C):
//! autobuilder will dogfood the iterate-and-prove loop to build out the
//! full implementation of this subcommand against a written PRD.

use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing scripts/run-metrics.sh.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    unimplemented!(
        "autobuilder metric-harness: invoke <project>/scripts/run-metrics.sh, \
         parse its target/autobuilder/metrics.json, normalize against \
         schema autobuilder.metrics.v1, re-emit to stdout and to disk"
    )
}
