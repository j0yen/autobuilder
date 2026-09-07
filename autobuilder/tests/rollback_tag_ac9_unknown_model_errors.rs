//! AC9 (P0, PRD-autobuilder-rollback-tag-aware) — a config declaring an
//! unknown `rollback_model` value exits non-zero, names the bad value, and
//! writes no receipt.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::indexing_slicing)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "commit", "-q", "-m", msg],
    );
}

#[test]
fn ac9_unknown_rollback_model_errors_and_writes_no_receipt() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"svc\"\nversion = \"0.1.0\"\n").unwrap();
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "yolo-mode"}"#).unwrap();
    commit_all(&project, "initial");
    git(&project, &["tag", "v0.1.0"]);

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit for an unknown rollback_model");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("yolo-mode"), "stderr should name the bad value: {stderr}");

    assert!(
        !project.join("target/autobuilder/receipts/rollback-plan.json").exists(),
        "no receipt should be written for an invalid config"
    );
}
