//! AC-ac-traceability.{1,2}: all PRD AC ids covered by a test → pass; one
//! uncovered → block + listed in `untraced_ac_ids`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[test]
fn ac_ac_traceability_1_all_covered_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::write(
        project.join("PRD-fixture.md"),
        "# PRD\n\nAC1 (MUST): foo.\nAC2 (MUST): bar.\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.join("tests")).unwrap();
    std::fs::write(
        project.join("tests/acceptance.rs"),
        "#[test] fn ac1_works() {} #[test] fn ac2_works() {}",
    )
    .unwrap();
    run_producer("ac-traceability", project).unwrap();
    let v = read_receipt(project, "ac-traceability-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    let untraced = v
        .get("untraced_ac_ids")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(untraced.is_empty());
}

#[test]
fn ac_ac_traceability_2_uncovered_ac_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::write(
        project.join("PRD-fixture.md"),
        "# PRD\n\nAC1 (MUST): foo.\nAC2 (MUST): bar.\nAC9 (MUST): orphan.\n",
    )
    .unwrap();
    std::fs::create_dir_all(project.join("tests")).unwrap();
    std::fs::write(
        project.join("tests/acceptance.rs"),
        "#[test] fn ac1_works() {} #[test] fn ac2_works() {}",
    )
    .unwrap();
    run_producer("ac-traceability", project).unwrap();
    let v = read_receipt(project, "ac-traceability-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let untraced = v
        .get("untraced_ac_ids")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(
        untraced.iter().any(|s| s.as_str() == Some("AC9")),
        "expected AC9 in untraced list"
    );
}
