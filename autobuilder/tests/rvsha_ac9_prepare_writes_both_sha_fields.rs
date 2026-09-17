//! AC9 (P1, PRD-autobuilder-reviewer-intent-card-sha): given `prepare` runs,
//! `review-request.json` carries `intent_card_sha` and `intent_card_sha256`
//! with identical values — so the field the reviewer copies from is named
//! exactly what it writes back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rvsha_ac9_prepare_writes_both_sha_fields() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q", "-b", "main"]);
    fs::create_dir_all(dir.join("agent")).unwrap();
    fs::write(
        dir.join("agent/intent-card.json"),
        b"{\"schema\":\"autobuilder.intent_card.v1\",\"intent_slug\":\"rvsha-fixture\"}\n",
    )
    .unwrap();
    git(&dir, &["add", "-A"]);
    git(
        &dir,
        &[
            "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
            "commit", "-q", "-m", "init",
        ],
    );

    let output = autobuilder()
        .args(["reviewer-agent", "prepare", "--project"])
        .arg(&dir)
        .arg("--base")
        .arg("main")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "prepare must succeed; stdout={stdout} stderr={stderr}"
    );

    let request: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("target/autobuilder/review-request.json")).unwrap(),
    )
    .unwrap();
    let sha256_field = request["intent_card_sha256"]
        .as_str()
        .expect("intent_card_sha256 must be a string");
    let sha_field = request["intent_card_sha"]
        .as_str()
        .expect("intent_card_sha must be present as a string");
    assert_eq!(
        sha_field, sha256_field,
        "intent_card_sha and intent_card_sha256 must carry identical values"
    );
    assert!(sha_field.starts_with("sha256:"), "value must be the canonical prefixed form");
}
