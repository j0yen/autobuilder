//! AC7 (P1, PRD-autobuilder-rollback-tag-aware) — a version bump in the
//! range that was never tagged blocks `redeploy-tag` mode with reason
//! `tag-lineage-gap`, naming the untagged version.

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

fn write_cargo_toml(dir: &Path, name: &str, version: &str) {
    fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n")).unwrap();
}

#[test]
fn ac7_untagged_version_bump_blocks_tag_lineage_gap() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.1.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.1.0");
    git(&project, &["tag", "v0.1.0"]);

    // Bumped to 0.2.0 but never tagged — the gap.
    write_cargo_toml(&project, "svc", "0.2.0");
    commit_all(&project, "bump to 0.2.0 (forgot to tag)");

    // Then bumped again to 0.3.0, tagged this time, at HEAD.
    write_cargo_toml(&project, "svc", "0.3.0");
    commit_all(&project, "bump to 0.3.0");
    git(&project, &["tag", "v0.3.0"]);

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
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "block");
    assert_eq!(v["block_reason"], "tag-lineage-gap");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.2.0"), "block_detail should name the untagged version: {detail}");
}
