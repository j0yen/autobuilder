//! AC2 (P0, PRD-autobuilder-receipt-write-verify): given a zero-byte
//! `flake-audit-receipt.json`, `autobuilder gate` names it `unreadable:
//! empty` on the summary line the operator journal captures verbatim, and
//! the written `release-receipt.json` lists it under `unreadable_receipts`
//! with `cause: "empty"` — never an unnamed `block`.
//!
//! The other 24 receipts are synthesized straight from `RECEIPT_SPECS` (the
//! same table `autobuilder gate` itself walks) so this test exercises the
//! real 25-receipt shape without hand-maintaining a second copy of it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::fs;
use std::path::Path;
use std::process::Command;

use autobuilder_gate::{ReceiptPath, ReceiptSpec, RECEIPT_SPECS};
use serde_json::json;
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

/// Build a passing receipt value for `spec` at `head_sha`, mirroring exactly
/// what `check_verdict` in `crates/gate` requires for that spec's name.
fn synth_value(spec: &ReceiptSpec, head_sha: &str) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("schema".into(), json!(spec.expected_schema));
    if spec.requires_head_match {
        obj.insert("head_sha".into(), json!(head_sha));
    }
    match spec.name {
        "risk-gate" => {
            obj.insert("blocking_count".into(), json!(0));
        }
        "reviewer-agent" => {
            obj.insert("decision".into(), json!("pass"));
        }
        "intake" => {} // presence + schema is the whole contract
        _ => {
            if !spec.pass_verdicts.is_empty() {
                obj.insert("verdict".into(), json!(spec.pass_verdicts[0]));
            }
        }
    }
    serde_json::Value::Object(obj)
}

fn receipt_path(receipts_dir: &Path, spec: &ReceiptSpec, head_sha: &str) -> std::path::PathBuf {
    match spec.file_name {
        ReceiptPath::Static(s) => receipts_dir.join(s),
        ReceiptPath::HeadShaJson => receipts_dir.join(format!("{head_sha}.json")),
    }
}

#[test]
fn rcptio_ac2_unreadable_zero_byte_receipt_named_block() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    fs::write(dir.join("README.md"), "fixture\n").unwrap();
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
            "commit", "-q", "-m", "init",
        ],
    );
    let head_sha = git_stdout(dir, &["rev-parse", "HEAD"]);

    let receipts_dir = dir.join("target/autobuilder/receipts");
    fs::create_dir_all(&receipts_dir).unwrap();

    for spec in RECEIPT_SPECS {
        let path = receipt_path(&receipts_dir, spec, &head_sha);
        if spec.name == "flake-audit" {
            // Simulate a write that failed partway and left an empty file
            // behind (the incident this PRD exists to close) — deliberately
            // NOT going through `autobuilder_receipt::write`, since the
            // point is that the READER must classify whatever is actually
            // on disk, however it got there.
            fs::write(&path, b"").unwrap();
        } else {
            let value = synth_value(spec, &head_sha);
            autobuilder_receipt::write(&path, value).unwrap();
        }
    }

    let output = autobuilder()
        .arg("gate")
        .arg("--project")
        .arg(dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "gate must exit non-zero when a receipt is unreadable; stdout={stdout}"
    );
    assert!(
        stdout.contains("unreadable=1"),
        "summary line must carry unreadable=1; stdout={stdout}"
    );
    assert!(
        stdout.contains("flake-audit: empty"),
        "summary line must name the unreadable receipt and its cause; stdout={stdout}"
    );
    // The per-check line must still name it too (never an empty blocker
    // name) — this is what gate-delta.sh's new_blocks parser keys on.
    assert!(
        stdout.contains("✗ flake-audit"),
        "per-check line must name flake-audit; stdout={stdout}"
    );

    let release_receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("target/autobuilder/release-receipt.json")).unwrap(),
    )
    .unwrap();
    let unreadable = release_receipt
        .get("unreadable_receipts")
        .and_then(serde_json::Value::as_array)
        .expect("unreadable_receipts must be an array");
    assert_eq!(unreadable.len(), 1, "exactly one unreadable receipt: {release_receipt}");
    assert_eq!(unreadable[0]["name"], "flake-audit");
    assert_eq!(unreadable[0]["cause"], "empty");

    assert_eq!(release_receipt["pass_count"], RECEIPT_SPECS.len() - 1);
    assert_eq!(release_receipt["block_count"], 1);
    assert_eq!(release_receipt["verdict"], "block");
}
