//! AC5 (P1, PRD-autobuilder-receipt-write-verify): the `rcptio` selftest
//! cases exist, are named, and are green. AC1–AC4's own tests
//! (`rcptio_ac1`..`rcptio_ac4`, this same `tests/` directory) ARE those
//! cases; this file is the fixture-set sanity check that ties them to the
//! 25-receipt table they all assume, so a future receipt added to
//! `RECEIPT_SPECS` without updating this PRD's fixtures fails loudly here
//! instead of silently under-testing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use autobuilder_gate::RECEIPT_SPECS;

#[test]
fn rcptio_ac5_selftest_fixture_set_named() {
    assert_eq!(
        RECEIPT_SPECS.len(),
        25,
        "rcptio's AC2/AC4 fixtures assume exactly 25 receipts (8 core + 17 \
         extended); update tests/rcptio_ac2_*.rs and tests/rcptio_ac4_*.rs \
         alongside any change to RECEIPT_SPECS"
    );
    assert!(
        RECEIPT_SPECS.iter().any(|s| s.name == "flake-audit"),
        "AC2's zero-byte fixture targets the flake-audit receipt by name \
         (the 2026-09-11 incident this PRD closes) — it must stay in the table"
    );
}
