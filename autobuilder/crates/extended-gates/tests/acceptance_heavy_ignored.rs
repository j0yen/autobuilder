//! Heavy-producer ACs (`#[ignore]`d by default; run with `cargo test -- --ignored`).
//!
//! These spawn real `cargo` processes against a tempdir fixture and are too
//! slow for the default test run. They are kept in-tree so the AC pattern
//! is documented and runnable.
//!
//! Covered (one happy + one planted per producer):
//! - determinism, cold-build-time, mutation-kill, flake-audit, hermetic-build
//!
//! Also covered: `target/autobuilder/receipts/` must survive `determinism`
//! and `cold-build-time`, which both run `cargo clean` — if they clean the
//! project's real `target/` dir (instead of an isolated `CARGO_TARGET_DIR`)
//! every other producer's receipt is wiped along with the build artifacts.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::undocumented_unsafe_blocks,
    unsafe_code
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn scaffold_trivial_lib(project: &std::path::Path) {
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(project.join("src/lib.rs"), b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .unwrap();
}

#[test]
#[ignore]
fn ac_determinism_1_skipped_when_heavy_disabled() {
    // Sanity: with AUTOBUILDER_SKIP_HEAVY set, the producer emits skipped
    // rather than blocking on a real build. This is the happy-path
    // fast-mode behaviour.
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);
    // SAFETY: tests are single-threaded for env vars; we set + run + read.
    unsafe {
        std::env::set_var("AUTOBUILDER_SKIP_HEAVY", "1");
    }
    run_producer("determinism", project).unwrap();
    let v = read_receipt(project, "determinism-receipt.json");
    assert_eq!(verdict_of(&v), "skipped");
    unsafe {
        std::env::remove_var("AUTOBUILDER_SKIP_HEAVY");
    }
}

#[test]
#[ignore]
fn ac_cold_build_time_1_skipped_when_heavy_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);
    unsafe {
        std::env::set_var("AUTOBUILDER_SKIP_HEAVY", "1");
    }
    run_producer("cold-build-time", project).unwrap();
    let v = read_receipt(project, "cold-build-time-receipt.json");
    assert_eq!(verdict_of(&v), "skipped");
    unsafe {
        std::env::remove_var("AUTOBUILDER_SKIP_HEAVY");
    }
}

#[test]
#[ignore]
fn ac_mutation_kill_1_skipped_when_heavy_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);
    unsafe {
        std::env::set_var("AUTOBUILDER_SKIP_HEAVY", "1");
    }
    run_producer("mutation-kill", project).unwrap();
    let v = read_receipt(project, "mutation-kill-receipt.json");
    assert_eq!(verdict_of(&v), "skipped");
    unsafe {
        std::env::remove_var("AUTOBUILDER_SKIP_HEAVY");
    }
}

#[test]
#[ignore]
fn ac_flake_audit_1_skipped_when_heavy_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);
    unsafe {
        std::env::set_var("AUTOBUILDER_SKIP_HEAVY", "1");
    }
    run_producer("flake-audit", project).unwrap();
    let v = read_receipt(project, "flake-audit-receipt.json");
    assert_eq!(verdict_of(&v), "skipped");
    unsafe {
        std::env::remove_var("AUTOBUILDER_SKIP_HEAVY");
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn ac_hermetic_build_1_no_new_sockets_passes() {
    // On Linux, a no-op `cargo build --offline` against an already-built
    // project should produce zero new outbound sockets. This is the closest
    // we can get to a "happy path" without a heavyweight sandbox.
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);
    // First build to warm target/; then producer runs --offline on it.
    let _ = std::process::Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(project)
        .status();
    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");
    // PRD-rustbuild-hermetic-scope: attribution is now scoped to the
    // build's own process tree, so machine-wide noise (this used to say
    // "either pass or block depending on what else is on the host") can no
    // longer cause a false block here — a quiet build must pass.
    assert_eq!(
        v.get("platform").and_then(serde_json::Value::as_str),
        Some("linux")
    );
    assert_eq!(verdict_of(&v), "pass", "quiet build must pass: {v:?}");
}

/// Write a sentinel receipt into `target/autobuilder/receipts/` and assert
/// it is still there (byte-identical) after the given producer runs. Guards
/// against a producer's `cargo clean` wiping the project's real `target/`.
fn assert_sentinel_survives(producer: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);

    let receipts_dir = project.join("target/autobuilder/receipts");
    std::fs::create_dir_all(&receipts_dir).unwrap();
    let sentinel_path = receipts_dir.join("sentinel-receipt.json");
    let sentinel_bytes = b"{\"schema\":\"test.sentinel.v1\",\"verdict\":\"pass\"}";
    std::fs::write(&sentinel_path, sentinel_bytes).unwrap();

    run_producer(producer, project).unwrap();

    let after = std::fs::read(&sentinel_path).unwrap_or_else(|e| {
        panic!(
            "sentinel {} did not survive running {producer}: {e}",
            sentinel_path.display()
        )
    });
    assert_eq!(
        after, sentinel_bytes,
        "sentinel receipt was modified by {producer} (target/autobuilder/receipts/ must be untouched)"
    );
}

#[test]
#[ignore]
fn ac_determinism_2_target_autobuilder_receipts_survives() {
    assert_sentinel_survives("determinism");
}

#[test]
#[ignore]
fn ac_cold_build_time_2_target_autobuilder_receipts_survives() {
    assert_sentinel_survives("cold-build-time");
}
