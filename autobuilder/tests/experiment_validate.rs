//! Integration tests for `autobuilder experiment validate`.
//!
//! Each test invokes the built binary against a fixture under
//! `tests/fixtures/experiment_validate/` and asserts exit code +
//! stderr message shape. The binary is a black box; we deliberately
//! do not depend on a library surface so the surface we test is the
//! one users actually invoke.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::process::Command;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn fixture(name: &str) -> String {
    format!("tests/fixtures/experiment_validate/{name}")
}

/// AC1 (MUST) — a valid manifest pointing to a valid intent-card exits 0,
/// emits "experiment validate: ... valid (slug=... slices=N)" on stdout.
#[test]
fn ac1_happy_path_exits_zero() {
    let out = autobuilder()
        .args(["experiment", "validate", "--manifest", &fixture("happy.toml")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}; stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("experiment validate:") && stdout.contains("slug=happy-fixture") && stdout.contains("slices=2"),
        "stdout missing summary; got: {stdout}"
    );
}

/// AC2 (MUST) — a manifest missing required top-level fields fails with
/// non-zero exit and surfaces the missing fields on stderr.
#[test]
fn ac2_missing_required_fields_fails() {
    let out = autobuilder()
        .args([
            "experiment",
            "validate",
            "--manifest",
            &fixture("missing_campaign.toml"),
        ])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected failure on missing campaign block"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("/: missing required field `campaign`"),
        "stderr did not surface missing campaign field: {stderr}"
    );
}

/// AC3 (MUST) — a slice transition outside the allowed set fails with
/// a message naming the offending value and the allowed list.
#[test]
fn ac3_bad_transition_fails() {
    let out = autobuilder()
        .args([
            "experiment",
            "validate",
            "--manifest",
            &fixture("bad_transition.toml"),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected failure on bad transition");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("/slices/0/transition")
            && stderr.contains("rebase-and-pray"),
        "stderr did not surface bad transition: {stderr}"
    );
}

/// AC4 (MUST) — a slice pointing to a missing intent_card file fails with
/// the resolved (absolute or manifest-relative) path in the error.
#[test]
fn ac4_missing_intent_card_fails() {
    let out = autobuilder()
        .args([
            "experiment",
            "validate",
            "--manifest",
            &fixture("missing_intent_card.toml"),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected failure on missing card");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("/slices/0/intent_card")
            && stderr.contains("does-not-exist.json"),
        "stderr did not surface missing intent_card: {stderr}"
    );
}
