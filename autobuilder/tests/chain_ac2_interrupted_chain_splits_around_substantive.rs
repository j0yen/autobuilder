//! AC2 (PRD-rollback-mechanical-chains): a chain on the chain-allowed path
//! set (`www/`) interrupted by a substantive commit touching `src/` must
//! classify the interrupting commit `substantive`, while the chain segments
//! on either side of it still classify `mechanical(chain)`.
//!
//! Fixture note (PRD-rollback-chain-member-revert-check): each segment
//! commit touches its own distinct file under `www/` so every member is
//! independently revert-clean under the now-per-member check (a same-file
//! version of this fixture stopped being all-clean once revert-cleanliness
//! is verified per member instead of once against each segment's terminal
//! commit — see the sibling `chainmember_ac*` tests for the case where that
//! matters).

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
fn chain_ac2_interrupted_chain_splits_around_substantive() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("www")).unwrap();
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("www/index.html"), "v0\n").unwrap();
    fs::write(project.join("src/lib.rs"), "fn f() {}\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // Segment A: a 2-commit www/ chain, each commit touching its own file.
    fs::write(project.join("www/copy-a1.html"), "a1\n").unwrap();
    commit_all(project, "www: copy a1");
    fs::write(project.join("www/copy-a2.html"), "a2\n").unwrap();
    commit_all(project, "www: copy a2");

    // Interrupter: a genuine src/ change, breaking path_ok.
    fs::write(project.join("src/lib.rs"), "fn f() { 1 }\n").unwrap();
    commit_all(project, "src: real fix");

    // Segment B: another 2-commit www/ chain, each commit touching its own file.
    fs::write(project.join("www/copy-b1.html"), "b1\n").unwrap();
    commit_all(project, "www: copy b1");
    fs::write(project.join("www/copy-b2.html"), "b2\n").unwrap();
    commit_all(project, "www: copy b2");

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
    let _ = out.status;

    let receipt: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json"))
            .unwrap(),
    )
    .unwrap();

    assert_eq!(receipt["commit_count"], 5, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["mechanical_chain"], 4, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["substantive"], 1, "receipt: {receipt}");

    let commits = receipt["commits"].as_array().unwrap();
    let by_subject = |subj: &str| -> &serde_json::Value {
        commits
            .iter()
            .find(|c| c["subject"] == subj)
            .unwrap_or_else(|| panic!("missing commit {subj} in receipt: {receipt}"))
    };

    assert_eq!(by_subject("src: real fix")["class"], "substantive", "receipt: {receipt}");
    for subj in ["www: copy a1", "www: copy a2", "www: copy b1", "www: copy b2"] {
        assert_eq!(by_subject(subj)["class"], "mechanical_chain", "receipt: {receipt}");
    }
}
