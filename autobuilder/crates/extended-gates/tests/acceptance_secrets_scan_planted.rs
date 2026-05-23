//! AC-secrets-scan.2: planted secret in fixture is detected.
//!
//! Builds a tempdir with a single source file containing a synthetic
//! `AKIA` access-key pattern (not a real key — the regex matches both
//! real and synthetic). Runs the producer; asserts verdict=block and the
//! finding references the planted file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

use autobuilder_extended_gates::{ProducerSpec, run_producer};
use serde_json::Value;

fn init_git(dir: &std::path::Path) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["init", "-q", "-b", "main"])
        .status();
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e.com")
        .args(["commit", "-q", "--allow-empty", "-m", "init"])
        .status();
}

#[test]
fn ac_secrets_scan_2_planted_aws_key_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);

    // Synthetic AKIA key. The regex AKIA[0-9A-Z]{16} matches this exact
    // string; it is NOT a real AWS access key.
    std::fs::write(
        project.join("config.toml"),
        b"# leaked key in fixture\nkey = \"AKIAEXAMPLEKEYBLOCKED\"\n",
    )
    .unwrap();

    let summary = run_producer("secrets-scan", project).unwrap();
    assert!(summary.contains("findings"), "summary: {summary}");

    let spec = ProducerSpec::lookup("secrets-scan").unwrap();
    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(spec.file_name);
    let value: Value = serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();

    assert_eq!(
        value.get("verdict").and_then(Value::as_str),
        Some("block"),
        "planted secret should trigger verdict=block"
    );
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings array");
    assert!(!findings.is_empty(), "expected at least one finding");
    let any_config = findings.iter().any(|f| {
        f.get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| p.contains("config.toml"))
    });
    assert!(any_config, "expected a finding referencing config.toml");
}

#[test]
fn ac_secrets_scan_1_clean_fixture_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::write(project.join("README.md"), b"# clean\n").unwrap();

    run_producer("secrets-scan", project).unwrap();

    let spec = ProducerSpec::lookup("secrets-scan").unwrap();
    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(spec.file_name);
    let value: Value = serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
    assert_eq!(
        value.get("verdict").and_then(Value::as_str),
        Some("pass"),
        "clean fixture should pass"
    );
}
