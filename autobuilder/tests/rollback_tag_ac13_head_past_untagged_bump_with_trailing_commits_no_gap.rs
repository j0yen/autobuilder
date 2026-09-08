//! AC1 regression (P0, PRD-autobuilder-rollback-tag-lineage-head-gap) —
//! reproduces the exact `mcphost` shape that stalled three PRDs: HEAD is a
//! couple of commits *past* the untagged version-bump commit (an
//! intent-card refresh, then a Cargo.lock sync — neither touches
//! `[package].version`), so the lineage entry's `commit_sha` is NOT equal
//! to `head_sha` even though nothing has bumped since. The fix must key on
//! "is this the newest lineage entry", not literal commit equality with
//! HEAD, or this exact real-world case keeps blocking.

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
fn ac1_head_past_untagged_bump_via_trailing_non_bump_commits_does_not_block() {
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

    // The bump — untagged, and NOT HEAD by the time the gate runs.
    write_cargo_toml(&project, "svc", "0.2.0");
    commit_all(&project, "svc: v0.2.0 — some-prd (parallel integrate)");
    let bump_sha = String::from_utf8(
        Command::new("git").arg("-C").arg(&project).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_owned();

    // Two trailing commits, exactly like mcphost's real stall: an
    // intent-card refresh, then a Cargo.lock sync — neither touches
    // [package].version, so no new lineage entry is recorded for either.
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag", "refreshed": true}"#).unwrap();
    commit_all(&project, "agent: refresh intent card for some-prd");
    fs::write(project.join("Cargo.lock"), "# lockfile stub\nversion = 4\n").unwrap();
    commit_all(&project, "chore: sync Cargo.lock version field to 0.2.0");
    let head_sha = String::from_utf8(
        Command::new("git").arg("-C").arg(&project).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    assert_ne!(bump_sha, head_sha, "test setup: HEAD must have moved past the bump commit");

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
    assert!(
        out.status.success(),
        "expected pass (untagged bump superseded only by non-version commits is not a lineage gap); stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_ne!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    let lineage = v["tag_lineage"].as_array().unwrap();
    assert_eq!(lineage.len(), 1, "lineage: {lineage:?}");
    assert_eq!(lineage[0]["commit_sha"], bump_sha, "lineage entry names the bump commit, not HEAD: {lineage:?}");
    assert!(lineage[0]["tag"].is_null());
}
