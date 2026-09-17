//! AC2 (P0, PRD-autobuilder-reviewer-intent-card-sha): given
//! `intent_card_sha` = `sha256:<hex>` (today's form) or the same hex in
//! uppercase, `finalize` succeeds with the canonical prefixed lowercase
//! form written to the receipt either way.

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

fn setup_project(tmp: &TempDir) -> (std::path::PathBuf, String, String) {
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
    (dir, head_sha, card_hex)
}

fn run_finalize_expect_ok(dir: &Path, head_sha: &str, given_intent_card_sha: &str) -> serde_json::Value {
    let reviewer_output = serde_json::json!({
        "schema": "autobuilder.reviewer_agent_receipt.v1",
        "head_sha": head_sha,
        "intent_card_sha": given_intent_card_sha,
        "decision": "pass",
        "block_reasons": [],
        "concern_reasons": [],
        "falsification": falsification_json(),
    });
    let input_path = dir.join(format!(
        "review-output-{}.json",
        given_intent_card_sha.len()
    ));
    fs::write(&input_path, serde_json::to_vec(&reviewer_output).unwrap()).unwrap();

    let output = autobuilder()
        .args(["reviewer-agent", "finalize", "--project"])
        .arg(dir)
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "finalize must accept intent_card_sha={given_intent_card_sha}; stdout={stdout} stderr={stderr}"
    );
    serde_json::from_slice(
        &fs::read(dir.join("target/autobuilder/receipts/reviewer-agent.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn rvsha_ac2_prefixed_hex_accepted() {
    let tmp = TempDir::new().unwrap();
    let (dir, head_sha, card_hex) = setup_project(&tmp);

    let receipt = run_finalize_expect_ok(&dir, &head_sha, &format!("sha256:{card_hex}"));
    assert_eq!(
        receipt["intent_card_sha"],
        serde_json::json!(format!("sha256:{card_hex}")),
        "prefixed input must round-trip to the same canonical form, not double-prefix"
    );
}

#[test]
fn rvsha_ac2_uppercase_hex_accepted() {
    let tmp = TempDir::new().unwrap();
    let (dir, head_sha, card_hex) = setup_project(&tmp);

    let receipt = run_finalize_expect_ok(&dir, &head_sha, &card_hex.to_ascii_uppercase());
    assert_eq!(
        receipt["intent_card_sha"],
        serde_json::json!(format!("sha256:{card_hex}")),
        "uppercase input must be lowercased in the canonical receipt form"
    );
}

#[test]
fn rvsha_ac2_prefixed_uppercase_hex_accepted() {
    let tmp = TempDir::new().unwrap();
    let (dir, head_sha, card_hex) = setup_project(&tmp);

    let receipt = run_finalize_expect_ok(
        &dir,
        &head_sha,
        &format!("sha256:{}", card_hex.to_ascii_uppercase()),
    );
    assert_eq!(
        receipt["intent_card_sha"],
        serde_json::json!(format!("sha256:{card_hex}")),
    );
}
