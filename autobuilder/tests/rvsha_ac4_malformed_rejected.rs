//! AC4 (P0, PRD-autobuilder-reviewer-intent-card-sha): given
//! `intent_card_sha` of 63 hex chars, or 64 chars with a non-hex character,
//! `finalize` fails with a message naming `intent_card_sha` and the
//! expected `sha256:<64 hex>` form.

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

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
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

fn setup_project(tmp: &TempDir) -> (std::path::PathBuf, String) {
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
    (dir, head_sha)
}

fn run_finalize_expect_malformed(dir: &Path, head_sha: &str, given: &str) {
    let reviewer_output = serde_json::json!({
        "schema": "autobuilder.reviewer_agent_receipt.v1",
        "head_sha": head_sha,
        "intent_card_sha": given,
        "decision": "pass",
        "block_reasons": [],
        "concern_reasons": [],
        "falsification": falsification_json(),
    });
    let input_path = dir.join(format!("review-output-{}.json", given.len()));
    fs::write(&input_path, serde_json::to_vec(&reviewer_output).unwrap()).unwrap();

    let output = autobuilder()
        .args(["reviewer-agent", "finalize", "--project"])
        .arg(dir)
        .arg("--input")
        .arg(&input_path)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "finalize must reject malformed intent_card_sha={given}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("intent_card_sha"),
        "error must name the intent_card_sha field; stderr={stderr}"
    );
    assert!(
        stderr.contains("sha256:<64 hex>"),
        "error must name the expected sha256:<64 hex> form; stderr={stderr}"
    );
    assert!(
        !dir.join("target/autobuilder/receipts/reviewer-agent.json").exists(),
        "finalize must not write a receipt for a malformed intent_card_sha"
    );
}

#[test]
fn rvsha_ac4_63_char_rejected() {
    let tmp = TempDir::new().unwrap();
    let (dir, head_sha) = setup_project(&tmp);
    let short = "c".repeat(63);
    run_finalize_expect_malformed(&dir, &head_sha, &short);
}

#[test]
fn rvsha_ac4_non_hex_rejected() {
    let tmp = TempDir::new().unwrap();
    let (dir, head_sha) = setup_project(&tmp);
    let non_hex = "z".repeat(64);
    run_finalize_expect_malformed(&dir, &head_sha, &non_hex);
}
