//! AC5 (P0, PRD-autobuilder-rollback-tag-aware) — a `redeploy-tag` receipt
//! carries `rollback_model`, `rollback_target` (previous tag + sha), and
//! `tag_lineage`, and validates as schema v2.

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

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "commit", "-q", "-m", msg],
    );
}

fn write_cargo_toml(dir: &Path, name: &str, version: &str) {
    fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n")).unwrap();
}

#[test]
fn ac5_redeploy_tag_receipt_is_schema_v2_with_target_and_lineage() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.1.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.1.0");
    let base_sha = git_stdout(&project, &["rev-parse", "HEAD"]);
    git(&project, &["tag", "v0.1.0"]);

    write_cargo_toml(&project, "svc", "0.2.0");
    commit_all(&project, "bump to 0.2.0");
    git(&project, &["tag", "v0.2.0"]);

    let out = autobuilder()
        .args([
            "rollback-plan",
            "--project",
            project.to_str().unwrap(),
            "--base",
            "v0.1.0",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();

    assert_eq!(v["schema"], "autobuilder.rollback_plan_receipt.v2");
    assert_eq!(v["rollback_model"], "redeploy-tag");
    assert_eq!(v["rollback_target"]["tag"], "v0.1.0");
    assert_eq!(v["rollback_target"]["sha"], base_sha);
    assert!(v["rollback_target"]["redeploy_command"].as_str().unwrap().contains("v0.1.0"));
    let lineage = v["tag_lineage"].as_array().unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0]["version"], "0.2.0");
    assert_eq!(lineage[0]["tag"], "v0.2.0");
    // Existing (v1-era) fields remain, so downstream jq / the gate stay valid.
    for field in ["head_sha", "base_ref", "base_sha", "verdict", "captured_at", "receipt_digest"] {
        assert!(v.get(field).is_some(), "missing existing field {field}");
    }
}
