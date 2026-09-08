//! PRD-rustbuild-hermetic-scope AC8 (P2) — `--strict` also attributes
//! loopback.
//!
//! Given --strict, When a fixture talks to loopback, Then the verdict is
//! block with the loopback entry attributed.
//!
//! Drives the producer through `set_strict` (the same safe process-wide
//! flag `hermetic-build --strict` sets) rather than the CLI, since this is
//! a library-level integration test against `run_producer`. Loopback
//! traffic is simulated via a UDP `connect()` (no packet sent, no
//! listening peer needed) to `127.0.0.1`.
//!
//! `#[ignore]`d: spawns a real `cargo build` subprocess. Run with
//! `cargo test -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::producers::hermetic_build::set_strict;
use autobuilder_extended_gates::run_producer;

#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn hermetic_scope_ac8_strict_attributes_loopback() {
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
    std::fs::write(
        project.join("build.rs"),
        "fn main() {\n\
         \x20   if let Ok(s) = std::net::UdpSocket::bind(\"0.0.0.0:0\") {\n\
         \x20       if s.connect(\"127.0.0.1:54321\").is_ok() {\n\
         \x20           std::thread::sleep(std::time::Duration::from_millis(400));\n\
         \x20       }\n\
         \x20   }\n\
         }\n",
    )
    .unwrap();

    set_strict(true);
    let result = run_producer("hermetic-build", project);
    // Always restore the process-wide default before asserting, so a panic
    // below still leaves the flag sane for any other #[test] fn compiled
    // into this binary (there is only one today, but this is the discipline
    // that keeps that safe as the file grows).
    set_strict(false);
    result.unwrap();

    let v = read_receipt(project, "hermetic-build-receipt.json");

    assert_eq!(
        v.get("strict").and_then(serde_json::Value::as_bool),
        Some(true),
        "receipt must record strict=true: {v:?}"
    );
    assert_eq!(
        verdict_of(&v),
        "block",
        "--strict must attribute the loopback connection and block: {v:?}"
    );
    let new_sockets = v
        .get("new_sockets")
        .and_then(serde_json::Value::as_array)
        .expect("new_sockets must be an array");
    let has_loopback_entry = new_sockets.iter().any(|s| {
        s.get("remote")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|r| r.starts_with("127.0.0.1:54321"))
    });
    assert!(
        has_loopback_entry,
        "expected a loopback entry naming 127.0.0.1:54321: {new_sockets:?}"
    );
}
