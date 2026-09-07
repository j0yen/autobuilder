//! AC2 (P0, PRD-autobuilder-rollback-tag-aware) — a library-crate fixture
//! with no `rollback_model` set and one non-revert-clean commit still
//! blocks, identical (in every field the pre-PRD producer computed) to the
//! pre-change golden receipt for that fixture. Only the v2 schema string
//! and the new (empty, for this mode) v2 fields are allowed to differ —
//! the PRD requires the *logic* (base resolution, per-commit merge-tree
//! revert check, verdict rule) to be byte-for-byte unchanged, not the
//! receipt's schema version.

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

/// Library fixture: no `agent/` config at all (default `revert-commits`),
/// one tag (`v0.1.0`), then two commits touching the same line of the same
/// file back-to-back — the classic conflicting-revert shape: reverting the
/// first commit (`line1` -> `line2`) after the second (`line2` -> `line3`)
/// has already landed conflicts, because HEAD no longer has `line2` where
/// the revert expects it.
fn conflicting_fixture() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("file.txt"), "line1\n").unwrap();
    commit_all(&project, "initial: line1");
    git(&project, &["tag", "v0.1.0"]);

    fs::write(project.join("file.txt"), "line2\n").unwrap();
    commit_all(&project, "change to line2");

    fs::write(project.join("file.txt"), "line3\n").unwrap();
    commit_all(&project, "change to line3");

    (tmp, project)
}

#[test]
fn ac2_one_non_revert_clean_commit_blocks_like_the_pre_change_golden() {
    let (_tmp, project) = conflicting_fixture();

    let out = autobuilder()
        .args(["rollback-plan", "--project", project.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected block (non-zero exit)");

    let receipt_text =
        fs::read_to_string(project.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt_text).unwrap();

    // Pre-change golden fields: unchanged logic.
    assert_eq!(v["verdict"], "block");
    assert_eq!(v["commit_count"], 2);
    assert_eq!(v["blocking_count"], 1);
    assert_eq!(v["revertable_count"], 1);
    assert_eq!(v["base_tag"], "v0.1.0");
    let commits = v["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 2);
    let non_revertable: Vec<_> = commits.iter().filter(|c| c["revertable"] == false).collect();
    assert_eq!(non_revertable.len(), 1);
    assert_eq!(non_revertable[0]["subject"], "change to line2");

    // New-in-v2 fields: additive only, correctly reflecting revert-commits mode.
    assert_eq!(v["schema"], "autobuilder.rollback_plan_receipt.v2");
    assert_eq!(v["rollback_model"], "revert-commits");
    assert!(v.get("rollback_target").is_none() || v["rollback_target"].is_null());
    assert!(v.get("tag_lineage").is_none() || v["tag_lineage"].is_null());
}
