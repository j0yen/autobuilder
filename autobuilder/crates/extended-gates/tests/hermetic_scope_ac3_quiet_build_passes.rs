//! PRD-rustbuild-hermetic-scope AC3 (P0) — a quiet build (no network
//! activity anywhere, in the tree or off it) passes with empty
//! `new_sockets`.
//!
//! `#[ignore]`d like the other hermetic-build ACs: spawns a real `cargo
//! build` subprocess, too slow for the default test run. Run with
//! `cargo test -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn hermetic_scope_ac3_quiet_build_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);

    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");

    assert_eq!(
        verdict_of(&v),
        "pass",
        "a build with no network activity anywhere must pass under\
         process-tree attribution (machine-wide noise no longer matters): {v:?}"
    );
    let new_sockets = v
        .get("new_sockets")
        .and_then(serde_json::Value::as_array)
        .expect("new_sockets must be an array");
    assert!(new_sockets.is_empty(), "expected no attributed sockets: {new_sockets:?}");
    assert_eq!(
        v.get("cargo_exit_code").and_then(serde_json::Value::as_i64),
        Some(0)
    );
}
