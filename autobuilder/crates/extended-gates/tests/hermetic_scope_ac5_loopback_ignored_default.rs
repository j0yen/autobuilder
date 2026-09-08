//! PRD-rustbuild-hermetic-scope AC5 (P1) — loopback ignored by default.
//!
//! Given sccache active over its local socket, When a hermetic fixture
//! builds, Then loopback/unix traffic does not appear and the receipt
//! names the ignore rule.
//!
//! Stands in for "sccache active over its local socket" with a build
//! script that `connect()`s a UDP socket to `127.0.0.1` — the same shape
//! of local-daemon traffic sccache's rustc wrapper would produce, without
//! actually depending on sccache being installed. UDP because `connect()`
//! on `SOCK_DGRAM` only records a routing/socket-state association and
//! sends no packet, so no listening peer is needed.
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
fn hermetic_scope_ac5_loopback_not_attributed_by_default() {
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

    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");

    assert_eq!(
        verdict_of(&v),
        "pass",
        "a loopback-only build-tree connection must not block by default: {v:?}"
    );
    let new_sockets = v
        .get("new_sockets")
        .and_then(serde_json::Value::as_array)
        .expect("new_sockets must be an array");
    assert!(
        new_sockets.is_empty(),
        "loopback connection leaked into attribution: {new_sockets:?}"
    );
    let ignore_rules = v
        .get("ignore_rules")
        .and_then(serde_json::Value::as_array)
        .expect("ignore_rules must be an array");
    let names: Vec<&str> = ignore_rules.iter().filter_map(serde_json::Value::as_str).collect();
    assert!(
        names.contains(&"loopback"),
        "receipt must name the loopback ignore rule it applied: {names:?}"
    );
}
