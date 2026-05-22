//! Stage 4 — 7-receipt risk gate.
//!
//! Walks `target/autobuilder/receipts/{intake,vti-plan,proof-receipt,risk-gate,
//! reviewer-agent,rollback-plan,ci-checks}.json`, verifies each is present and
//! that its declared schema, `head_sha`, and `verdict` are consistent with the
//! current HEAD, then emits `target/autobuilder/release-receipt.json` with the
//! aggregated verdict. Exits non-zero on `block`.
//!
//! The gate does not re-run the underlying checks — it only attests to their
//! results being present, schema-valid, and HEAD-bound. The individual
//! producers (e.g. `autobuilder rollback-plan`, `autobuilder ci-checks`) own
//! the actual work and their own digest-binding.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing target/autobuilder/receipts/.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

struct ReceiptSpec {
    name: &'static str,
    file_name: ReceiptPath,
    expected_schema: &'static str,
    requires_head_match: bool,
    pass_verdicts: &'static [&'static str],
}

enum ReceiptPath {
    Static(&'static str),
    HeadShaJson,
}

const RECEIPT_SPECS: &[ReceiptSpec] = &[
    ReceiptSpec {
        name: "intake",
        file_name: ReceiptPath::Static("intake.json"),
        expected_schema: "autobuilder.intent_card.v1",
        requires_head_match: false,
        pass_verdicts: &[],
    },
    ReceiptSpec {
        name: "vti-plan",
        file_name: ReceiptPath::Static("vti-plan.json"),
        expected_schema: "autobuilder.vti_plan_receipt.v1",
        requires_head_match: true,
        pass_verdicts: &["pass"],
    },
    ReceiptSpec {
        name: "proof-receipt",
        file_name: ReceiptPath::HeadShaJson,
        expected_schema: "autobuilder.iteration_receipt.v1",
        requires_head_match: true,
        pass_verdicts: &["baseline", "advance"],
    },
    ReceiptSpec {
        name: "risk-gate",
        file_name: ReceiptPath::Static("risk-gate.json"),
        expected_schema: "autobuilder.bad_rust_audit.v1",
        requires_head_match: false,
        pass_verdicts: &[],
    },
    ReceiptSpec {
        name: "reviewer-agent",
        file_name: ReceiptPath::Static("reviewer-agent.json"),
        expected_schema: "autobuilder.reviewer_agent_receipt.v1",
        requires_head_match: true,
        pass_verdicts: &["pass", "concern"],
    },
    ReceiptSpec {
        name: "rollback-plan",
        file_name: ReceiptPath::Static("rollback-plan.json"),
        expected_schema: "autobuilder.rollback_plan_receipt.v1",
        requires_head_match: true,
        pass_verdicts: &["pass"],
    },
    ReceiptSpec {
        name: "ci-checks",
        file_name: ReceiptPath::Static("ci-checks.json"),
        expected_schema: "autobuilder.ci_checks_receipt.v1",
        requires_head_match: true,
        pass_verdicts: &["pass"],
    },
];

#[derive(Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)] // every bool is an independent observation surfaced in the receipt
struct ReceiptCheck {
    name: &'static str,
    path: String,
    present: bool,
    schema_expected: &'static str,
    schema_observed: Option<String>,
    schema_match: bool,
    head_sha_required: bool,
    head_sha_observed: Option<String>,
    head_sha_match: bool,
    verdict_observed: Option<String>,
    decision_observed: Option<String>,
    blocking_count_observed: Option<i64>,
    receipt_digest_observed: Option<String>,
    pass: bool,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReleaseReceipt {
    schema: &'static str,
    head_sha: String,
    verdict: &'static str,
    pass_count: usize,
    block_count: usize,
    checks: Vec<ReceiptCheck>,
    captured_at: String,
    receipt_digest: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let head_sha = git_rev_parse(&project, "HEAD")?;
    let receipts_dir = project.join("target/autobuilder/receipts");

    let mut checks: Vec<ReceiptCheck> = Vec::with_capacity(RECEIPT_SPECS.len());
    for spec in RECEIPT_SPECS {
        let file_name = match spec.file_name {
            ReceiptPath::Static(s) => s.to_owned(),
            ReceiptPath::HeadShaJson => format!("{head_sha}.json"),
        };
        let path = receipts_dir.join(&file_name);
        let check = check_receipt(spec, &path, &head_sha);
        checks.push(check);
    }

    let pass_count = checks.iter().filter(|c| c.pass).count();
    let block_count = checks.len() - pass_count;
    let verdict = if block_count == 0 { "pass" } else { "block" };

    let doc = ReleaseReceipt {
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

#[allow(clippy::too_many_lines)] // single linear receipt-decoding pipeline
fn check_receipt(spec: &ReceiptSpec, path: &Path, head_sha: &str) -> ReceiptCheck {
    let path_str = path.to_string_lossy().into_owned();
    let mut check = ReceiptCheck {
        name: spec.name,
        path: path_str,
        present: false,
        schema_expected: spec.expected_schema,
        schema_observed: None,
        schema_match: false,
        head_sha_required: spec.requires_head_match,
        head_sha_observed: None,
        head_sha_match: !spec.requires_head_match,
        verdict_observed: None,
        decision_observed: None,
        blocking_count_observed: None,
        receipt_digest_observed: None,
        pass: false,
        notes: Vec::new(),
    };

    let Ok(bytes) = fs::read(path) else {
        check.notes.push(format!("missing: {}", path.display()));
        return check;
    };
    check.present = true;
    if bytes.is_empty() {
        check.notes.push("file is empty".to_owned());
        return check;
    }

    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            check.notes.push(format!("invalid JSON: {e}"));
            return check;
        }
    };

    if let Some(s) = value.get("schema").and_then(|v| v.as_str()) {
        check.schema_observed = Some(s.to_owned());
        check.schema_match = s == spec.expected_schema;
        if !check.schema_match {
            check.notes.push(format!(
                "schema mismatch: expected {} got {s}",
                spec.expected_schema
            ));
        }
    } else {
        check
            .notes
            .push(format!("missing `schema` field; expected {}", spec.expected_schema));
    }

    if spec.requires_head_match {
        if let Some(h) = value.get("head_sha").and_then(|v| v.as_str()) {
            check.head_sha_observed = Some(h.to_owned());
            check.head_sha_match = h == head_sha;
            if !check.head_sha_match {
                check
                    .notes
                    .push(format!("head_sha mismatch: receipt={h} HEAD={head_sha}"));
            }
        } else {
            check.head_sha_match = false;
            check
                .notes
                .push("missing `head_sha` field (required for this receipt)".to_owned());
        }
    }

    check.receipt_digest_observed = value
        .get("receipt_digest")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let verdict = value.get("verdict").and_then(|v| v.as_str());
    let decision = value.get("decision").and_then(|v| v.as_str());
    let blocking_count = value
        .get("blocking_count")
        .and_then(serde_json::Value::as_i64);
    check.verdict_observed = verdict.map(str::to_owned);
    check.decision_observed = decision.map(str::to_owned);
    check.blocking_count_observed = blocking_count;

    let verdict_ok = check_verdict(spec, verdict, decision, blocking_count, &mut check.notes);

    check.pass = check.present
        && check.schema_match
        && check.head_sha_match
        && verdict_ok;
    check
}

fn check_verdict(
    spec: &ReceiptSpec,
    verdict: Option<&str>,
    decision: Option<&str>,
    blocking_count: Option<i64>,
    notes: &mut Vec<String>,
) -> bool {
    // The risk-gate / audit envelope uses `blocking_count` instead of a verdict.
    if spec.name == "risk-gate" {
        match blocking_count {
            Some(0) => true,
            Some(n) => {
                notes.push(format!("risk-gate has {n} blocking finding(s)"));
                false
            }
            None => {
                notes.push("risk-gate missing `blocking_count`".to_owned());
                false
            }
        }
    } else if spec.name == "reviewer-agent" {
        match decision {
            Some(d) if spec.pass_verdicts.contains(&d) => true,
            Some(d) => {
                notes.push(format!(
                    "reviewer decision={d} not in {:?}",
                    spec.pass_verdicts
                ));
                false
            }
            None => {
                notes.push("reviewer-agent missing `decision`".to_owned());
                false
            }
        }
    } else if spec.pass_verdicts.is_empty() {
        // intake — presence + schema is the contract; no verdict semantics yet.
        true
    } else {
        match verdict {
            Some(v) if spec.pass_verdicts.contains(&v) => true,
            Some(v) => {
                notes.push(format!(
                    "verdict={v} not in {:?}",
                    spec.pass_verdicts
                ));
                false
            }
            None => {
                notes.push("missing `verdict`".to_owned());
                false
            }
        }
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
