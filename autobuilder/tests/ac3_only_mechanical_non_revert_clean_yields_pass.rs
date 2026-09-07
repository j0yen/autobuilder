//! AC3 (PRD-autobuilder-rollback-mechanical-commits): a fixture where the
//! ONLY non-revert-clean commits are mechanical (matching one of the three
//! patterns, file-set-bounded) must yield `verdict=pass` and
//! `blocking_count=0`, even though `mechanical_count > 0` and some of those
//! mechanical commits show `revertable=false`.
//!
//! Fixture: two `agent: refresh intent card for ...` commits touching the
//! same file. The older one is non-revert-clean by construction (the newer
//! one already superseded it) — exactly the real-world pile-up this PRD
//! targets.

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
fn ac3_only_mechanical_non_revert_clean_yields_pass() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("baseline.txt"), "baseline\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    fs::create_dir_all(project.join("agent")).unwrap();
    fs::write(project.join("agent/intent-card.json"), "v1\n").unwrap();
    commit_all(project, "agent: refresh intent card for foo");

    // Supersede the same file — the earlier refresh's revert now conflicts
    // against this HEAD, but it's still mechanical-shaped.
    fs::write(project.join("agent/intent-card.json"), "v2\n").unwrap();
    commit_all(project, "agent: refresh intent card for bar");

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
    assert_eq!(receipt["mechanical_count"], 2, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 0, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "pass", "receipt: {receipt}");
    // The command itself must exit 0 (verdict=pass), matching `run()`'s
    // `blocking > 0` gate.
    assert!(
        out.status.success(),
        "rollback-plan should exit 0 on verdict=pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let commits = receipt["commits"].as_array().unwrap();
    let non_clean_mechanical = commits
        .iter()
        .filter(|c| c["mechanical"] == true && c["revertable"] == false)
        .count();
    assert!(
        non_clean_mechanical >= 1,
        "expected at least one non-revert-clean mechanical commit: {receipt}"
    );
}
