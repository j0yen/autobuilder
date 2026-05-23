//! AC-binary-size.{1,2}: under-budget bins pass; planted oversized bin blocks.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn write_fake_bin(project: &std::path::Path, name: &str, bytes: usize) {
    let dir = project.join("target/release");
    std::fs::create_dir_all(&dir).unwrap();
    let payload = vec![0u8; bytes];
    std::fs::write(dir.join(name), payload).unwrap();
}

#[test]
fn ac_binary_size_1_under_budget_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_fake_bin(project, "tinybin", 1024);
    std::fs::write(
        project.join("extended-gates.toml"),
        b"default_max_bytes = 50000000\n",
    )
    .unwrap();
    run_producer("binary-size", project).unwrap();
    let v = read_receipt(project, "binary-size-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
}

#[test]
fn ac_binary_size_2_over_budget_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_fake_bin(project, "bloated", 10_000);
    std::fs::write(
        project.join("extended-gates.toml"),
        b"default_max_bytes = 100\n",
    )
    .unwrap();
    run_producer("binary-size", project).unwrap();
    let v = read_receipt(project, "binary-size-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let over = v
        .get("over_budget")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert_eq!(over.len(), 1);
    assert_eq!(
        over[0].as_str(),
        Some("bloated"),
        "planted oversized bin should be in over_budget list"
    );
}
