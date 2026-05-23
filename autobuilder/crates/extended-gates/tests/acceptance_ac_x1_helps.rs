//! AC-X1: all 16 producer binaries respond to `--help` with exit 0.
//!
//! This is the unfakeable presence check the parent
//! `stage4_receipt_producers_callable` scalar will pick up. Runs the
//! `target/release/<name>` binaries built by `cargo build --release`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use autobuilder_extended_gates::PRODUCER_SPECS;

fn release_dir() -> PathBuf {
    let mut here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.pop();
    here.pop();
    here.join("target/release")
}

#[test]
fn ac_x1_every_producer_bin_has_help() {
    let dir = release_dir();
    let mut missing: Vec<String> = Vec::new();
    let mut failing: Vec<String> = Vec::new();
    for spec in PRODUCER_SPECS {
        let bin = dir.join(spec.name);
        if !bin.exists() {
            missing.push(spec.name.to_owned());
            continue;
        }
        let output = Command::new(&bin)
            .arg("--help")
            .output()
            .unwrap_or_else(|e| panic!("spawn {} --help: {e}", spec.name));
        if !output.status.success() {
            failing.push(format!(
                "{} exit={:?} stderr={}",
                spec.name,
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "missing release binaries (run `cargo build --release -p autobuilder-extended-gates`): {missing:?}"
    );
    assert!(
        failing.is_empty(),
        "binaries that do not respond to --help: {failing:?}"
    );
}
