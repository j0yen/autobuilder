//! AC-schema-compat.{1,2}: additive-only diff passes; removed required
//! field blocks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn write_schema(project: &std::path::Path, json: &str) {
    let dir = project.join("schemas");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("thing.json"), json).unwrap();
}

#[test]
fn ac_schema_compat_1_additive_change_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_schema(
        project,
        r#"{"required":["a"],"properties":{"a":{"type":"string"}}}"#,
    );
    commit_all(project, "v1");
    // Add an optional property; keep all required fields.
    write_schema(
        project,
        r#"{"required":["a"],"properties":{"a":{"type":"string"},"b":{"type":"number"}}}"#,
    );
    commit_all(project, "v2");
    run_producer("schema-compat", project).unwrap();
    let v = read_receipt(project, "schema-compat-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
}

#[test]
fn ac_schema_compat_2_removed_required_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_schema(
        project,
        r#"{"required":["a","b"],"properties":{"a":{"type":"string"},"b":{"type":"number"}}}"#,
    );
    commit_all(project, "v1");
    // Remove "b" from required AND properties.
    write_schema(
        project,
        r#"{"required":["a"],"properties":{"a":{"type":"string"}}}"#,
    );
    commit_all(project, "v2");
    run_producer("schema-compat", project).unwrap();
    let v = read_receipt(project, "schema-compat-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let incompat = v
        .get("incompatibilities")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(!incompat.is_empty());
}
