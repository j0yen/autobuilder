//! PRD-autobuilder-rollback-tag-aware AC6 (P0) — the gate accepts a v1
//! rollback-plan receipt during the v1 -> v2 transition window, and the v2
//! receipt (the now-canonical schema) as well. Mirrors
//! `hermetic_scope_ac6_v1_transition.rs`'s v1/v2 transition pattern.
//!
//! Given a v1 receipt on disk, When `autobuilder gate` reads it, Then it is
//! accepted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use autobuilder_gate::{RECEIPT_SPECS, check_receipt_value};
use serde_json::json;

fn rollback_plan_spec() -> &'static autobuilder_gate::ReceiptSpec {
    RECEIPT_SPECS
        .iter()
        .find(|s| s.name == "rollback-plan")
        .expect("rollback-plan spec must be in RECEIPT_SPECS")
}

#[test]
fn rollback_tag_ac6_v1_receipt_accepted_during_transition() {
    let spec = rollback_plan_spec();
    let head = "a".repeat(40);
    // Shape of a receipt the producer would have written before the v2
    // bump: no rollback_model / rollback_target / tag_lineage fields.
    let v1 = json!({
        "schema": "autobuilder.rollback_plan_receipt.v1",
        "head_sha": head,
        "base_ref": "v0.1.0",
        "base_sha": "b".repeat(40),
        "base_tag": "v0.1.0",
        "base_note": null,
        "rollback_md": "target/autobuilder/rollback.md",
        "commit_count": 0,
        "revertable_count": 0,
        "blocking_count": 0,
        "verdict": "pass",
        "commits": [],
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
    });

    let check = check_receipt_value(spec, &v1, &head);
    assert!(check.schema_match, "v1 schema must still be accepted during the transition window: {:?}", check.notes);
    assert!(check.pass, "a structurally valid v1 receipt must pass the gate check: {:?}", check.notes);
}

#[test]
fn rollback_tag_ac6_v2_receipt_is_the_canonical_schema() {
    let spec = rollback_plan_spec();
    assert_eq!(
        spec.expected_schema, "autobuilder.rollback_plan_receipt.v2",
        "rollback-plan's canonical schema must be v2 post-ship"
    );

    let head = "c".repeat(40);
    let v2 = json!({
        "schema": "autobuilder.rollback_plan_receipt.v2",
        "head_sha": head,
        "base_ref": "v0.1.0",
        "base_sha": "d".repeat(40),
        "base_tag": "v0.1.0",
        "base_note": null,
        "rollback_md": "target/autobuilder/rollback.md",
        "commit_count": 0,
        "revertable_count": 0,
        "blocking_count": 0,
        "verdict": "pass",
        "commits": [],
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
        "rollback_model": "redeploy-tag",
        "rollback_target": {
            "tag": "v0.1.0",
            "sha": "d".repeat(40),
            "redeploy_command": "svc-deploy redeploy --tag v0.1.0",
        },
        "tag_lineage": [],
    });

    let check = check_receipt_value(spec, &v2, &head);
    assert!(check.schema_match, "{:?}", check.notes);
    assert!(check.pass, "{:?}", check.notes);
}

#[test]
fn rollback_tag_ac6_unrelated_schema_still_rejected() {
    // Negative control: accepting v1 during transition must not widen into
    // accepting arbitrary schema strings.
    let spec = rollback_plan_spec();
    let head = "e".repeat(40);
    let bogus = json!({
        "schema": "autobuilder.rollback_plan_receipt.v99-not-real",
        "head_sha": head,
        "verdict": "pass",
        "captured_at": "2026-01-01T00:00:00Z",
        "receipt_digest": "deadbeef",
    });
    let check = check_receipt_value(spec, &bogus, &head);
    assert!(!check.schema_match);
    assert!(!check.pass);
}
