//! AC3 (PRD-rollback-mechanical-chains): a clean no-ff merge classifies
//! `mechanical(merge)` via a real `git revert --no-commit -m 1` dry-run and
//! does not block.

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
fn mergerev_ac3_clean_merge_classifies_mechanical() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("baseline.txt"), "baseline\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    git(project, &["checkout", "-q", "-b", "feature"]);
    fs::write(project.join("feature.txt"), "feature\n").unwrap();
    commit_all(project, "feature: add feature.txt");

    git(project, &["checkout", "-q", "main"]);
    git(
        project,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "merge",
            "--no-ff",
            "-q",
            "-m",
            "Merge branch 'feature'",
            "feature",
        ],
    );

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

    assert_eq!(receipt["commit_count"], 1, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["mechanical_merge"], 1, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 0, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "pass", "receipt: {receipt}");
    assert!(
        out.status.success(),
        "rollback-plan should exit 0 on verdict=pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let commits = receipt["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["class"], "mechanical_merge", "receipt: {receipt}");
    assert_eq!(commits[0]["mechanical"], true, "receipt: {receipt}");
    assert_eq!(commits[0]["revertable"], true, "receipt: {receipt}");
    assert_eq!(commits[0]["parent_count"], 2, "receipt: {receipt}");
}
