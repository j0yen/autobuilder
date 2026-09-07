//! AC4 (PRD-rollback-mechanical-chains): a merge whose `-m 1` revert
//! conflicts stays classified `substantive` (blocking), and the receipt
//! records the conflicting paths.

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
fn mergerev_ac4_conflicting_merge_stays_substantive() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("shared.txt"), "line1\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // Feature branch introduces a line into shared.txt.
    git(project, &["checkout", "-q", "-b", "feature"]);
    fs::write(project.join("shared.txt"), "line1\nfeature-line\n").unwrap();
    commit_all(project, "feature: append feature-line");

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

    // A later commit on main rewrites the exact hunk the merge introduced —
    // reverting the merge with `-m 1` against this HEAD must conflict.
    fs::write(project.join("shared.txt"), "line1\nfeature-line-changed\n").unwrap();
    commit_all(project, "main: adjust feature-line wording");

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

    assert_eq!(receipt["commit_count"], 2, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "block", "receipt: {receipt}");
    assert!(
        !out.status.success(),
        "rollback-plan should exit non-zero on verdict=block"
    );

    let commits = receipt["commits"].as_array().unwrap();
    let merge = commits
        .iter()
        .find(|c| c["parent_count"] == 2)
        .unwrap_or_else(|| panic!("no merge commit found in receipt: {receipt}"));
    assert_eq!(merge["class"], "substantive", "receipt: {receipt}");
    assert_eq!(merge["revertable"], false, "receipt: {receipt}");
    assert!(
        merge["note"].as_str().unwrap().contains("shared.txt"),
        "expected conflict path shared.txt in note: {receipt}"
    );
    assert!(
        receipt["blocking_count"].as_u64().unwrap() >= 1,
        "receipt: {receipt}"
    );
}
