//! AC14 (P0, hotfix 2026-09-30 — run 309, mcphost) — a concurrent land can
//! release the EXACT version this branch bumped to, on a different commit,
//! before this branch reaches land: `mcphost-chain-run-lineage` bumped to
//! 0.63.0 at d50baaa, but run 312 released v0.63.0 on a different sha
//! first, so land's version-at-rebase self-heal bumped again to 0.64.0.
//! The superseded 0.63.0 bump was never going to carry the tag itself (a
//! different commit already does) — it must NOT block `tag-lineage-gap`,
//! only the newest entry's taggable/blocked check (today's behavior)
//! governs the verdict. The receipt's `superseded` count must name how
//! many such bumps were forgiven.

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
fn ac14_superseded_bump_tagged_on_a_different_commit_is_not_a_gap() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.62.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.62.0");
    git(&project, &["tag", "v0.62.0"]);

    // A concurrent land (run 312) already released v0.63.0 -- on a
    // DIFFERENT commit than this branch's own bump below. Tagging the
    // base commit a second time is enough to prove the point: the check
    // must look for the tag anywhere in the repo, not on this branch's
    // own 0.63.0 commit.
    git(&project, &["tag", "v0.63.0"]);

    // This branch's own bump to 0.63.0 — collided with run 312's release,
    // so it will never itself carry the v0.63.0 tag.
    write_cargo_toml(&project, "svc", "0.63.0");
    commit_all(&project, "svc: v0.63.0 (mcphost-chain-run-lineage)");

    // land's version-at-rebase self-heal bumps again, past the collision.
    write_cargo_toml(&project, "svc", "0.64.0");
    commit_all(&project, "land: version-at-rebase self-heal v0.63.0 -> v0.64.0");

    let out = autobuilder()
        .args([
            "rollback-plan",
            "--project",
            project.to_str().unwrap(),
            "--base",
            "v0.62.0",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected pass (superseded bump tagged elsewhere is not a lineage gap); stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_ne!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_eq!(v["superseded"], 1, "the 0.63.0 bump must be counted as superseded: {v}");
    let lineage = v["tag_lineage"].as_array().unwrap();
    assert_eq!(lineage.len(), 2, "lineage: {lineage:?}");
}
