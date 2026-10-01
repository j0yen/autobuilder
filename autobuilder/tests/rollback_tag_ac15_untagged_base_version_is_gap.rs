//! AC15 (P0, hotfix 2026-09-30) — an explicit `--base` that resolves to a
//! commit whose own `[package].version` has no matching tag AT that commit
//! is itself a lineage gap: nothing reachable from base confirms this
//! starting point was ever released, so there is no revertable redeploy
//! target regardless of what the branch does afterward.

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

fn head_sha(dir: &Path) -> String {
    String::from_utf8(Command::new("git").arg("-C").arg(dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_owned()
}

#[test]
fn ac15_explicit_base_with_untagged_own_version_blocks_tag_lineage_gap() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.60.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.60.0");
    git(&project, &["tag", "v0.60.0"]);

    // Bumped to 0.61.0 -- never tagged. `--base` below points straight at
    // THIS commit (by sha, not by tag), so the base itself is the gap.
    write_cargo_toml(&project, "svc", "0.61.0");
    commit_all(&project, "bump to 0.61.0 (forgot to tag)");
    let base_sha = head_sha(&project);

    // One more commit past the untagged base -- HEAD itself is fine either
    // way; the gap is in the base, not in base..HEAD.
    write_cargo_toml(&project, "svc", "0.62.0");
    commit_all(&project, "bump to 0.62.0");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "block");
    assert_eq!(v["block_reason"], "tag-lineage-gap");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.61.0"), "block_detail should name the base's own untagged version: {detail}");
}
