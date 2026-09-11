//! AC4 (P0, PRD-autobuilder-receipt-write-verify): given all 25 receipts
//! readable and passing, `autobuilder gate` behaves exactly as it did
//! before this PRD — `unreadable_receipts` is empty and the verdict is
//! `pass` — the new classification never fires a false positive on a
//! healthy run.

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
        "intake" => {}
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
fn rcptio_ac4_all_readable_receipts_unaffected() {
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
        let value = synth_value(spec, &head_sha);
        autobuilder_receipt::write(&path, value).unwrap();
    }

    let output = autobuilder()
        .arg("gate")
        .arg("--project")
        .arg(dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "gate must pass when every receipt is readable and passing; stdout={stdout}"
    );
    assert!(
        stdout.contains("unreadable=0"),
        "summary line must carry unreadable=0 on a healthy run; stdout={stdout}"
    );

    let release_receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("target/autobuilder/release-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        release_receipt["unreadable_receipts"],
        serde_json::json!([]),
        "no receipt should be classified unreadable: {release_receipt}"
    );
    assert_eq!(release_receipt["verdict"], "pass");
    assert_eq!(release_receipt["block_count"], 0);
    assert_eq!(release_receipt["pass_count"], RECEIPT_SPECS.len());
}
