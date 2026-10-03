//! AC16 (P0, hotfix 2026-10-03, PRD-autobuilder-rollback-tag-ancestor-ok) —
//! the base-exact-match check used to require `v<base_version>` to point
//! AT base itself. That falsely blocked a base that is a normal un-bumped
//! hotfix commit sitting a few commits past its own release tag (mcphost
//! main `e39196a0`, one hotfix past `v0.72.0`, runs 358/354/336 on
//! 2026-10-03): hotfix PRs without a version bump are ordinary on main, not
//! a lineage gap.
//!
//! The base is now also accepted when `v<base_version>` lives on an
//! ANCESTOR of base and the version at that tag's commit still equals
//! `base_version` (nothing changed the declared version in between). A tag
//! that exists only at a DIFFERENT version than the base's own version is
//! still a genuine gap.

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
fn ac16_unbumped_hotfix_past_release_tag_passes_with_distance_one() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.72.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.72.0");
    git(&project, &["tag", "v0.72.0"]);
    let tag_commit = head_sha(&project);

    // commit A: the release tag. commit B: a hotfix PR with no version
    // bump — ordinary on main, and the shape that used to false-block.
    fs::write(project.join("README.md"), "hotfix\n").unwrap();
    commit_all(&project, "hotfix: no version bump");
    let base_sha = head_sha(&project);
    assert_ne!(base_sha, tag_commit, "base must be past the tag, not the tag commit itself");

    // One more commit past base — a real version bump, untagged (the
    // normal shape before `ship-tag.sh` tags a passing gate).
    write_cargo_toml(&project, "svc", "0.73.0");
    commit_all(&project, "bump to 0.73.0");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected pass (un-bumped hotfix past its release tag is not a lineage gap); stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_ne!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert_eq!(v["base_tag_commit"], tag_commit, "receipt: {v}");
    assert_eq!(v["base_tag_distance"], 1, "receipt: {v}");
}

#[test]
fn ac16_base_bumped_past_release_tag_without_its_own_tag_still_blocks() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.72.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.72.0");
    git(&project, &["tag", "v0.72.0"]);

    // Base itself bumps to 0.73.0 — a DIFFERENT version than the nearest
    // ancestor tag (v0.72.0) — and nothing ever tags v0.73.0. This is a
    // genuine gap, not an un-bumped hotfix: the ancestor tolerance must not
    // forgive it.
    write_cargo_toml(&project, "svc", "0.73.0");
    commit_all(&project, "bump to 0.73.0 (forgot to tag)");
    let base_sha = head_sha(&project);

    write_cargo_toml(&project, "svc", "0.74.0");
    commit_all(&project, "bump to 0.74.0");

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
    assert!(detail.contains("0.73.0"), "block_detail should name the base's own untagged version: {detail}");
    assert!(v["base_tag_commit"].is_null(), "receipt: {v}");
    assert!(v["base_tag_distance"].is_null(), "receipt: {v}");
}
