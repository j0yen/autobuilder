//! AC-X3: `autobuilder-gate::RECEIPT_SPECS` covers all 16 extended-gates
//! producers, taking the total from 8 to 24.
//!
//! Asserts both directions: every producer's schema string appears in
//! `RECEIPT_SPECS`, and there are no orphan specs (claims that don't map
//! back to a producer).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use autobuilder_extended_gates::PRODUCER_SPECS;
use autobuilder_gate::{RECEIPT_SPECS, ReceiptPath};

#[test]
fn ac_x3_gate_has_24_receipts() {
    assert_eq!(
        RECEIPT_SPECS.len(),
        24,
        "expected 8 original + 16 extended = 24"
    );
}

#[test]
fn ac_x3_every_producer_is_in_gate() {
    let gate_schemas: BTreeMap<&str, &str> = RECEIPT_SPECS
        .iter()
        .map(|s| (s.expected_schema, s.name))
        .collect();
    for spec in PRODUCER_SPECS {
        let found = gate_schemas.get(spec.schema);
        assert!(
            found.is_some(),
            "producer {} schema {} not in gate RECEIPT_SPECS",
            spec.name,
            spec.schema
        );
        assert_eq!(
            found.copied(),
            Some(spec.name),
            "schema {} maps to a different name in gate vs producer table",
            spec.schema
        );
    }
}

#[test]
fn ac_x3_every_extended_gate_spec_maps_to_a_producer() {
    let producer_schemas: BTreeMap<&str, &str> = PRODUCER_SPECS
        .iter()
        .map(|s| (s.schema, s.name))
        .collect();
    let known_originals = [
        "autobuilder.intent_card.v1",
        "autobuilder.vti_plan_receipt.v1",
        "autobuilder.iteration_receipt.v1",
        "autobuilder.bad_rust_audit.v1",
        "autobuilder.reviewer_agent_receipt.v1",
        "autobuilder.rollback_plan_receipt.v1",
        "autobuilder.ci_checks_receipt.v1",
        "autobuilder.session_trace_receipt.v1",
    ];
    for spec in RECEIPT_SPECS {
        if known_originals.contains(&spec.expected_schema) {
            continue;
        }
        let found = producer_schemas.get(spec.expected_schema);
        assert!(
            found.is_some(),
            "gate spec {} ({}) has no matching producer",
            spec.name,
            spec.expected_schema
        );
    }
}

#[test]
fn ac_x4_every_producer_writes_to_canonical_path() {
    for spec in PRODUCER_SPECS {
        let in_gate = RECEIPT_SPECS
            .iter()
            .find(|s| s.expected_schema == spec.schema)
            .unwrap_or_else(|| panic!("no gate spec for {}", spec.name));
        match in_gate.file_name {
            ReceiptPath::Static(s) => assert_eq!(
                s, spec.file_name,
                "{}: gate file_name {} != producer file_name {}",
                spec.name, s, spec.file_name
            ),
            ReceiptPath::HeadShaJson => panic!(
                "{}: extended-gates producer should use Static file_name, got HeadShaJson",
                spec.name
            ),
        }
    }
}
