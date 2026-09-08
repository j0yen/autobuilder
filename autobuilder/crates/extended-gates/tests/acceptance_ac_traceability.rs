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

/// PRD-build-extend-gate-nested-crate-project: when `--project` is resolved
/// to a nested Cargo root (rustbuild's own `autobuilder/` layout), the PRD
/// file lives one level up at the repo root, not inside `project` itself.
/// `locate_prd` must fall back to the parent directory instead of blocking
/// with "no PRD-*.md found in project root".
#[test]
fn ac_ac_traceability_3_nested_project_finds_prd_in_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path();
    init_git(repo_root);
    std::fs::write(
        repo_root.join("PRD-fixture.md"),
        "# PRD\n\nAC1 (MUST): foo.\nAC2 (MUST): bar.\n",
    )
    .unwrap();
    let nested = repo_root.join("autobuilder");
    std::fs::create_dir_all(nested.join("tests")).unwrap();
    std::fs::write(
        nested.join("tests/acceptance.rs"),
        "#[test] fn ac1_works() {} #[test] fn ac2_works() {}",
    )
    .unwrap();
    run_producer("ac-traceability", &nested).unwrap();
    let v = read_receipt(&nested, "ac-traceability-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(
        v.get("prd_path").and_then(serde_json::Value::as_str),
        repo_root.join("PRD-fixture.md").to_str()
    );
}
