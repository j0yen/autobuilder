//! AC1 (P0, PRD-rollback-redeploy-tag-onboard) — the onboarding path this
//! PRD exists to prove: a deploy-tag service opts in purely by dropping an
//! `agent/deploy-manifest.toml` marker file (no explicit `rollback_model`
//! key anywhere), and `rollback-plan` still infers `redeploy-tag` and
//! passes on the mcphost-shaped interleaved-`--no-ff`-merge history that
//! blocked real mcphost gates twice on 2026-09-05 under the
//! revert-commits-only check.
//!
//! This is deliberately distinct from
//! `rollback_tag_ac10_mcphost_interleaved_merges_pass.rs` (PRD-autobuilder-
//! rollback-tag-aware), which opts in via an explicit `rollback_model` key
//! in `agent/intent-card.json`. That test proves the explicit-override
//! path works; this one proves the marker-file *inference* path
//! (`infer_rollback_model`) works on its own, with no override present.

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
/// `--no-ff`, mirroring mcphost's real `worktree-extend.sh integrate`
/// history (same shape as AC10's fixture).
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
fn ac1_deploy_manifest_marker_alone_infers_redeploy_tag_and_passes() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "mcphost", "0.13.0");
    // The onboarding signal under test: a near-empty marker file, and
    // NOTHING else — no `rollback_model` key in intent-card.json, no
    // AUTOBUILDER_PROGRAM.md at all. `infer_rollback_model` must flip to
    // `redeploy-tag` on the marker's mere existence.
    fs::write(agent.join("deploy-manifest.toml"), "# mcphost deploy manifest\n# ships by tag, rolls back by redeploying the previous tag\n").unwrap();
    commit_all(&project, "initial: v0.13.0");
    git(&project, &["tag", "v0.13.0"]);

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
    assert_eq!(
        v["rollback_model"], "redeploy-tag",
        "marker file alone must be enough for infer_rollback_model to select redeploy-tag"
    );
    assert_eq!(v["rollback_target"]["tag"], "v0.13.0");
}
