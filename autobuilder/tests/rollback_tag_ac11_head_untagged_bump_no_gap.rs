//! AC1 (P0, PRD-autobuilder-rollback-tag-lineage-head-gap) — when the only
//! untagged entry in the tag lineage is the entry whose commit == HEAD (the
//! ordinary shape right after `extend-handler.sh bump-version`, before
//! `ship-tag.sh` has tagged a passing gate), `redeploy-tag` mode must not
//! block with `tag-lineage-gap`. HEAD's version is untagged but taggable
//! (nothing else claims the tag name), so it falls through to the normal
//! pass path instead.

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
fn ac1_single_bump_at_head_untagged_does_not_block_tag_lineage_gap() {
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

    // The only bump since the base tag — right at HEAD, not yet tagged.
    // This is exactly what `extend-handler.sh bump-version` leaves behind
    // before the gate has passed and `ship-tag.sh` can tag it.
    write_cargo_toml(&project, "svc", "0.2.0");
    commit_all(&project, "bump to 0.2.0");

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
        "expected pass (untagged HEAD bump is not a lineage gap); stdout={} stderr={}",
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
    assert!(lineage[0]["tag"].is_null(), "HEAD's own bump is untagged: {lineage:?}");
}
