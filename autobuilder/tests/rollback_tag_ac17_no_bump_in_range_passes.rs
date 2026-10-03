//! AC17 (P0, hotfix 2026-10-03, PRD-autobuilder-rollback-tag-no-bump-in-
//! range, run 368 mcphost) — `head_tag_status`'s "tag already claimed by a
//! different commit" check assumed every gated branch carries a version
//! bump. wm-build's land bumps the version itself and resets any inner bump
//! (0.9.66 SQUASH behavior), so a branch gated for land legitimately has
//! `[package].version` at HEAD equal to the version at its own base — no
//! bump in `base..HEAD` at all. That's not a collision: the "different
//! commit" the tag already lives on IS the base's own certified release
//! (exact match, or an ancestor per PR #3's tolerance, `base_tag_commit`).
//!
//! The fix must still block a REAL collision: a genuine bump in range whose
//! target version's tag is claimed by some unrelated, non-ancestor commit
//! (AC12/AC2-style — unaffected), and a bump-target version that happens to
//! match a tag reachable from nowhere HEAD actually descends from.

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

fn write_cargo_toml_no_version(dir: &Path, name: &str) {
    fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\n")).unwrap();
}

fn head_sha(dir: &Path) -> String {
    String::from_utf8(Command::new("git").arg("-C").arg(dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_owned()
}

fn init_project(project: &Path) {
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(project, &["init", "-q"]);
    git(project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
}

fn receipt(project: &Path) -> serde_json::Value {
    let text = fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// (i) base IS the release — `v0.77.0` tagged exactly at base — and HEAD
/// adds a source commit without bumping. No real bump happened anywhere in
/// `base..HEAD`: the tag `head_tag_status` finds "on a different commit" is
/// base's own commit. Must pass with `no-bump-in-range`.
#[test]
fn ac17_no_bump_in_range_base_exact_tag_passes() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    init_project(&project);
    write_cargo_toml(&project, "svc", "0.77.0");
    commit_all(&project, "initial: v0.77.0");
    git(&project, &["tag", "v0.77.0"]);
    let base_sha = head_sha(&project);

    // HEAD: a plain hotfix commit, no version bump — the ordinary shape of
    // a branch gated for land before land's own bump happens.
    fs::write(project.join("README.md"), "hotfix\n").unwrap();
    commit_all(&project, "hotfix: no version bump");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected pass (no bump in range; collision is base's own release); stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v = receipt(&project);
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_eq!(v["pass_reason"], "no-bump-in-range", "receipt: {v}");
    assert_eq!(v["head_version"], "0.77.0", "receipt: {v}");
    assert_eq!(v["base_version"], "0.77.0", "receipt: {v}");
    assert_eq!(v["range_bumps"], 0, "receipt: {v}");
}

/// (i, variant) base is an un-bumped hotfix commit a few commits PAST its
/// own release tag (PR #3's ancestor tolerance), and HEAD adds yet another
/// un-bumped commit on top. Both tolerances stack: base-vs-its-tag (PR #3)
/// and head-vs-base (this fix).
#[test]
fn ac17_no_bump_in_range_base_ancestor_tag_passes() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    init_project(&project);
    write_cargo_toml(&project, "svc", "0.77.0");
    commit_all(&project, "initial: v0.77.0");
    git(&project, &["tag", "v0.77.0"]);

    // base: one un-bumped hotfix commit past the tag.
    fs::write(project.join("NOTES.md"), "hotfix A\n").unwrap();
    commit_all(&project, "hotfix A: no version bump");
    let base_sha = head_sha(&project);

    // HEAD: another un-bumped commit past base.
    fs::write(project.join("NOTES2.md"), "hotfix B\n").unwrap();
    commit_all(&project, "hotfix B: no version bump");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected pass; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let v = receipt(&project);
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_eq!(v["pass_reason"], "no-bump-in-range", "receipt: {v}");
    assert_eq!(v["base_tag_distance"], 1, "receipt: {v}");
}

/// (ii) HEAD genuinely bumps to 0.78.0, untagged at HEAD's own commit — and
/// `v0.78.0` is already claimed by an unrelated, non-ancestor commit (a
/// concurrent land took that version number first). A real bump happened in
/// range, so the no-bump-in-range forgiveness must not apply: still blocks
/// `head-untagged`, unchanged from before this fix.
#[test]
fn ac17_bump_in_range_with_untagged_collision_still_blocks() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    init_project(&project);
    write_cargo_toml(&project, "svc", "0.77.0");
    commit_all(&project, "initial: v0.77.0");
    git(&project, &["tag", "v0.77.0"]);
    let base_sha = head_sha(&project);

    // An unrelated commit, on a disjoint orphan history, claims v0.78.0 —
    // not reachable from (not an ancestor of) the HEAD we're about to build.
    git(&project, &["checkout", "-q", "--orphan", "elsewhere"]);
    write_cargo_toml(&project, "svc", "0.78.0");
    commit_all(&project, "elsewhere: v0.78.0 (unrelated history)");
    git(&project, &["tag", "v0.78.0"]);

    // Back on main, HEAD bumps to 0.78.0 too — but HEAD's own commit is
    // untagged, and the only v0.78.0 tag in the repo is the unrelated one.
    git(&project, &["checkout", "-q", "main"]);
    write_cargo_toml(&project, "svc", "0.78.0");
    commit_all(&project, "bump to 0.78.0");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let v = receipt(&project);
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "head-untagged", "receipt: {v}");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.78.0"), "block_detail should name HEAD's version: {detail}");
    assert!(v["pass_reason"].is_null(), "receipt: {v}");
}

/// (iii) base has no readable version at all (skips the base-release check
/// entirely), HEAD introduces a version for the first time — but that exact
/// version's tag already exists on a commit that is NOT an ancestor of
/// HEAD. There is no certified `base_tag_commit` to match against, so the
/// no-bump-in-range forgiveness must not apply: blocks `head-untagged`, the
/// real "already exists on a different commit" case.
#[test]
fn ac17_base_unversioned_tag_elsewhere_not_ancestor_still_blocks() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    init_project(&project);

    // An unrelated, disjoint commit claims v0.77.0 first.
    git(&project, &["checkout", "-q", "--orphan", "elsewhere"]);
    write_cargo_toml(&project, "svc", "0.77.0");
    commit_all(&project, "elsewhere: v0.77.0 (unrelated history)");
    git(&project, &["tag", "v0.77.0"]);

    // main: base commit has NO [package].version at all.
    git(&project, &["checkout", "-q", "--orphan", "main"]);
    write_cargo_toml_no_version(&project, "svc");
    commit_all(&project, "initial: no version yet");
    let base_sha = head_sha(&project);

    // HEAD: introduces version 0.77.0 for the first time, untagged.
    write_cargo_toml(&project, "svc", "0.77.0");
    commit_all(&project, "introduce version 0.77.0");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", &base_sha])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let v = receipt(&project);
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "head-untagged", "receipt: {v}");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.77.0"), "block_detail should name HEAD's version: {detail}");
    assert!(v["pass_reason"].is_null(), "receipt: {v}");
}
