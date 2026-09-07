//! AC1 (PRD-rollback-mechanical-chains): a maximal ≥2-commit sequence on the
//! chain-allowed path set (`www/`), where each later commit supersedes the
//! last, classifies `mechanical(chain)` for every member — and
//! revert-cleanliness is checked only once, against the chain's terminal
//! commit, not per-link.

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
fn chain_ac1_clean_chain_classifies_mechanical() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("www")).unwrap();
    fs::write(project.join("www/index.html"), "v0\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    // A 3-commit chain, each edit superseding the last on the same file.
    fs::write(project.join("www/index.html"), "v1\n").unwrap();
    commit_all(project, "www: tagline v1");
    fs::write(project.join("www/index.html"), "v2\n").unwrap();
    commit_all(project, "www: tagline v2");
    fs::write(project.join("www/index.html"), "v3\n").unwrap();
    commit_all(project, "www: tagline v3");

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
    }
    // The chain's terminal (v3, the only commit nothing later touches) is
    // the one revert-cleanliness was actually computed against, and it's
    // clean since HEAD *is* v3.
    let terminal = commits
        .iter()
        .find(|c| c["subject"] == "www: tagline v3")
        .unwrap();
    assert_eq!(terminal["revertable"], true, "receipt: {receipt}");
}
