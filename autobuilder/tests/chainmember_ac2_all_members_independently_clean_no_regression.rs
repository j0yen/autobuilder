//! AC2 (PRD-rollback-chain-member-revert-check): the same chain shape as
//! the AC1 fixture (a 3-commit chain on the chain-allowed path set,
//! `www/`), but with every member independently revert-clean. Asserts no
//! regression from the terminal-only baseline for the common case: all
//! three members still classify `mechanical(chain)` and `blocking_count`
//! stays 0.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    clippy::panic
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
fn chainmember_ac2_all_members_independently_clean_no_regression() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("www")).unwrap();
    fs::write(project.join("www/base.html"), "v0\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // A 3-commit chain, each member touching its own distinct file — none
    // of the three interacts with another, so every member's own dry-run
    // revert is independently clean.
    fs::write(project.join("www/page-a.html"), "a\n").unwrap();
    commit_all(project, "www: add page a");
    fs::write(project.join("www/page-b.html"), "b\n").unwrap();
    commit_all(project, "www: add page b");
    fs::write(project.join("www/page-c.html"), "c\n").unwrap();
    commit_all(project, "www: add page c");

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

    assert_eq!(receipt["commit_count"], 3, "receipt: {receipt}");
    assert_eq!(receipt["mechanical_count"], 3, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["mechanical_chain"], 3, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["substantive"], 0, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 0, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "pass", "receipt: {receipt}");
    assert!(
        out.status.success(),
        "rollback-plan should exit 0 on verdict=pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let commits = receipt["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 3);
    for c in commits {
        assert_eq!(c["class"], "mechanical_chain", "receipt: {receipt}");
        assert_eq!(c["mechanical"], true, "receipt: {receipt}");
        assert_eq!(c["revertable"], true, "receipt: {receipt}");
    }
}
