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

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use autobuilder_gate as gate_lib;
use clap::Args as ClapArgs;

use crate::receipt;

/// Default floor (GiB) below which the receipt phase refuses to start.
/// Overridden by `AUTOBUILDER_RECEIPT_FREE_FLOOR_GB`.
const DEFAULT_FREE_FLOOR_GB: u64 = 5;

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

    // --- free-space pre-flight (PRD-autobuilder-receipt-write-verify
    // requirement 4) — refuse before touching a single receipt when the
    // destination filesystem is at or below the floor. This is a REFUSAL,
    // not a verdict: no release-receipt.json is written, so a caller
    // (extend-gate.sh) that only updates its verdict cache after seeing one
    // never sees this run at all. -----------------------------------------
    let floor_gb: u64 = env::var("AUTOBUILDER_RECEIPT_FREE_FLOOR_GB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FREE_FLOOR_GB);
    let free_gb = free_space_gb(&receipts_dir, &project)?;
    if free_gb < floor_gb {
        println!("gate  refused  (cause=disk-low free_gb={free_gb})");
        return Err(anyhow!(
            "refusing to start the receipt phase: {free_gb}GB free < {floor_gb}GB floor"
        ));
    }

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
    let unreadable = gate_lib::unreadable_receipts(&checks);

    let doc = gate_lib::ReleaseReceipt {
        schema: "autobuilder.release_receipt.v1",
        head_sha: head_sha.clone(),
        verdict,
        pass_count,
        block_count,
        checks,
        unreadable_receipts: unreadable,
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let release_path = project.join("target/autobuilder/release-receipt.json");
    receipt::write(&release_path, value)?;

    // Named unreadable state on the SAME summary line extend-gate.sh
    // captures as `$summary` (`grep -m1 '^gate: '`) and folds verbatim into
    // its own journal append — so `unreadable=<n> <first-name>: <cause>`
    // reaches the operator journal with no changes needed on that side
    // (requirement 3). `new_blocks` naming: every check, unreadable or
    // not, still prints its own `  ✗ <name> — ...` line below, which is
    // what gate-delta.sh's new_blocks parser keys on — an unreadable
    // receipt is never an empty name there either.
    let unreadable_suffix = match doc.unreadable_receipts.first() {
        Some(u) => format!(" {}: {}", u.name, u.cause),
        None => String::new(),
    };
    println!(
        "gate: head={head_sha} receipts={} pass={pass_count} block={block_count} verdict={verdict} unreadable={}{unreadable_suffix}",
        doc.pass_count + doc.block_count,
        doc.unreadable_receipts.len()
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

/// Free space, in whole GiB, on the filesystem backing `dir` (falling back
/// to `fallback_dir` when `dir` does not exist yet — a fresh project may
/// have no `target/autobuilder/receipts/` at all before its first gate).
///
/// `AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE` short-circuits to a fixed value
/// when set (a "fake free-space report", per the acceptance test) — the
/// same test-injection convention the build skill already uses elsewhere
/// (`EXTEND_GATE_JOURNAL`, `BURST_LANE_TEST`) rather than requiring a test
/// to actually fill a disk. Uses `df`, which itself reads `statvfs` (no
/// unsafe FFI needed in a crate that denies `unsafe_code`).
fn free_space_gb(dir: &Path, fallback_dir: &Path) -> Result<u64> {
    if let Ok(over) = env::var("AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE") {
        return over
            .trim()
            .parse()
            .with_context(|| format!("AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE={over} is not a u64"));
    }
    let probe = if dir.is_dir() { dir } else { fallback_dir };
    let output = Command::new("df")
        .args(["--output=avail", "-B1"])
        .arg(probe)
        .output()
        .with_context(|| format!("failed to spawn df on {}", probe.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "df {} failed: {}",
            probe.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let avail_bytes: u64 = stdout
        .lines()
        .nth(1)
        .map(str::trim)
        .with_context(|| format!("df {} produced no data row", probe.display()))?
        .parse()
        .with_context(|| format!("df {} avail column was not a number", probe.display()))?;
    Ok(avail_bytes / 1_000_000_000)
}
