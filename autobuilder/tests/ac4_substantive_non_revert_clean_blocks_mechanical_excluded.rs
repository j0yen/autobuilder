//! AC4 (PRD-autobuilder-rollback-mechanical-commits): a fixture with one
//! substantive non-revert-clean commit (an ordinary `src/` change unrelated
//! to any of the three mechanical patterns) plus several non-revert-clean
//! mechanical commits must yield `verdict=block` and `blocking_count=1` —
//! the mechanical ones are excluded, the substantive one still blocks.
//! Proves the fix narrows scope without disabling the check.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::indexing_slicing
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "-q",
            "-m",
            msg,
        ],
    );
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn ac4_substantive_non_revert_clean_blocks_mechanical_excluded() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(project.join("agent")).unwrap();
    fs::write(project.join("src/lib.rs"), "1\n").unwrap();
    fs::write(project.join("agent/intent-card.json"), "v0\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // Substantive commit #1 — an ordinary src/ change, unrelated to any
    // mechanical pattern.
    fs::write(project.join("src/lib.rs"), "2\n").unwrap();
    commit_all(project, "src: initial helper");

    // Mechanical commit #1 (intent-card refresh).
    fs::write(project.join("agent/intent-card.json"), "v1\n").unwrap();
    commit_all(project, "agent: refresh intent card for foo");

    // Mechanical commit #2 — supersedes mechanical #1's file, making it
    // non-revert-clean.
    fs::write(project.join("agent/intent-card.json"), "v2\n").unwrap();
    commit_all(project, "agent: refresh intent card for bar");

    // Substantive commit #2 — supersedes substantive #1's line, making it
    // non-revert-clean.
    fs::write(project.join("src/lib.rs"), "3\n").unwrap();
    commit_all(project, "src: update helper");

    let out = autobuilder()
        .args([
            "rollback-plan",
            "--project",
            project.to_str().unwrap(),
            "--base",
            &base,
        ])
        .output()
        .unwrap();

    let receipt: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json"))
            .unwrap(),
    )
    .unwrap();

    assert_eq!(receipt["commit_count"], 4, "receipt: {receipt}");
    assert_eq!(receipt["mechanical_count"], 2, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 1, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "block", "receipt: {receipt}");
    assert!(
        !out.status.success(),
        "rollback-plan should exit non-zero on verdict=block"
    );

    let commits = receipt["commits"].as_array().unwrap();
    let blocking: Vec<&serde_json::Value> = commits
        .iter()
        .filter(|c| c["mechanical"] == false && c["revertable"] == false)
        .collect();
    assert_eq!(blocking.len(), 1, "receipt: {receipt}");
    assert_eq!(blocking[0]["subject"], "src: initial helper", "receipt: {receipt}");

    let non_clean_mechanical = commits
        .iter()
        .filter(|c| c["mechanical"] == true && c["revertable"] == false)
        .count();
    assert!(
        non_clean_mechanical >= 1,
        "expected at least one non-revert-clean mechanical commit excluded from blocking: {receipt}"
    );
}
