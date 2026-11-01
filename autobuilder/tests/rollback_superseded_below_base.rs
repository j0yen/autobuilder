//! rplb (autobuilder 0.9.2) — a version bump strictly below the base version
//! is a superseded bump, not a lineage gap (a branch that merged main after
//! bumping lands back at base's version); a final tree BELOW base would land
//! a version regression and blocks as `version-regression-below-base`.
//!
//! a: untagged bump to 0.2.5 under a v0.3.0 base, final tree restored to
//!    0.3.0 -> pass, output names `superseded-below-base`.
//! b: untagged bump above base that is not the newest -> still
//!    `tag-lineage-gap` (guard).
//! e: the same 0.2.5 bump left as the final tree -> block
//!    `version-regression-below-base`.
//! f: base Cargo.toml version unreadable -> today's block stands and the
//!    reason is printed; no below-base rule runs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::indexing_slicing)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn write_cargo_toml(dir: &Path, version: &str) {
    fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"svc\"\nversion = \"{version}\"\n")).unwrap();
}

/// A fixture repo whose first commit is `base_version` tagged `v<base_version>`.
fn fixture_with_tagged_base(tmp: &TempDir, base_version: &str) -> PathBuf {
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, base_version);
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, &format!("initial: v{base_version}"));
    git(&project, &["tag", &format!("v{base_version}")]);
    project
}

fn rollback_plan(project: &Path, base: &str) -> Output {
    autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap(), "--base", base])
        .output()
        .unwrap()
}

fn receipt(project: &Path) -> serde_json::Value {
    let text = fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_untagged_bump_below_base_with_restored_final_tree_is_superseded() {
    let tmp = TempDir::new().unwrap();
    let project = fixture_with_tagged_base(&tmp, "0.3.0");

    write_cargo_toml(&project, "0.2.5");
    commit_all(&project, "release: 0.2.5");
    write_cargo_toml(&project, "0.3.0");
    commit_all(&project, "merge main: back to 0.3.0");

    let out = rollback_plan(&project, "v0.3.0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "expected pass; stdout={stdout} stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("superseded-below-base"), "stdout must name the classification: {stdout}");
    assert!(stdout.contains("version=0.2.5"), "stdout must name the bumped version: {stdout}");
    let v = receipt(&project);
    assert_eq!(v["verdict"], "pass", "receipt: {v}");
    assert_ne!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    assert_eq!(v["superseded"], 1, "the 0.2.5 bump counts as superseded: {v}");
}

#[test]
fn b_untagged_non_newest_bump_above_base_still_gaps() {
    let tmp = TempDir::new().unwrap();
    let project = fixture_with_tagged_base(&tmp, "0.3.0");

    write_cargo_toml(&project, "0.3.1");
    commit_all(&project, "bump to 0.3.1 (forgot to tag)");
    write_cargo_toml(&project, "0.3.2");
    commit_all(&project, "bump to 0.3.2");

    let out = rollback_plan(&project, "v0.3.0");
    assert!(!out.status.success(), "expected block (non-zero exit)");
    let v = receipt(&project);
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "tag-lineage-gap", "receipt: {v}");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.3.1"), "block_detail should name the untagged bump: {detail}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("superseded-below-base"));
}

#[test]
fn e_final_tree_below_base_blocks_version_regression() {
    let tmp = TempDir::new().unwrap();
    let project = fixture_with_tagged_base(&tmp, "0.3.0");

    write_cargo_toml(&project, "0.2.5");
    commit_all(&project, "release: 0.2.5");

    let out = rollback_plan(&project, "v0.3.0");
    assert!(!out.status.success(), "expected block (non-zero exit)");
    let v = receipt(&project);
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "version-regression-below-base", "receipt: {v}");
    let detail = v["block_detail"].as_str().unwrap();
    assert!(detail.contains("0.2.5") && detail.contains("0.3.0"), "detail names both versions: {detail}");
}

#[test]
fn f_unreadable_base_version_keeps_todays_block_and_prints_reason() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    fs::create_dir_all(project.join("agent")).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("agent/intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();

    // An unrelated, disjoint commit claims v0.77.0 first.
    git(&project, &["checkout", "-q", "--orphan", "elsewhere"]);
    write_cargo_toml(&project, "0.77.0");
    commit_all(&project, "elsewhere: v0.77.0 (unrelated history)");
    git(&project, &["tag", "v0.77.0"]);

    // main: the base commit has no Cargo.toml version at all.
    git(&project, &["checkout", "-q", "--orphan", "main"]);
    fs::write(project.join("Cargo.toml"), "[package]\nname = \"svc\"\n").unwrap();
    commit_all(&project, "initial: no version yet");
    let base = String::from_utf8(
        Command::new("git").arg("-C").arg(&project).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
    )
    .unwrap();

    write_cargo_toml(&project, "0.77.0");
    commit_all(&project, "introduce version 0.77.0");

    let out = rollback_plan(&project, base.trim());
    assert!(!out.status.success(), "expected block (non-zero exit)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("base-version-unreadable"), "stdout must print the reason: {stdout}");
    assert!(!stdout.contains("superseded-below-base"), "no below-base rule may run: {stdout}");
    let v = receipt(&project);
    assert_eq!(v["verdict"], "block", "receipt: {v}");
    assert_eq!(v["block_reason"], "head-untagged", "today's reason is kept: {v}");
}
