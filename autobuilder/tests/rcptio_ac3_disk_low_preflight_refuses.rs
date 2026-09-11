//! AC3 (P0, PRD-autobuilder-receipt-write-verify): given a fake free-space
//! report of 2 GB, `autobuilder gate` refuses before touching a single
//! receipt — `gate  refused  (cause=disk-low free_gb=2)`, non-zero exit,
//! and `release-receipt.json` (the file the verdict cache is built from)
//! is never written, so the refusal can never be mistaken for a verdict.
//!
//! `AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE` is the fault-injection knob (the
//! "fake report") — production reads real free space via `df`; this test
//! only exercises the override so it never depends on the test host's
//! actual disk usage.

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
fn rcptio_ac3_disk_low_preflight_refuses() {
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

    let output = autobuilder()
        .arg("gate")
        .arg("--project")
        .arg(dir)
        .env("AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE", "2")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "gate must refuse (non-zero exit) below the free-space floor; stdout={stdout}"
    );
    assert!(
        stdout.contains("gate  refused  (cause=disk-low free_gb=2)"),
        "refusal line must name the cause and observed free_gb; stdout={stdout}"
    );
    assert!(
        !dir.join("target/autobuilder/release-receipt.json").exists(),
        "a refusal is not a verdict — release-receipt.json must never be written"
    );

    // A floor set below the (fake) reported free space is a no-op — the
    // refusal is threshold-gated, not unconditional.
    let output2 = autobuilder()
        .arg("gate")
        .arg("--project")
        .arg(dir)
        .env("AUTOBUILDER_RECEIPT_FREE_GB_OVERRIDE", "2")
        .env("AUTOBUILDER_RECEIPT_FREE_FLOOR_GB", "1")
        .output()
        .unwrap();
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    assert!(
        !stdout2.contains("disk-low"),
        "a floor below the reported free space must not refuse; stdout={stdout2}"
    );
}
