//! AC-sbom.{1,2}: SBOM materializes on a clean Cargo.lock; emits a sensible
//! receipt even when the lock file has no packages (verdict=pass with 0
//! components — SBOM with zero deps is a valid SBOM, not a failure).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[test]
fn ac_sbom_1_happy_path() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_lock(
        project,
        &[
            ("anyhow", "1.0.0", Some("MIT OR Apache-2.0")),
            ("serde", "1.0.0", Some("MIT OR Apache-2.0")),
        ],
    );
    run_producer("sbom", project).unwrap();
    let v = read_receipt(project, "sbom-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    let count = v
        .get("components_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap();
    assert_eq!(count, 2);
    let bom = v.get("bom").unwrap();
    assert_eq!(
        bom.get("bomFormat").and_then(serde_json::Value::as_str),
        Some("CycloneDX")
    );
}

#[test]
fn ac_sbom_2_empty_lock_is_still_valid_sbom() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::write(project.join("Cargo.lock"), b"version = 3\n").unwrap();
    run_producer("sbom", project).unwrap();
    let v = read_receipt(project, "sbom-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(
        v.get("components_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
}
