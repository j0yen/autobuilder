//! AC-semver-check.{1,2}: identical API → patch (pass with patch expected);
//! removed pub fn → major (block when patch expected).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[test]
fn ac_semver_check_1_no_change_is_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("src/lib.rs"),
        "pub fn foo() {}\npub fn bar() -> u32 { 0 }\n",
    )
    .unwrap();
    std::fs::write(
        project.join("extended-gates.toml"),
        b"semver_expected_bump = \"patch\"\n",
    )
    .unwrap();
    commit_all(project, "v1");
    // No-op change to lib.rs (whitespace).
    std::fs::write(
        project.join("src/lib.rs"),
        "pub fn foo() {}\n\npub fn bar() -> u32 { 0 }\n",
    )
    .unwrap();
    commit_all(project, "v2");
    run_producer("semver-check", project).unwrap();
    let v = read_receipt(project, "semver-check-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(
        v.get("compatibility").and_then(serde_json::Value::as_str),
        Some("patch")
    );
}

#[test]
fn ac_semver_check_2_removed_pub_fn_is_major_blocks_under_patch() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("src/lib.rs"),
        "pub fn foo() {}\npub fn bar() -> u32 { 0 }\n",
    )
    .unwrap();
    std::fs::write(
        project.join("extended-gates.toml"),
        b"semver_expected_bump = \"patch\"\n",
    )
    .unwrap();
    commit_all(project, "v1");
    // Remove `bar`.
    std::fs::write(project.join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    commit_all(project, "v2");
    run_producer("semver-check", project).unwrap();
    let v = read_receipt(project, "semver-check-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    assert_eq!(
        v.get("compatibility").and_then(serde_json::Value::as_str),
        Some("major")
    );
    let breaking = v
        .get("breaking_changes")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(breaking.iter().any(|s| s
        .as_str()
        .is_some_and(|t| t.contains("bar"))));
}
