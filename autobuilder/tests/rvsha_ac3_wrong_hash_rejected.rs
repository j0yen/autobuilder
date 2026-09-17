//! AC3 (P0, PRD-autobuilder-reviewer-intent-card-sha): given
//! `intent_card_sha` = the hash of a DIFFERENT card, `finalize` fails and
//! the error names the given value, the normalized given hex, and the
//! current card's hex.

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

fn falsification_json() -> serde_json::Value {
    serde_json::json!({
        "test_audit": "reviewed the acceptance tests; all cover their AC.",
        "panic_audit": "no unwrap/expect outside test code.",
        "unsafe_audit": "no unsafe present.",
        "public_api_audit": "no misuse path found.",
        "deps_audit": "no dependency changes since baseline.",
        "drift_audit": "no drift since first-green.",
        "counter_attack": {
            "description": "a malformed input could bypass validation.",
            "test_skeleton": "#[test]\nfn would_break_x() { unimplemented!() }"
        }
    })
}

#[test]
fn rvsha_ac3_wrong_hash_rejected() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-q"]);
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
    let head_sha = git_stdout(&dir, &["rev-parse", "HEAD"]);
    let card_bytes = fs::read(dir.join("agent/intent-card.json")).unwrap();
    let card_hex = sha256_hex_bare(&card_bytes);

    // Hash of a DIFFERENT (never-written) card.
    let wrong_hex = sha256_hex_bare(b"a different intent card entirely\n");
    assert_ne!(wrong_hex, card_hex, "test fixture sanity: hashes must differ");
    let given = format!("sha256:{wrong_hex}");

    let reviewer_output = serde_json::json!({
        "schema": "autobuilder.reviewer_agent_receipt.v1",
        "head_sha": head_sha,
        "intent_card_sha": given,
        "decision": "pass",
        "block_reasons": [],
        "concern_reasons": [],
        "falsification": falsification_json(),
    });
    let input_path = dir.join("review-output.json");
    fs::write(&input_path, serde_json::to_vec(&reviewer_output).unwrap()).unwrap();

    let output = autobuilder()
        .args(["reviewer-agent", "finalize", "--project"])
        .arg(&dir)
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "finalize must reject a card hash that doesn't match the current card"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&given),
        "error must contain the given value {given}; stderr={stderr}"
    );
    assert!(
        stderr.contains(&wrong_hex),
        "error must contain the normalized given hex {wrong_hex}; stderr={stderr}"
    );
    assert!(
        stderr.contains(&card_hex),
        "error must contain the current card's hex {card_hex}; stderr={stderr}"
    );

    // No receipt should be written on a rejected mismatch.
    assert!(
        !dir.join("target/autobuilder/receipts/reviewer-agent.json").exists(),
        "finalize must not write a receipt for a rejected intent_card_sha"
    );
}
