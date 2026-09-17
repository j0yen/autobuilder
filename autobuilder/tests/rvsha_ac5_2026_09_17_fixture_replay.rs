//! AC5 (P0, PRD-autobuilder-reviewer-intent-card-sha): replay of the
//! 2026-09-17 15:54:42Z proof-lane object. The real incident: the reviewer
//! wrote a bare-hex `intent_card_sha` for a valid `block` verdict
//! (`block_reasons=["must-ac5-extend-gate-verification-missing"]`); the
//! OLD `finalize` byte-compared against the `sha256:`-prefixed form and
//! rejected the receipt outright, so the gate went red on
//! `reviewer-agent:infra` and the real finding never reached the receipt.
//!
//! This test stores that object's shape (schema, decision, block_reasons,
//! falsification) and its intent card as fixtures (`tests/fixtures/
//! rvsha-2026-09-17/`) — the `head_sha`/`intent_card_sha` values are
//! templated and filled in at test time against a freshly constructed repo,
//! since the original incident's exact git object ids were never captured
//! byte-for-byte in the PRD's own grounding text. What's replayed exactly is
//! the defect shape: a bare-hex `intent_card_sha` on an otherwise-valid
//! `block` receipt. After this PRD's fix, `finalize` must accept the bare
//! hex and still write `decision=block` with the one real block reason —
//! never an infra rejection that hides it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::fs;
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};
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

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn sha256_hex_bare(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[test]
fn rvsha_ac5_2026_09_17_fixture_replay() {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rvsha-2026-09-17");
    let intent_card_fixture = fs::read(fixture_dir.join("intent-card.json")).unwrap();
    let review_output_template =
        fs::read_to_string(fixture_dir.join("review-output.json")).unwrap();

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q"]);
    fs::create_dir_all(dir.join("agent")).unwrap();
    fs::write(dir.join("agent/intent-card.json"), &intent_card_fixture).unwrap();
    git(&dir, &["add", "-A"]);
    git(
        &dir,
        &[
            "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
            "commit", "-q", "-m", "init",
        ],
    );
    let head_sha = git_stdout(&dir, &["rev-parse", "HEAD"]);
    // Bare hex, no sha256: prefix — replaying the exact defect shape.
    let card_hex_bare = sha256_hex_bare(&intent_card_fixture);

    let filled = review_output_template
        .replace("{{HEAD_SHA}}", &head_sha)
        .replace("{{INTENT_CARD_SHA}}", &card_hex_bare);
    // Sanity: the fixture really is a block verdict with one block reason,
    // and the card hash really is bare (no prefix) as in the incident.
    let parsed: serde_json::Value = serde_json::from_str(&filled).unwrap();
    assert_eq!(parsed["decision"], serde_json::json!("block"));
    assert!(!card_hex_bare.starts_with("sha256:"));

    let input_path = dir.join("review-output.json");
    fs::write(&input_path, &filled).unwrap();

    let output = autobuilder()
        .args(["reviewer-agent", "finalize", "--project"])
        .arg(&dir)
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // finalize itself still returns non-zero for a block decision (R2 of
    // the base contract, unchanged) — the fix is that it gets FAR ENOUGH to
    // write the receipt at all, instead of rejecting the object as
    // malformed/mismatched before ever reaching the decision.
    assert!(
        !output.status.success(),
        "a block decision must still exit non-zero; stdout={stdout} stderr={stderr}"
    );
    assert!(
        stdout.contains("decision=block"),
        "stdout must report decision=block, not an infra-shaped rejection; stdout={stdout}"
    );

    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("target/autobuilder/receipts/reviewer-agent.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["decision"], serde_json::json!("block"));
    assert_eq!(
        receipt["block_reasons"],
        serde_json::json!(["must-ac5-extend-gate-verification-missing"])
    );
    assert_eq!(
        receipt["intent_card_sha"],
        serde_json::json!(format!("sha256:{card_hex_bare}")),
        "the written receipt must carry the canonical prefixed form even though the input was bare"
    );
}
