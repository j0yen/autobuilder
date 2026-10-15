//! AC16 (P0, hotfix 2026-10-15 — mcphost runs 256 on ryzen7 and 834 on
//! RedBaron) — a land's squash merge bumps `Cargo.toml` (its
//! `release: <ver>` commit message is squashed into the merge commit) and
//! the `v<ver>` tag is created minutes later by a deploy unit on another
//! machine. In that window the base commit's version has no tag at the
//! base or any ancestor, but that is a PENDING tag, not a lineage gap: the
//! rollback target is the newest tagged ancestor.
//!
//! Three cases: (1) release line in the squash body -> pass with
//! `pending_tag`; (2) a bump with no `release:` line anywhere -> still
//! `tag-lineage-gap`; (3) release line but no tag anywhere ->
//! `no-previous-tag`.

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

fn commit_all(dir: &Path, msgs: &[&str]) {
    git(dir, &["add", "-A"]);
    let mut args = vec!["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "commit", "-q"];
    for m in msgs {
        args.push("-m");
        args.push(m);
    }
    git(dir, &args);
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

/// v0.108.0 (tagged when `tag_initial`), an ordinary commit, then a bump to
/// 0.108.4 whose commit message is `bump_msgs`, then one trailing commit
/// that does not touch the version. Returns `(project, base_sha)` where the
/// base is the bump commit.
fn fixture(tmp: &TempDir, tag_initial: bool, bump_msgs: &[&str]) -> (std::path::PathBuf, String) {
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.108.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, &["release: 0.108.0"]);
    if tag_initial {
        git(&project, &["tag", "v0.108.0"]);
    }
    fs::write(project.join("notes.txt"), "an ordinary commit\n").unwrap();
    commit_all(&project, &["loop: earlier-slug (#298)"]);

    write_cargo_toml(&project, "svc", "0.108.4");
    commit_all(&project, bump_msgs);
    let base_sha = head_sha(&project);

    fs::write(project.join("notes.txt"), "trailing commit\n").unwrap();
    commit_all(&project, &["chore: trailing commit, no version change"]);
    (project, base_sha)
}

fn run(project: &Path, base: &str) -> (bool, serde_json::Value) {
    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", base])
        .output()
        .unwrap();
    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    (out.status.success(), serde_json::from_str(&receipt_text).unwrap())
}

#[test]
fn ac16_squash_release_line_with_tagged_ancestor_passes_with_pending_tag() {
    let tmp = TempDir::new().unwrap();
    let (project, base) = fixture(&tmp, true, &["loop: some-slug (#299)", "release: 0.108.4"]);
    let (ok, v) = run(&project, &base);
    assert!(ok, "expected pass (pending tag after a land); receipt: {v}");
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_eq!(v["pending_tag"], "v0.108.4", "receipt: {v}");
    assert_eq!(v["rollback_target"]["tag"], "v0.108.0", "receipt: {v}");
    assert!(v["note"].as_str().unwrap().contains("pending"), "receipt: {v}");
    assert!(v.get("block_reason").is_none() || v["block_reason"].is_null(), "receipt: {v}");
}

#[test]
fn ac16_bump_without_release_line_is_still_tag_lineage_gap() {
    let tmp = TempDir::new().unwrap();
    let (project, base) = fixture(&tmp, true, &["bump to 0.108.4 by hand"]);
    let (ok, v) = run(&project, &base);
    assert!(!ok, "expected block (non-zero exit); receipt: {v}");
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert!(v.get("pending_tag").is_none(), "receipt: {v}");
}

#[test]
fn ac16_release_line_with_no_tag_anywhere_is_no_previous_tag() {
    let tmp = TempDir::new().unwrap();
    let (project, base) = fixture(&tmp, false, &["loop: some-slug (#299)", "release: 0.108.4"]);
    let (ok, v) = run(&project, &base);
    assert!(!ok, "expected block (non-zero exit); receipt: {v}");
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "no-previous-tag", "receipt: {v}");
}

/// Real 056b407f shape: the squash body lists each squashed commit subject
/// as a `* `-prefixed bullet, including `* release: <ver>`.
fn squash_body(bullet: &str, release: &str) -> String {
    format!("{bullet} PRD — x: implement\n{bullet} {release}\n{bullet} chore: regen")
}

#[test]
fn ac16_squash_body_star_bullet_release_line_passes_with_pending_tag() {
    let tmp = TempDir::new().unwrap();
    let body = squash_body("*", "release: 0.108.4");
    let (project, base) = fixture(&tmp, true, &["loop: x (#298)", &body]);
    let (ok, v) = run(&project, &base);
    assert!(ok, "expected pass (pending tag, bulleted release line); receipt: {v}");
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_eq!(v["pending_tag"], "v0.108.4", "receipt: {v}");
    assert_eq!(v["rollback_target"]["tag"], "v0.108.0", "receipt: {v}");
}

#[test]
fn ac16_squash_body_dash_bullet_release_line_passes_with_pending_tag() {
    let tmp = TempDir::new().unwrap();
    let body = squash_body("-", "release: 0.108.4");
    let (project, base) = fixture(&tmp, true, &["loop: x (#298)", &body]);
    let (ok, v) = run(&project, &base);
    assert!(ok, "expected pass (pending tag, dash bullet); receipt: {v}");
    assert_eq!(v["pending_tag"], "v0.108.4", "receipt: {v}");
}

#[test]
fn ac16_squash_body_bullet_with_other_version_is_still_gap() {
    let tmp = TempDir::new().unwrap();
    let body = squash_body("*", "release: 0.108.3");
    let (project, base) = fixture(&tmp, true, &["loop: x (#298)", &body]);
    let (ok, v) = run(&project, &base);
    assert!(!ok, "expected block; receipt: {v}");
    assert_eq!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert!(v.get("pending_tag").is_none(), "receipt: {v}");
}

#[test]
fn ac16_squash_body_wrong_case_bullet_is_still_gap() {
    let tmp = TempDir::new().unwrap();
    let body = squash_body("*", "Release: 0.108.4");
    let (project, base) = fixture(&tmp, true, &["loop: x (#298)", &body]);
    let (ok, v) = run(&project, &base);
    assert!(!ok, "expected block; receipt: {v}");
    assert_eq!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
}

#[test]
fn ac16_squash_body_bullet_longer_version_is_still_gap() {
    let tmp = TempDir::new().unwrap();
    let body = squash_body("*", "release: 0.108.40");
    let (project, base) = fixture(&tmp, true, &["loop: x (#298)", &body]);
    let (ok, v) = run(&project, &base);
    assert!(!ok, "expected block; receipt: {v}");
    assert_eq!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
}
