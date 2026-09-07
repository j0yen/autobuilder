//! AC10 (P0, PRD-autobuilder-rollback-tag-aware) — the concrete mcphost
//! scenario: a HEAD with interleaved v0.13.1/v0.13.2 merge commits,
//! configured `redeploy-tag`, base `v0.13.0`. Verdict must be pass — this
//! is the exact real-world shape that blocked the mcphost gate twice on
//! 2026-09-05 under the old revert-commits-only check.

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

/// Merge a throwaway branch's version-bump commit into `main` via
/// `--no-ff`, mirroring mcphost's real interleaved-integrate history.
fn merge_bump(dir: &Path, name: &str, version: &str, branch: &str) {
    git(dir, &["checkout", "-q", "-b", branch]);
    write_cargo_toml(dir, name, version);
    commit_all(dir, &format!("bump to {version}"));
    git(dir, &["checkout", "-q", "main"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "merge",
            "--no-ff",
            "-q",
            "-m",
            &format!("merge: integrate v{version}"),
            branch,
        ],
    );
}

#[test]
fn ac10_mcphost_style_interleaved_merges_pass_at_base_v0_13_0() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "mcphost", "0.13.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.13.0");
    git(&project, &["tag", "v0.13.0"]);

    // Two more --no-ff parallel-extend integrates land on top, each its own
    // version-bump merge commit, each tagged as it shipped.
    merge_bump(&project, "mcphost", "0.13.1", "autobuilder/extend-a");
    git(&project, &["tag", "v0.13.1"]);
    merge_bump(&project, "mcphost", "0.13.2", "autobuilder/extend-b");
    git(&project, &["tag", "v0.13.2"]); // HEAD

    let out = autobuilder()
        .args([
            "rollback-plan",
            "--project",
            project.to_str().unwrap(),
            "--base",
            "v0.13.0",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected pass; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "pass");
    assert_eq!(v["rollback_target"]["tag"], "v0.13.0");
}
