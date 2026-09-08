//! PRD-rustbuild-hermetic-scope AC7 (P0) — cargo exiting nonzero.
//!
//! Given cargo exits nonzero, When the producer finishes, Then the receipt
//! records the exit code and the attribution list gathered up to that
//! point.
//!
//! `#[ignore]`d: spawns a real `cargo build` subprocess. Run with
//! `cargo test -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn hermetic_scope_ac7_nonzero_cargo_exit_is_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .unwrap();
    std::fs::write(project.join("src/lib.rs"), b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n")
        .unwrap();
    // A build script that fails: cargo reports a nonzero exit without ever
    // reaching a linked binary.
    std::fs::write(
        project.join("build.rs"),
        b"fn main() { std::process::exit(7); }\n",
    )
    .unwrap();

    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");

    assert_eq!(
        verdict_of(&v),
        "block",
        "a nonzero cargo exit must block regardless of socket activity: {v:?}"
    );
    // build.rs's own exit code (7) doesn't necessarily equal cargo's exit
    // code (cargo wraps failed build-script runs into its own nonzero
    // status), so just assert it's present and nonzero.
    let exit = v
        .get("cargo_exit_code")
        .and_then(serde_json::Value::as_i64)
        .expect("cargo_exit_code must be present even on a build-script failure");
    assert_ne!(exit, 0, "expected a nonzero cargo exit code: {v:?}");
    // The attribution list must still be a (possibly empty) array, not
    // absent — "attribution list still emitted" per the PRD's edge case.
    assert!(
        v.get("new_sockets").and_then(serde_json::Value::as_array).is_some(),
        "new_sockets must still be emitted on a crashed build: {v:?}"
    );
}
