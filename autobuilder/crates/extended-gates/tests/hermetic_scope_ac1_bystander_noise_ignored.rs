//! PRD-rustbuild-hermetic-scope AC1 (P0) — machine noise immunity.
//!
//! Given a hermetic fixture crate and a bystander process opening outbound
//! sockets during the build, When hermetic-build runs, Then the verdict is
//! pass and `new_sockets` is empty.
//!
//! The "bystander" here is the test harness process itself — never a
//! descendant of the `cargo build` child the producer spawns, exactly like
//! the co-running review session / tailscale / agorabus traffic on the
//! real workstation this PRD is fixing false blocks for. It holds a
//! non-loopback UDP "connection" (a routing/socket-state association;
//! `connect()` on `SOCK_DGRAM` never actually sends a packet, so this
//! needs no listening peer and can't itself flake on reachability) for the
//! whole build window; under the old machine-wide before/after diff this
//! would have shown up as a "new socket" and blocked. Under process-tree
//! attribution it must not, because the bystander's pid is never in the
//! build's descendant set.
//!
//! `#[ignore]`d: spawns a real `cargo build` subprocess. Run with
//! `cargo test -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use std::net::UdpSocket;

use autobuilder_extended_gates::run_producer;

#[cfg(target_os = "linux")]
#[test]
#[ignore]
fn hermetic_scope_ac1_bystander_noise_is_never_attributed() {
    let Some(ip) = local_nonloopback_ip() else {
        eprintln!("skip: host has no non-loopback interface with a route");
        return;
    };

    // The bystander socket: opened by the TEST process, not by anything
    // cargo will spawn. Held open (kept alive) across the whole build.
    let bystander = UdpSocket::bind("0.0.0.0:0").unwrap();
    bystander.connect((ip, 54321)).unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    scaffold_trivial_lib(project);

    run_producer("hermetic-build", project).unwrap();
    let v = read_receipt(project, "hermetic-build-receipt.json");

    drop(bystander);

    assert_eq!(
        verdict_of(&v),
        "pass",
        "a bystander process's socket must never be attributed to the build: {v:?}"
    );
    let new_sockets = v
        .get("new_sockets")
        .and_then(serde_json::Value::as_array)
        .expect("new_sockets must be an array");
    assert!(
        new_sockets.is_empty(),
        "bystander connection leaked into the build's attribution: {new_sockets:?}"
    );
}
