//! AC3 (P0, PRD-autobuilder-rollback-tag-aware) — the same library fixture
//! shape as AC2 but with all commits revert-clean (touching different
//! files instead of the same line) passes, identical to the pre-change
//! golden for that fixture.

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

#[test]
fn ac3_all_revert_clean_commits_pass_like_the_pre_change_golden() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("a.txt"), "a\n").unwrap();
    commit_all(&project, "initial: a");
    git(&project, &["tag", "v0.1.0"]);

    // Two independent, non-overlapping edits: each reverts cleanly onto
    // current HEAD regardless of order.
    fs::write(project.join("b.txt"), "b\n").unwrap();
    commit_all(&project, "add b");
    fs::write(project.join("c.txt"), "c\n").unwrap();
    commit_all(&project, "add c");

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap()])
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
    assert_eq!(v["commit_count"], 2);
    assert_eq!(v["blocking_count"], 0);
    assert_eq!(v["revertable_count"], 2);
    assert_eq!(v["base_tag"], "v0.1.0");

    assert_eq!(v["schema"], "autobuilder.rollback_plan_receipt.v2");
    assert_eq!(v["rollback_model"], "revert-commits");
    assert!(v.get("rollback_target").is_none() || v["rollback_target"].is_null());
    assert!(v.get("tag_lineage").is_none() || v["tag_lineage"].is_null());
}
