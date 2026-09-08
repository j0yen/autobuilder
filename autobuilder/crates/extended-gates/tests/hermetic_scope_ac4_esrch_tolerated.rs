//! PRD-rustbuild-hermetic-scope AC4 (P0) — ESRCH races during sampling are
//! tolerated.
//!
//! Given children exiting mid-sample, When the sampler walks the tree,
//! Then ESRCH races are tolerated and the run completes.
//!
//! The pure-function boundary (`socket_inodes`/`read_ppid`/`read_comm`
//! against a pid that no longer exists) already has a fast unit test in
//! `src/producers/hermetic_build.rs`. This is the integration-level
//! counterpart: a build script that spawns and reaps dozens of trivial
//! child processes in a tight loop, generating exactly the kind of
//! rapid-exit churn in the build's own process tree that would trigger
//! ENOENT/ESRCH races in `/proc/<pid>/{stat,comm,fd}` reads during
//! sampling. The assertion is simply that the producer completes without
//! erroring or panicking and still hands back a well-formed receipt — if
//! a race weren't tolerated, this would be the test that surfaces it (as
//! an `Err` bubbling out of `run_producer`, or a panic).
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
fn hermetic_scope_ac4_child_churn_does_not_error_or_panic() {
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
    // Spawn+reap 200 trivial child processes back to back: each spends
    // its entire life between two sample ticks in the common case, so the
    // sampler is very likely to observe at least a few pids mid-exit.
    std::fs::write(
        project.join("build.rs"),
        b"fn main() {\n\
          \x20   for _ in 0..200 {\n\
          \x20       let _ = std::process::Command::new(\"true\").status();\n\
          \x20   }\n\
          }\n",
    )
    .unwrap();

    let result = run_producer("hermetic-build", project);
    assert!(result.is_ok(), "producer must complete despite pid churn: {result:?}");

    let v = read_receipt(project, "hermetic-build-receipt.json");
    assert_eq!(
        v.get("cargo_exit_code").and_then(serde_json::Value::as_i64),
        Some(0)
    );
    assert_eq!(
        verdict_of(&v),
        "pass",
        "the churning children never open sockets, so this must still pass: {v:?}"
    );
}
