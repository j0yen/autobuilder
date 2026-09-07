//! AC1 (PRD-autobuilder-rollback-mechanical-commits): a commit whose subject
//! is `agent: refresh intent card for foo` and whose only changed file is
//! `agent/intent-card.json` classifies `mechanical`, regardless of whether
//! its revert dry-run succeeds. Synthetic git repo built inside the test —
//! no dependency on the live mcphost checkout.

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
fn ac1_mechanical_subject_and_files_classifies_mechanical() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("baseline.txt"), "baseline\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    fs::create_dir_all(project.join("agent")).unwrap();
    fs::write(project.join("agent/intent-card.json"), "{\"v\":1}\n").unwrap();
    commit_all(project, "agent: refresh intent card for foo");

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
    // Even a passing gate is fine here — AC1 only asserts classification.
    let _ = out.status;

    let receipt: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json"))
            .unwrap(),
    )
    .unwrap();

    assert_eq!(receipt["mechanical_count"], 1, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 0, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "pass", "receipt: {receipt}");
    let commits = receipt["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0]["mechanical"], true,
        "intent-card-only commit must classify mechanical: {receipt}"
    );
}
