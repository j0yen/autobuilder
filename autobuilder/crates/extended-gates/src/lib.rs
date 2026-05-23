//! 16 receipt producers extending the autobuilder risk gate from 8 to 24.
//!
//! Each producer audits one class of failure mode the existing 8-receipt
//! gate does not catch: supply-chain (4), reproducibility (3), performance
//! (3), API contract (3), test quality (3). Every producer writes
//! `target/autobuilder/receipts/<name>-receipt.json` via
//! [`autobuilder_receipt::write`] so the digest-binding and timestamp
//! formatting are reused, not reinvented.
//!
//! The [`PRODUCER_SPECS`] table is consumed by `autobuilder-gate`'s
//! `RECEIPT_SPECS` extension to add the 16 new specs to the 8-receipt
//! aggregator without duplicating the schema strings or pass-verdict
//! allowlists.
//!
//! ## Hard discipline
//!
//! - Pure-Rust in-tree: no shell-out to `cargo-audit`, `cargo-deny`,
//!   `gitleaks`, `cargo-semver-checks`, or `cargo-mutants`. The toolchain
//!   binaries (`cargo`, `git`) are allowed because they are build deps the
//!   workspace already assumes.
//! - Every receipt is digest-bound via [`autobuilder_receipt::write`].
//! - Every producer reports `verdict ∈ {"pass", "block", "skipped"}` per its
//!   [`ProducerSpec`] entry; unknown verdicts fail the gate aggregator.

#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod prelude;
pub mod producers;

pub use prelude::{ProducerSpec, run_producer};

/// One row per producer. Consumed by `autobuilder-gate`'s `RECEIPT_SPECS`
/// extension.
///
/// Adding a producer: append a [`ProducerSpec`] entry here, add the matching
/// module under [`producers`], add a `[[bin]]` entry in `Cargo.toml`, and a
/// thin `src/bin/<snake_case_name>.rs` that calls [`run_producer`].
pub const PRODUCER_SPECS: &[ProducerSpec] = &[
    // --- supply-chain (4) ---
    ProducerSpec {
        name: "supply-audit",
        schema: "autobuilder.supply_audit_receipt.v1",
        file_name: "supply-audit-receipt.json",
        pass_verdicts: &["pass"],
    },
    ProducerSpec {
        name: "license-audit",
        schema: "autobuilder.license_audit_receipt.v1",
        file_name: "license-audit-receipt.json",
        pass_verdicts: &["pass"],
    },
    ProducerSpec {
        name: "secrets-scan",
        schema: "autobuilder.secrets_scan_receipt.v1",
        file_name: "secrets-scan-receipt.json",
        pass_verdicts: &["pass"],
    },
    ProducerSpec {
        name: "sbom",
        schema: "autobuilder.sbom_receipt.v1",
        file_name: "sbom-receipt.json",
        pass_verdicts: &["pass"],
    },
    // --- reproducibility (3) ---
    ProducerSpec {
        name: "determinism",
        schema: "autobuilder.determinism_receipt.v1",
        file_name: "determinism-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "hermetic-build",
        schema: "autobuilder.hermetic_build_receipt.v1",
        file_name: "hermetic-build-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "msrv-verify",
        schema: "autobuilder.msrv_verify_receipt.v1",
        file_name: "msrv-verify-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    // --- performance (3) ---
    ProducerSpec {
        name: "binary-size",
        schema: "autobuilder.binary_size_receipt.v1",
        file_name: "binary-size-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "cold-build-time",
        schema: "autobuilder.cold_build_time_receipt.v1",
        file_name: "cold-build-time-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "bench-delta",
        schema: "autobuilder.bench_delta_receipt.v1",
        file_name: "bench-delta-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    // --- API contract (3) ---
    ProducerSpec {
        name: "semver-check",
        schema: "autobuilder.semver_check_receipt.v1",
        file_name: "semver-check-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "cli-surface",
        schema: "autobuilder.cli_surface_receipt.v1",
        file_name: "cli-surface-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "schema-compat",
        schema: "autobuilder.schema_compat_receipt.v1",
        file_name: "schema-compat-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    // --- test quality (3) ---
    ProducerSpec {
        name: "ac-traceability",
        schema: "autobuilder.ac_traceability_receipt.v1",
        file_name: "ac-traceability-receipt.json",
        pass_verdicts: &["pass"],
    },
    ProducerSpec {
        name: "mutation-kill",
        schema: "autobuilder.mutation_kill_receipt.v1",
        file_name: "mutation-kill-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
    ProducerSpec {
        name: "flake-audit",
        schema: "autobuilder.flake_audit_receipt.v1",
        file_name: "flake-audit-receipt.json",
        pass_verdicts: &["pass", "skipped"],
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod table_tests {
    use super::PRODUCER_SPECS;

    #[test]
    fn ac_x2_table_has_16_unique_entries() {
        assert_eq!(PRODUCER_SPECS.len(), 16, "expected 16 producer specs");
        let mut names: Vec<_> = PRODUCER_SPECS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 16, "producer names must be unique");
    }

    #[test]
    fn ac_x2_schemas_are_well_formed() {
        for spec in PRODUCER_SPECS {
            assert!(
                spec.schema.starts_with("autobuilder."),
                "schema must start with autobuilder.: {}",
                spec.schema
            );
            assert!(
                spec.schema.ends_with("_receipt.v1"),
                "schema must end with _receipt.v1: {}",
                spec.schema
            );
            assert!(
                !spec.pass_verdicts.is_empty(),
                "{} has empty pass_verdicts",
                spec.name
            );
            assert!(
                spec.file_name.ends_with("-receipt.json"),
                "{} file_name must end with -receipt.json: {}",
                spec.name,
                spec.file_name
            );
        }
    }
}
