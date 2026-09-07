//! AC2 (PRD-autobuilder-rollback-mechanical-commits): the same mechanical
//! subject line, but the commit's changed-file set ALSO includes `src/lib.rs`
//! — must classify `substantive`. The mechanical subject-line match alone
//! must never launder an unexpected file change.

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
fn ac2_mechanical_subject_with_extra_file_classifies_substantive() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path();

    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/lib.rs"), "fn foo() {}\n").unwrap();
    commit_all(project, "chore: baseline");
    let base = git_stdout(project, &["rev-parse", "HEAD"]);

    fs::create_dir_all(project.join("agent")).unwrap();
    fs::write(project.join("agent/intent-card.json"), "{\"v\":1}\n").unwrap();
    // The mechanical subject, but this commit sneaks in a src/ edit too.
    fs::write(project.join("src/lib.rs"), "fn foo() { 1 }\n").unwrap();
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
    let _ = out.status;

    let receipt: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json"))
            .unwrap(),
    )
    .unwrap();

    assert_eq!(receipt["mechanical_count"], 0, "receipt: {receipt}");
    let commits = receipt["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0]["mechanical"], false,
        "unexpected src/ edit under mechanical subject must not classify mechanical: {receipt}"
    );
}
