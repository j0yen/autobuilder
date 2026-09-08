//! AC5 (P1, PRD-rollback-redeploy-tag-onboard) — `scripts/set-rollback-model.sh`
//! must stamp `rollback_model: redeploy-tag` onto a real repo's
//! `agent/intent-card.json` and do so idempotently: a second run makes no
//! further change to the file.
//!
//! The fixture at `tests/fixtures/mcphost-intent-card.json` is a read-only
//! captured copy of mcphost's real `agent/intent-card.json`
//! (`/home/jsy/wintermute/mcphost/agent/intent-card.json`, captured
//! 2026-09-06) — it carries none of this PRD's `rollback_model` key, same
//! as mcphost's real card today. This test never opens the real mcphost
//! path; it only ever touches its own tempdir copy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

const FIXTURE: &str = include_str!("fixtures/mcphost-intent-card.json");

fn script_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/set-rollback-model.sh")
}

fn run_script(repo: &Path, model: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new("bash");
    cmd.arg(script_path()).arg(repo);
    if let Some(m) = model {
        cmd.arg(m);
    }
    cmd.output().expect("failed to run set-rollback-model.sh")
}

#[test]
fn ac5_set_rollback_model_stamps_redeploy_tag_idempotently() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("mcphost-copy");
    let agent = repo.join("agent");
    fs::create_dir_all(&agent).unwrap();
    // Fixture confirmed to carry no rollback_model key, mirroring mcphost's
    // real card as of this PRD's authoring — the case this helper exists
    // to fix.
    let card_path = agent.join("intent-card.json");
    fs::write(&card_path, FIXTURE).unwrap();
    let before: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert!(
        before.get("rollback_model").is_none(),
        "fixture must start with no rollback_model key for this test to prove anything"
    );

    // First run: default model (redeploy-tag), must set the key.
    let out1 = run_script(&repo, None);
    assert!(
        out1.status.success(),
        "first run failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out1.stdout),
        String::from_utf8_lossy(&out1.stderr)
    );
    let after_first = fs::read_to_string(&card_path).unwrap();
    let v1: serde_json::Value = serde_json::from_str(&after_first).unwrap();
    assert_eq!(v1["rollback_model"], "redeploy-tag");
    // Every other field must be untouched — spot-check a few, including a
    // nested one, plus a full round-trip equality check against `before`
    // with `rollback_model` re-inserted.
    assert_eq!(v1["intent_slug"], before["intent_slug"]);
    assert_eq!(v1["five_whys_trace"], before["five_whys_trace"]);
    let mut expected = before.clone();
    expected["rollback_model"] = serde_json::json!("redeploy-tag");
    assert_eq!(v1, expected, "only rollback_model should differ from the original card");

    // Second run: same model requested — must be a true no-op (byte-for-byte
    // identical file content, not just semantically-equivalent JSON).
    let out2 = run_script(&repo, None);
    assert!(
        out2.status.success(),
        "second run failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out2.stdout).contains("no change"),
        "second run should report no change; stdout={}",
        String::from_utf8_lossy(&out2.stdout)
    );
    let after_second = fs::read_to_string(&card_path).unwrap();
    assert_eq!(
        after_first, after_second,
        "second run must leave the card byte-for-byte identical (idempotent)"
    );
}

#[test]
fn ac5_set_rollback_model_rejects_invalid_model() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("project");
    let agent = repo.join("agent");
    fs::create_dir_all(&agent).unwrap();
    fs::write(agent.join("intent-card.json"), FIXTURE).unwrap();

    let out = run_script(&repo, Some("not-a-real-model"));
    assert!(!out.status.success(), "invalid model must be rejected");
}

#[test]
fn ac5_set_rollback_model_requires_existing_card() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("no-card-here");
    fs::create_dir_all(repo.join("agent")).unwrap();

    let out = run_script(&repo, None);
    assert!(!out.status.success(), "missing intent-card.json must be an error, not a silent create");
}
