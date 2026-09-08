//! AC1 (PRD-rollback-chain-member-revert-check): a 3-commit chain on the
//! chain-allowed path set (`www/`) where the MIDDLE member deletes a path
//! that the terminal member later re-adds with different, unrelated
//! content — the reviewer-agent `counter_attack` shape from
//! PRD-rollback-mechanical-chains's ship (`target/autobuilder/receipts/reviewer-agent.json`,
//! `reviewed_at=2026-09-08T13:52:00Z`). Under the prior terminal-only check,
//! the terminal commit's dry-run revert is (trivially) clean — reverting the
//! tip of history against itself always is — so the whole chain, middle
//! member included, classified `mechanical(chain)` and never counted toward
//! `blocking_count`. The middle member is NOT actually safe to roll back to
//! on its own: reverting it means restoring the path it deleted, but that
//! path now holds different content (re-added by the terminal commit), so a
//! real `git revert` there conflicts.
//!
//! This test asserts the fix: the middle commit is verified against its OWN
//! dry-run revert, classifies `substantive`, and is counted in
//! `blocking_count` — while the other two (independently-clean) chain
//! members keep `mechanical(chain)`.

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
fn chainmember_ac1_middle_member_non_revert_clean_excluded_and_blocking() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("www")).unwrap();
    // `other.html` is untouched by the delete/re-add drama below — it's
    // here so the chain's FIRST member has something of its own to edit
    // that never interacts with `index.html`.
    fs::write(project.join("www/other.html"), "o1\n").unwrap();
    fs::write(project.join("www/index.html"), "v1\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // Chain member 1: touches only other.html — independently revert-clean,
    // unrelated to the index.html story below.
    fs::write(project.join("www/other.html"), "o2\n").unwrap();
    commit_all(project, "www: copy refresh");

    // Chain member 2 (middle): deletes index.html.
    git(project, &["rm", "-q", "www/index.html"]);
    commit_all(project, "www: retire old page");

    // Chain member 3 (terminal): re-adds index.html with DIFFERENT content.
    // Reverting this one is trivially clean (HEAD *is* this commit), which
    // is exactly why the prior terminal-only check missed the middle
    // member's problem.
    fs::write(project.join("www/index.html"), "relaunch content\n").unwrap();
    commit_all(project, "www: relaunch page");

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
    // The middle commit is substantive and blocking; the other two members
    // of the same chain stay mechanical(chain).
    assert_eq!(receipt["classified"]["mechanical_chain"], 2, "receipt: {receipt}");
    assert_eq!(receipt["classified"]["substantive"], 1, "receipt: {receipt}");
    assert_eq!(receipt["blocking_count"], 1, "receipt: {receipt}");
    assert_eq!(receipt["verdict"], "block", "receipt: {receipt}");
    assert!(
        !out.status.success(),
        "rollback-plan should exit non-zero when blocking_count > 0"
    );

    let commits = receipt["commits"].as_array().unwrap();
    let by_subject = |subj: &str| -> &serde_json::Value {
        commits
            .iter()
            .find(|c| c["subject"] == subj)
            .unwrap_or_else(|| panic!("missing commit {subj} in receipt: {receipt}"))
    };

    let middle = by_subject("www: retire old page");
    assert_eq!(middle["class"], "substantive", "receipt: {receipt}");
    assert_eq!(middle["mechanical"], false, "receipt: {receipt}");
    assert_eq!(middle["revertable"], false, "receipt: {receipt}");

    let first = by_subject("www: copy refresh");
    assert_eq!(first["class"], "mechanical_chain", "receipt: {receipt}");
    assert_eq!(first["revertable"], true, "receipt: {receipt}");

    let terminal = by_subject("www: relaunch page");
    assert_eq!(terminal["class"], "mechanical_chain", "receipt: {receipt}");
    assert_eq!(terminal["revertable"], true, "receipt: {receipt}");
}
