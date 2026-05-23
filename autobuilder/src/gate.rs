//! Stage 4 — 8-receipt risk gate (CLI subcommand wrapper).
//!
//! The pure-function core (`check_receipt_at`, `check_verdict`, `aggregate`,
//! `RECEIPT_SPECS`, `ReceiptCheck`, `ReleaseReceipt`) lives in
//! `crates/gate/` (autobuilder-gate) under its own intent-card
//! (`PRD-gate.md`) and an 8-AC adversarial suite (happy path, schema
//! mismatch, `head_sha` mismatch, verdict allowlist, risk-gate special-case,
//! permutation invariance, malformed-file handling, parent-repo integration).
//!
//! This module is the clap-dispatched shim that adds the orchestration
//! glue the lib intentionally does not own: clap Args, git rev-parse, file
//! IO over receipts/, the release-receipt write via autobuilder-receipt's
//! digest-binding write, and the printed pass/fail summary.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use autobuilder_gate as gate_lib;
use clap::Args as ClapArgs;

use crate::receipt;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing target/autobuilder/receipts/.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let receipts_dir = project.join("target/autobuilder/receipts");

    let mut checks = Vec::with_capacity(gate_lib::RECEIPT_SPECS.len());
    for spec in gate_lib::RECEIPT_SPECS {
        let file_name = match spec.file_name {
            gate_lib::ReceiptPath::Static(s) => s.to_owned(),
            gate_lib::ReceiptPath::HeadShaJson => format!("{head_sha}.json"),
        };
        let path = receipts_dir.join(&file_name);
        checks.push(gate_lib::check_receipt_at(spec, &path, &head_sha));
    }

    let (pass_count, block_count, verdict) = gate_lib::aggregate(&checks);

    let doc = gate_lib::ReleaseReceipt {
        schema: "autobuilder.release_receipt.v1",
        head_sha: head_sha.clone(),
        verdict,
        pass_count,
        block_count,
        checks,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let release_path = project.join("target/autobuilder/release-receipt.json");
    receipt::write(&release_path, value)?;

    println!(
        "gate: head={head_sha} receipts={} pass={pass_count} block={block_count} verdict={verdict}",
        doc.pass_count + doc.block_count
    );
    for c in &doc.checks {
        let status = if c.pass { "✓" } else { "✗" };
        let notes = if c.notes.is_empty() {
            String::new()
        } else {
            format!(" — {}", c.notes.join("; "))
        };
        println!("  {status} {}{notes}", c.name);
    }

    if verdict == "pass" {
        Ok(())
    } else {
        Err(anyhow!(
            "{block_count} of {} receipts failed; see {}",
            doc.pass_count + doc.block_count,
            release_path.display()
        ))
    }
}

fn git_rev_parse(project: &Path, refname: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", refname])
        .output()
        .with_context(|| format!("failed to spawn git rev-parse {refname}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git rev-parse {refname} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
