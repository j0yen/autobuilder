//! AC-secrets-scan.3/4 (PRD-autobuilder-secrets-scan-allowlist): the
//! `extended-gates.toml::secrets_scan_allowlist` config-driven skip
//! mechanism, and its control check.
//!
//! Two isolated tempdir projects:
//!   - `ac_secrets_scan_control_outside_allowlist_blocks`: a synthetic
//!     secret planted at a path NOT covered by any allowlist entry still
//!     yields verdict=block — the missing control check PRD-autobuilder-
//!     gate-debt's original AC3 called for.
//!   - `ac_secrets_scan_allowlisted_path_passes`: the same synthetic secret,
//!     planted at a path an `extended-gates.toml::secrets_scan_allowlist`
//!     glob covers, yields verdict=pass, and `files_scanned` still counts
//!     the allowlisted file (skipped-for-secrets, not skipped-for-existence).

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

fn read_receipt(project: &std::path::Path) -> Value {
    let spec = ProducerSpec::lookup("secrets-scan").unwrap();
    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(spec.file_name);
    serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap()
}

/// Build a synthetic secret at runtime rather than as one contiguous string
/// literal. This file lives in the real, git-tracked crate tree scanned by
/// every real `secrets-scan` run against this repo — the exact same
/// self-referential false positive this PRD fixes for
/// `acceptance_secrets_scan_planted.rs` would recur here if `AKIA` plus 16
/// pattern-matching characters ever appeared contiguously in THIS file's own
/// source text, defeating the control check's own point (a real secret
/// caught outside any allowlist, not another planted fixture needing one).
fn synth_secret(tail: &str) -> String {
    format!("AKIA{tail}")
}

#[test]
fn ac_secrets_scan_control_outside_allowlist_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);

    // extended-gates.toml allowlists a DIFFERENT path than the one the
    // secret is planted at — the allowlist exists but does not cover this
    // file, so the scan must still block.
    std::fs::write(
        project.join("extended-gates.toml"),
        b"secrets_scan_allowlist = [\"some/other/allowlisted.rs\"]\n",
    )
    .unwrap();
    let secret = synth_secret("CONTROLNOTALLOWED12");
    let content = format!("// not in any allowlist\nlet key = \"{secret}\";\n");
    std::fs::write(project.join("not_allowlisted.rs"), content.as_bytes()).unwrap();

    run_producer("secrets-scan", project).unwrap();
    let value = read_receipt(project);

    assert_eq!(
        value.get("verdict").and_then(Value::as_str),
        Some("block"),
        "a secret outside any allowlist entry must still block"
    );
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings array");
    assert!(
        findings
            .iter()
            .any(|f| f
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|p| p.contains("not_allowlisted.rs"))),
        "expected a finding referencing not_allowlisted.rs, got {findings:?}"
    );
}

#[test]
fn ac_secrets_scan_allowlisted_path_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);

    std::fs::write(
        project.join("extended-gates.toml"),
        b"secrets_scan_allowlist = [\"fixtures/leaked.rs\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.join("fixtures")).unwrap();
    let secret = synth_secret("ALLOWEDPATHSKIPD1234");
    let content = format!("// planted, but allowlisted\nlet key = \"{secret}\";\n");
    std::fs::write(project.join("fixtures/leaked.rs"), content.as_bytes()).unwrap();
    // A second, non-allowlisted clean file so files_scanned counts > 1.
    std::fs::write(project.join("README.md"), b"# clean\n").unwrap();

    run_producer("secrets-scan", project).unwrap();
    let value = read_receipt(project);

    assert_eq!(
        value.get("verdict").and_then(Value::as_str),
        Some("pass"),
        "a secret at an allowlisted path must not block"
    );
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .expect("findings array");
    assert!(
        findings.is_empty(),
        "allowlisted file must not appear in findings, got {findings:?}"
    );
    // Skipped-for-secrets, not skipped-for-existence: the allowlisted file
    // is still counted as scanned (plus the extended-gates.toml itself and
    // README.md).
    let files_scanned = value
        .get("files_scanned")
        .and_then(Value::as_u64)
        .expect("files_scanned");
    assert!(
        files_scanned >= 2,
        "expected the allowlisted file to still count toward files_scanned, got {files_scanned}"
    );
}
