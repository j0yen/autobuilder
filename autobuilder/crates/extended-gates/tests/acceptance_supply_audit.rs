//! AC-supply-audit.{1,2}: clean Cargo.lock against vendored advisories
//! passes; planted vulnerable dep blocks.
//!
//! Builds a minimal vendored RUSTSEC TOML in the fixture project so we
//! don't depend on the workspace's `vendor/rustsec/` having content.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn write_advisory(project: &std::path::Path, id: &str, package: &str, version: &str) {
    let dir = project.join("vendor/rustsec");
    std::fs::create_dir_all(&dir).unwrap();
    let toml = format!(
        "[advisory]\nid = \"{id}\"\npackage = \"{package}\"\nvulnerable_versions = [\"{version}\"]\n"
    );
    std::fs::write(dir.join(format!("{id}.toml")), toml).unwrap();
}

#[test]
fn ac_supply_audit_1_clean_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_lock(
        project,
        &[("anyhow", "1.0.0", None), ("serde", "1.0.0", None)],
    );
    // Advisory db with one advisory that won't match anything in the lock.
    write_advisory(project, "RUSTSEC-2099-9999", "some-other-crate", "0.0.0");
    run_producer("supply-audit", project).unwrap();
    let v = read_receipt(project, "supply-audit-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    let found = v
        .get("advisories_found")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(found.is_empty());
}

#[test]
fn ac_supply_audit_2_planted_cve_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_lock(
        project,
        &[
            ("safe-dep", "1.0.0", None),
            ("vulnerable-dep", "0.5.0", None),
        ],
    );
    write_advisory(project, "RUSTSEC-2099-1234", "vulnerable-dep", "0.5.0");
    run_producer("supply-audit", project).unwrap();
    let v = read_receipt(project, "supply-audit-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let found = v
        .get("advisories_found")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0]
            .get("advisory_id")
            .and_then(serde_json::Value::as_str),
        Some("RUSTSEC-2099-1234")
    );
    assert_eq!(
        found[0]
            .get("package")
            .and_then(serde_json::Value::as_str),
        Some("vulnerable-dep")
    );
}
