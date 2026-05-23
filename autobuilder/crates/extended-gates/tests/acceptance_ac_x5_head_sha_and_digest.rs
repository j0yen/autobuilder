//! AC-X5 + AC-X6: every receipt embeds the project's HEAD sha and a digest
//! that verifies under the autobuilder-receipt canonicalization.
//!
//! Drives one producer (`secrets-scan`) end-to-end against a tempdir with
//! a `git init`'d project; asserts:
//! - the receipt's `head_sha` equals `git rev-parse HEAD`
//! - re-canonicalizing with `receipt_digest=""` and re-hashing reproduces
//!   the stored `receipt_digest` (digest-roundtrip)
//! - mutating any other field invalidates the digest (forgery detection)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(status.success(), "git {args:?} failed");
}

fn init_fixture(dir: &Path) -> String {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("README.md"), b"hello\n").unwrap();
    run_git(dir, &["init", "-q", "-b", "main"]);
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn ac_x5_x6_head_sha_and_digest_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    let head = init_fixture(project);

    let spec = autobuilder_extended_gates::ProducerSpec::lookup("secrets-scan").unwrap();
    autobuilder_extended_gates::run_producer("secrets-scan", project).unwrap();

    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(spec.file_name);
    let bytes = std::fs::read(&receipt_path).unwrap_or_else(|e| {
        panic!("read receipt {}: {e}", receipt_path.display())
    });
    let value: Value = serde_json::from_slice(&bytes).unwrap();

    // AC-X5: head_sha matches HEAD.
    assert_eq!(
        value.get("head_sha").and_then(Value::as_str),
        Some(head.as_str()),
        "head_sha should match HEAD"
    );

    // AC-X6: digest-roundtrip.
    let stored_digest = value
        .get("receipt_digest")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let mut without_digest = value.clone();
    without_digest
        .as_object_mut()
        .unwrap()
        .insert("receipt_digest".into(), json!(""));
    let canonical = autobuilder_receipt::canonical_json_bytes(&without_digest);
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let recomputed = format!("sha256:{:x}", hasher.finalize());
    assert_eq!(stored_digest, recomputed, "digest roundtrip failed");

    // AC-X6: forgery detection. Mutating any non-digest, non-captured_at
    // field changes the recomputed digest.
    let mut tampered = value.clone();
    tampered
        .as_object_mut()
        .unwrap()
        .insert("verdict".into(), json!("block"));
    tampered
        .as_object_mut()
        .unwrap()
        .insert("receipt_digest".into(), json!(""));
    let canonical_t = autobuilder_receipt::canonical_json_bytes(&tampered);
    let mut hasher_t = Sha256::new();
    hasher_t.update(&canonical_t);
    let tampered_digest = format!("sha256:{:x}", hasher_t.finalize());
    assert_ne!(
        tampered_digest, stored_digest,
        "mutating verdict should change the digest"
    );
}
