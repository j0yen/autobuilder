//! PRD-rustbuild-hermetic-scope AC2 (P0) — true-positive retention.
//!
//! Given a fixture whose build script opens a brief outbound TCP
//! connection, When hermetic-build runs, Then the verdict is block and the
//! entry carries the build-tree pid, comm, and remote address.
//!
//! The fixture's `build.rs` `connect()`s a UDP socket to the host's own
//! non-loopback interface (so the connection is genuinely "outbound"-shaped
//! and not filtered as loopback) and holds it ~400ms (long enough to
//! overlap several 150ms sample ticks reliably) before dropping it —
//! "brief" relative to the whole build, and it closes well before `cargo
//! build` exits. UDP rather than TCP deliberately: `connect()` on a
//! `SOCK_DGRAM` socket only records a routing/socket-state association
//! and sends no packet, so the test needs no listening peer and can't
//! flake on whether a real handshake completes.
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
fn hermetic_scope_ac2_buildscript_connection_is_attributed() {
    let Some(ip) = local_nonloopback_ip() else {
        eprintln!("skip: host has no non-loopback interface with a route");
        return;
    };
    let port = 54321;

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
    // The address is baked into the build script's source at fixture-write
    // time (rather than threaded through an env var) so the test needs no
    // unsafe env mutation.
    std::fs::write(
        project.join("build.rs"),
        format!(
            "fn main() {{\n\
             \x20   if let Ok(s) = std::net::UdpSocket::bind(\"0.0.0.0:0\") {{\n\
             \x20       if s.connect(\"{ip}:{port}\").is_ok() {{\n\
             \x20           std::thread::sleep(std::time::Duration::from_millis(400));\n\
             \x20       }}\n\
             \x20   }}\n\
             }}\n"
        ),
    )
    .unwrap();

    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");

    assert_eq!(
        verdict_of(&v),
        "block",
        "a build-tree connection must block even though cargo itself exits 0: {v:?}"
    );
    let new_sockets = v
        .get("new_sockets")
        .and_then(serde_json::Value::as_array)
        .expect("new_sockets must be an array");
    assert!(!new_sockets.is_empty(), "expected the build script's connection to be attributed");

    let entry = new_sockets
        .iter()
        .find(|s| {
            s.get("remote")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|r| r.starts_with(&format!("{ip}:{port}")))
        })
        .unwrap_or_else(|| panic!("no attributed socket matched {ip}:{port}: {new_sockets:?}"));
    let pid = entry.get("pid").and_then(serde_json::Value::as_i64).expect("pid field");
    assert!(pid > 0, "attributed pid must be a real (positive) build-tree pid: {entry:?}");
    let comm = entry.get("comm").and_then(serde_json::Value::as_str).expect("comm field");
    assert_ne!(comm, "", "comm must be populated");
}
