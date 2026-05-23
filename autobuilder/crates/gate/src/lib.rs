//! Pure-function core of the autobuilder 8-receipt risk gate.
//!
//! Walks `target/autobuilder/receipts/{intake,vti-plan,proof-receipt,risk-gate,
//! reviewer-agent,rollback-plan,ci-checks,session-trace}.json`, verifies each
//! is present and that its declared schema, `head_sha`, and `verdict` are
//! consistent with the current HEAD, and aggregates pass/block counts into a
//! release-receipt verdict.
//!
//! Public surface:
//!
//! - [`RECEIPT_SPECS`] — the 8 receipt specs, byte-identical to the in-tree gate.
//! - [`ReceiptSpec`], [`ReceiptPath`], [`ReceiptCheck`], [`ReleaseReceipt`] — types.
//! - [`check_receipt_value`] — pure: parse + validate one receipt JSON.
//! - [`check_receipt_at`] — I/O wrapper that reads a file then calls [`check_receipt_value`].
//! - [`check_verdict`] — pure: per-spec verdict allowlist + special-cases.
//! - [`aggregate`] — collapse a slice of checks into pass/block counts + verdict.
//!
//! The bin's `autobuilder gate` subcommand stays as a thin orchestrator that
//! wraps clap Args + git rev-parse + file IO around these primitives.

#![cfg_attr(not(test), forbid(unsafe_code))]

use std::fs;
use std::path::Path;

use serde::Serialize;

/// One receipt the gate walks.
#[derive(Debug, Clone, Copy)]
pub struct ReceiptSpec {
    /// Human name (e.g. `"intake"`, `"reviewer-agent"`).
    pub name: &'static str,
    /// How to compute the on-disk filename inside `receipts/`.
    pub file_name: ReceiptPath,
    /// The `"schema"` field the receipt JSON must declare verbatim.
    pub expected_schema: &'static str,
    /// True if the receipt's `head_sha` must equal the current HEAD.
    pub requires_head_match: bool,
    /// Verdict strings that count as passing. Empty means "presence + schema
    /// is the whole contract" (intake-style).
    pub pass_verdicts: &'static [&'static str],
}

/// Filename strategy for a receipt.
#[derive(Debug, Clone, Copy)]
pub enum ReceiptPath {
    /// Filename is a fixed string (e.g. `"intake.json"`).
    Static(&'static str),
    /// Filename is `<head_sha>.json` (used for proof-receipt).
    HeadShaJson,
}

/// The 8 receipts the gate walks, in order. Byte-identical to the in-tree
/// `autobuilder/src/gate.rs` table; the table is the contract.
pub const RECEIPT_SPECS: &[ReceiptSpec] = &[
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
    ReceiptSpec {
        name: "session-trace",
        file_name: ReceiptPath::Static("session-trace.json"),
        expected_schema: "autobuilder.session_trace_receipt.v1",
        requires_head_match: true,
        // `pass` = trace ran, no constraint violations.
        // `skipped` = tracer unavailable on the host; receipt still present
        // and digest-bound, but the gate must not reject the iteration for
        // an environment limitation.
        pass_verdicts: &["pass", "skipped"],
    },
];

/// Per-receipt observation surfaced in the release-receipt.
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ReceiptCheck {
    /// Receipt name (e.g. `"intake"`).
    pub name: &'static str,
    /// Path the gate looked at (string form for serialization).
    pub path: String,
    /// True if the file existed and was readable.
    pub present: bool,
    /// The schema string the spec expected.
    pub schema_expected: &'static str,
    /// The schema string observed in the receipt JSON, if any.
    pub schema_observed: Option<String>,
    /// True iff `schema_observed == Some(schema_expected)`.
    pub schema_match: bool,
    /// True if the spec required `head_sha` to match HEAD.
    pub head_sha_required: bool,
    /// The `head_sha` string observed in the receipt JSON, if any.
    pub head_sha_observed: Option<String>,
    /// True iff `head_sha` is not required, or it is required and matches.
    pub head_sha_match: bool,
    /// The `verdict` field observed in the receipt JSON, if any.
    pub verdict_observed: Option<String>,
    /// The `decision` field observed (reviewer-agent specific).
    pub decision_observed: Option<String>,
    /// The `blocking_count` field observed (risk-gate specific).
    pub blocking_count_observed: Option<i64>,
    /// The `receipt_digest` field observed (informational; not validated here).
    pub receipt_digest_observed: Option<String>,
    /// Aggregate of all per-field checks above.
    pub pass: bool,
    /// Per-check diagnostic notes (joined into the printed output).
    pub notes: Vec<String>,
}

/// The top-level release-receipt envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseReceipt {
    /// Always `"autobuilder.release_receipt.v1"`.
    pub schema: &'static str,
    /// HEAD sha at the time the gate ran.
    pub head_sha: String,
    /// Aggregate verdict: `"pass"` iff every check.pass is true.
    pub verdict: &'static str,
    /// Number of checks with `pass=true`.
    pub pass_count: usize,
    /// Number of checks with `pass=false`.
    pub block_count: usize,
    /// Per-receipt detail.
    pub checks: Vec<ReceiptCheck>,
    /// RFC3339 UTC timestamp.
    pub captured_at: String,
    /// sha256 self-binding digest (populated by `autobuilder_receipt::write`).
    pub receipt_digest: String,
}

/// Pure: validate one receipt JSON against its spec at the given HEAD sha.
///
/// Returns a `ReceiptCheck` describing every observation. Does not perform
/// file IO; use [`check_receipt_at`] for the IO-wrapped variant.
#[must_use]
pub fn check_receipt_value(
    spec: &ReceiptSpec,
    value: &serde_json::Value,
    head_sha: &str,
) -> ReceiptCheck {
    let mut check = ReceiptCheck {
        name: spec.name,
        path: String::new(),
        present: true,
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

    if let Some(s) = value.get("schema").and_then(serde_json::Value::as_str) {
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
        if let Some(h) = value.get("head_sha").and_then(serde_json::Value::as_str) {
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
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);

    let verdict = value.get("verdict").and_then(serde_json::Value::as_str);
    let decision = value.get("decision").and_then(serde_json::Value::as_str);
    let blocking_count = value
        .get("blocking_count")
        .and_then(serde_json::Value::as_i64);
    check.verdict_observed = verdict.map(str::to_owned);
    check.decision_observed = decision.map(str::to_owned);
    check.blocking_count_observed = blocking_count;

    let verdict_ok = check_verdict(spec, verdict, decision, blocking_count, &mut check.notes);

    check.pass = check.present && check.schema_match && check.head_sha_match && verdict_ok;
    check
}

/// IO-wrapped: read the file at `path`, hand bytes to [`check_receipt_value`].
///
/// Returns a `ReceiptCheck` with `present=false` if the file is missing,
/// empty, or unparseable. Never panics.
#[must_use]
pub fn check_receipt_at(spec: &ReceiptSpec, path: &Path, head_sha: &str) -> ReceiptCheck {
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

    let mut from_value = check_receipt_value(spec, &value, head_sha);
    from_value.path = check.path;
    from_value
}

/// Pure: per-spec verdict allowlist + special-cases for risk-gate / reviewer-agent.
pub fn check_verdict(
    spec: &ReceiptSpec,
    verdict: Option<&str>,
    decision: Option<&str>,
    blocking_count: Option<i64>,
    notes: &mut Vec<String>,
) -> bool {
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
        true
    } else {
        match verdict {
            Some(v) if spec.pass_verdicts.contains(&v) => true,
            Some(v) => {
                notes.push(format!("verdict={v} not in {:?}", spec.pass_verdicts));
                false
            }
            None => {
                notes.push("missing `verdict`".to_owned());
                false
            }
        }
    }
}

/// Pure: collapse a slice of checks into `(pass_count, block_count, verdict)`.
///
/// Verdict is `"pass"` iff every check's `.pass` is true; otherwise
/// `"block"`. Permutation-invariant over the input slice.
#[must_use]
pub fn aggregate(checks: &[ReceiptCheck]) -> (usize, usize, &'static str) {
    let pass = checks.iter().filter(|c| c.pass).count();
    let block = checks.len().saturating_sub(pass);
    let verdict = if block == 0 { "pass" } else { "block" };
    (pass, block, verdict)
}
