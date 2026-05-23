//! AC-license-audit.{1,2}: every dep license is in allowlist (pass); planted
//! GPL violation blocks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[test]
fn ac_license_audit_1_clean_allowlist_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_lock(
        project,
        &[
            ("anyhow", "1.0.0", Some("MIT OR Apache-2.0")),
            ("serde", "1.0.0", Some("Apache-2.0")),
            ("regex", "1.0.0", Some("MIT")),
        ],
    );
    run_producer("license-audit", project).unwrap();
    let v = read_receipt(project, "license-audit-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    let violations = v
        .get("violations")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(violations.is_empty());
}

#[test]
fn ac_license_audit_2_planted_gpl_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_lock(
        project,
        &[
            ("safe-dep", "1.0.0", Some("MIT")),
            ("gpl-leaked-in", "0.5.0", Some("GPL-3.0")),
        ],
    );
    run_producer("license-audit", project).unwrap();
    let v = read_receipt(project, "license-audit-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let violations = v
        .get("violations")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(
        violations[0]
            .get("package")
            .and_then(serde_json::Value::as_str),
        Some("gpl-leaked-in")
    );
}
