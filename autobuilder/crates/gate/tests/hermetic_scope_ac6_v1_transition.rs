//! PRD-rustbuild-hermetic-scope AC6 (P1) — the gate accepts a v1
//! hermetic-build receipt during the v1 -> v2 transition window, and the
//! v2 receipt (the now-canonical schema) as well.
//!
//! Given a v1 receipt on disk from before the ship, When autobuilder gate
//! reads it, Then it is accepted during the transition window.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use autobuilder_gate::{RECEIPT_SPECS, check_receipt_value};
use serde_json::json;

fn hermetic_build_spec() -> &'static autobuilder_gate::ReceiptSpec {
    RECEIPT_SPECS
        .iter()
        .find(|s| s.name == "hermetic-build")
        .expect("hermetic-build spec must be in RECEIPT_SPECS")
}

#[test]
fn hermetic_scope_ac6_v1_receipt_accepted_during_transition() {
    let spec = hermetic_build_spec();
    let head = "a".repeat(40);
    // Shape of a receipt this producer would have written before the
    // v2 bump: bare-string new_sockets, no per-socket attribution fields.
    let v1 = json!({
        "schema": "autobuilder.hermetic_build_receipt.v1",
        "verdict": "pass",
        "head_sha": head,
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
        "platform": "linux",
        "new_sockets": [],
        "cargo_exit_code": 0,
        "rustc_wrapper_disabled": true,
    });

    let check = check_receipt_value(spec, &v1, &head);
    assert!(
        check.schema_match,
        "v1 schema must still be accepted during the transition window: {:?}",
        check.notes
    );
    assert!(
        check.pass,
        "a structurally valid v1 receipt must pass the gate check: {:?}",
        check.notes
    );
}

#[test]
fn hermetic_scope_ac6_v2_receipt_is_the_canonical_schema() {
    let spec = hermetic_build_spec();
    assert_eq!(
        spec.expected_schema, "autobuilder.hermetic_build_receipt.v2",
        "hermetic-build's canonical schema must be v2 post-ship"
    );

    let head = "b".repeat(40);
    let v2 = json!({
        "schema": "autobuilder.hermetic_build_receipt.v2",
        "verdict": "pass",
        "head_sha": head,
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
        "platform": "linux",
        "new_sockets": [],
        "cargo_exit_code": 0,
        "rustc_wrapper_disabled": true,
        "sample_interval_ms": 150,
        "ignore_rules": ["loopback", "unix-domain"],
        "strict": false,
    });

    let check = check_receipt_value(spec, &v2, &head);
    assert!(check.schema_match, "{:?}", check.notes);
    assert!(check.pass, "{:?}", check.notes);
}

#[test]
fn hermetic_scope_ac6_unrelated_schema_still_rejected() {
    // Negative control: the acceptance of v1 during transition must not
    // widen into accepting arbitrary schema strings.
    let spec = hermetic_build_spec();
    let head = "c".repeat(40);
    let bogus = json!({
        "schema": "autobuilder.hermetic_build_receipt.v99-not-real",
        "verdict": "pass",
        "head_sha": head,
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
    });
    let check = check_receipt_value(spec, &bogus, &head);
    assert!(!check.schema_match);
    assert!(!check.pass);
}
