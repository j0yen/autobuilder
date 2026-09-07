//! AC4 (P0, PRD-autobuilder-rollback-tag-aware) — `redeploy-tag` mode on a
//! crate with zero tags blocks with reason `no-previous-tag`: there is
//! nothing to roll back to yet.

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
fn ac4_zero_tags_blocks_no_previous_tag() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"svc\"\nversion = \"0.1.0\"\n").unwrap();
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: no tags yet");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "block");
    assert_eq!(v["rollback_model"], "redeploy-tag");
    assert_eq!(v["block_reason"], "no-previous-tag");
}
