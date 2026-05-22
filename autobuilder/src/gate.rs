//! Stage 4 — Risk gate. Verifies all 7 receipts are present and
//! digest-bound to `HEAD`, then emits a `pass | block` verdict.
//!
//! Receipts checked: intake, vti-plan, proof-receipt, risk-gate,
//! reviewer-agent, rollback-plan, ci-checks. Missing or invalid →
//! block with a machine-readable diagnostic.

use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing target/autobuilder/receipts/.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    unimplemented!(
        "autobuilder gate: read target/autobuilder/receipts/{{intake,vti-plan,proof-receipt,risk-gate,reviewer-agent,rollback-plan,ci-checks}}.json, \
         verify each digest-binds to HEAD, validate each against its schema, \
         emit verdict JSON, exit non-zero on block"
    )
}
