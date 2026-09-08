//! AC1 (P0, PRD-autobuilder-rollback-tag-aware) — a crate configured
//! `rollback_model = redeploy-tag` with a contiguous v-tag lineage from the
//! previous deployed tag to HEAD, including `--no-ff` merge commits, passes
//! rollback-plan and names the previous tag as `rollback_target`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::too_many_lines, clippy::indexing_slicing)]

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

fn write_intent_card(dir: &Path, rollback_model: Option<&str>) {
    let agent = dir.join("agent");
    fs::create_dir_all(&agent).unwrap();
    let card = rollback_model.map_or_else(|| "{}".to_owned(), |m| format!("{{\"rollback_model\": \"{m}\"}}"));
    fs::write(agent.join("intent-card.json"), card).unwrap();
}

/// A minimal git repo, `Cargo.toml` at `name`/`version`, an intent-card
/// declaring `rollback_model` (or none), branch `main`.
fn init_repo(tmp: &TempDir, name: &str, version: &str, rollback_model: Option<&str>) -> std::path::PathBuf {
    let project = tmp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, name, version);
    write_intent_card(&project, rollback_model);
    commit_all(&project, &format!("v{version}"));
    project
}

/// Merge a throwaway branch's single version-bump commit into `main` via
/// `--no-ff`, simulating the interleaved merge history the PRD targets.
/// Returns the merge commit's sha.
fn merge_bump(dir: &Path, name: &str, version: &str, branch: &str) -> String {
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
            &format!("merge: release v{version}"),
            branch,
        ],
    );
    git_stdout(dir, &["rev-parse", "HEAD"])
}

#[test]
fn ac1_redeploy_tag_passes_with_merges_and_names_previous_tag() {
    let tmp = TempDir::new().unwrap();
    let project = init_repo(&tmp, "svc", "0.1.0", Some("redeploy-tag"));
    let base_sha = git_stdout(&project, &["rev-parse", "HEAD"]);
    git(&project, &["tag", "v0.1.0"]);

    let m1 = merge_bump(&project, "svc", "0.2.0", "feature/one");
    git(&project, &["tag", "v0.2.0"]);
    let _m2 = merge_bump(&project, "svc", "0.3.0", "feature/two");
    git(&project, &["tag", "v0.3.0"]); // HEAD already tagged too

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
        "expected pass; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(v["verdict"], "pass");
    assert_eq!(v["rollback_model"], "redeploy-tag");
    assert_eq!(v["rollback_target"]["tag"], "v0.1.0");
    assert_eq!(v["rollback_target"]["sha"], base_sha);
    // Both merge commits are present in the lineage.
    let lineage = v["tag_lineage"].as_array().unwrap();
    assert_eq!(lineage.len(), 2, "lineage: {lineage:?}");
    assert!(lineage.iter().any(|e| e["commit_sha"] == m1 && e["tag"] == "v0.2.0"));
}
