//! AC-X1: all 17 producer binaries respond to `--help` with exit 0.
//!
//! This is the unfakeable presence check the parent
//! `stage4_receipt_producers_callable` scalar will pick up. A fresh
//! checkout never has `--release` binaries pre-built (nothing in the
//! automated path runs `cargo build --release`), so this test builds the
//! debug binaries itself via `cargo build --bins` before asserting on
//! them — it must pass standalone under a plain `cargo test --workspace`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;

use autobuilder_extended_gates::PRODUCER_SPECS;

/// The workspace `target/` dir (two levels up from this crate's manifest).
fn workspace_target_dir() -> PathBuf {
    let mut here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.pop();
    here.pop();
    here.join("target")
}

/// Build every producer bin for this crate into `target/debug` and return
/// that directory. Uses the same `cargo` the outer `cargo test` invoked
/// (via the `CARGO` env var cargo sets for test binaries), so this works
/// offline/hermetically without assuming any prior out-of-band build.
fn ensure_bins_built(target_dir: &Path) -> PathBuf {
    let workspace_root = target_dir
        .parent()
        .expect("workspace target dir has a parent");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(&cargo)
        .args(["build", "--quiet", "-p", "autobuilder-extended-gates", "--bins"])
        .current_dir(workspace_root)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn `{cargo} build --bins`: {e}"));
    assert!(
        status.success(),
        "`cargo build --bins -p autobuilder-extended-gates` failed"
    );
    target_dir.join("debug")
}

#[test]
fn ac_x1_every_producer_bin_has_help() {
    let dir = ensure_bins_built(&workspace_target_dir());
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
        "missing debug binaries even after `cargo build --bins -p autobuilder-extended-gates`: {missing:?}"
    );
    assert!(
        failing.is_empty(),
        "binaries that do not respond to --help: {failing:?}"
    );
}
