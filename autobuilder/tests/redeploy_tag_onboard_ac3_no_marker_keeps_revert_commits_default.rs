//! AC3 (P0, PRD-rollback-redeploy-tag-onboard) — the regression guard this
//! PRD's own Non-goals demand: a repo with no `agent/deploy-manifest.toml`
//! and no explicit `rollback_model` (the `summa` / `rustbuild` shape) must
//! keep resolving to `revert-commits`, even when its history has the exact
//! interleaved `--no-ff` merge-commit shape that would pass under
//! `redeploy-tag`. Onboarding mcphost onto the existing model must not
//! quietly weaken (or auto-infer from merge-commit shape) the default for
//! every other crate in the fleet.

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

/// Same interleaved `--no-ff` merge-of-version-bump shape as
/// `redeploy_tag_onboard_ac1_*` and `rollback_tag_ac10_*` — deliberately
/// identical history, so this test isolates exactly one variable: the
/// absence of any onboarding signal.
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
fn ac3_no_deploy_manifest_and_no_rollback_model_key_stays_revert_commits() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    // No `agent/` directory at all — the `summa` / `rustbuild` shape named
    // explicitly in this PRD's Non-goals. Neither `agent/deploy-manifest.toml`
    // nor `agent/intent-card.json` nor `agent/AUTOBUILDER_PROGRAM.md` exist.
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "rustbuild-like", "0.13.0");
    commit_all(&project, "initial: v0.13.0");
    git(&project, &["tag", "v0.13.0"]);

    merge_bump(&project, "rustbuild-like", "0.13.1", "autobuilder/extend-a");
    git(&project, &["tag", "v0.13.1"]);
    merge_bump(&project, "rustbuild-like", "0.13.2", "autobuilder/extend-b");
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

    // Whatever the pass/block verdict on this particular history, the
    // load-bearing assertion is the MODEL: it must still be revert-commits,
    // never silently inferred as redeploy-tag from the merge-commit shape
    // alone.
    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();
    assert_eq!(
        v["rollback_model"], "revert-commits",
        "no onboarding marker/override present; default must stay revert-commits fleet-wide"
    );
    assert!(v.get("rollback_target").is_none() || v["rollback_target"].is_null());
    assert!(v.get("tag_lineage").is_none() || v["tag_lineage"].is_null());

    // Sanity: the same shape is proven to `pass` under redeploy-tag by the
    // AC1 test in this PRD (same fixture, only the marker file differs) —
    // this exit status is whatever revert-commits' own per-commit
    // git-revert-clean check produces for --no-ff merge commits, and is
    // asserted only to confirm the run completed and wrote a receipt,
    // never asserted as "must pass" or "must block" here.
    let _ = out.status.code();
}
