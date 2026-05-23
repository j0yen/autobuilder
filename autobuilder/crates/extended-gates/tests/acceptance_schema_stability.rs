//! AC-N.4 (schema stability) for the 11 light-weight producers.
//!
//! Every receipt declares a fixed set of top-level keys (in addition to the
//! common envelope: `schema`, `verdict`, `head_sha`, `captured_at`,
//! `receipt_digest`). This test asserts the declared set matches what the
//! producer actually emits — no accidental fields, no drift.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

const ENVELOPE: &[&str] = &[
    "schema",
    "verdict",
    "head_sha",
    "captured_at",
    "receipt_digest",
];

/// (producer name, receipt file, expected payload keys (sorted)).
const SHAPES: &[(&str, &str, &[&str])] = &[
    (
        "secrets-scan",
        "secrets-scan-receipt.json",
        &["files_scanned", "findings"],
    ),
    (
        "sbom",
        "sbom-receipt.json",
        &["bom", "components_count"],
    ),
    (
        "license-audit",
        "license-audit-receipt.json",
        &[
            "allowlist",
            "deps_scanned",
            "deps_unknown_license",
            "violations",
        ],
    ),
    (
        "supply-audit",
        "supply-audit-receipt.json",
        &[
            "advisories_found",
            "advisories_loaded",
            "advisory_db_ref",
            "deps_scanned",
            "ignored_advisories",
        ],
    ),
    (
        "msrv-verify",
        "msrv-verify-receipt.json",
        &[
            "cargo_check_exit",
            "declared_msrv",
            "note",
            "toolchain_available",
        ],
    ),
    (
        "binary-size",
        "binary-size-receipt.json",
        &["default_max_bytes", "measurements", "over_budget"],
    ),
    (
        "ac-traceability",
        "ac-traceability-receipt.json",
        &[
            "ac_ids",
            "prd_path",
            "tests_per_ac",
            "untraced_ac_ids",
        ],
    ),
    (
        "schema-compat",
        "schema-compat-receipt.json",
        &["incompatibilities", "schemas_checked"],
    ),
    (
        "semver-check",
        "semver-check-receipt.json",
        &[
            "additions",
            "base_ref",
            "breaking_changes",
            "compatibility",
            "expected_bump",
            "head_ref",
        ],
    ),
    (
        "bench-delta",
        "bench-delta-receipt.json",
        &[
            "baseline_path",
            "comparisons",
            "max_regression_pct",
            "regressed",
        ],
    ),
];

fn setup_minimal(producer: &str, project: &std::path::Path) {
    init_git(project);
    match producer {
        "secrets-scan" => {
            std::fs::write(project.join("README.md"), b"x\n").unwrap();
        }
        "sbom" | "license-audit" | "supply-audit" => {
            write_cargo_lock(project, &[("a", "1.0.0", Some("MIT"))]);
            std::fs::create_dir_all(project.join("vendor/rustsec")).unwrap();
        }
        "msrv-verify" => {
            write_cargo_toml(project, None);
        }
        "binary-size" => {
            let dir = project.join("target/release");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("tiny"), b"hi").unwrap();
            std::fs::write(
                project.join("extended-gates.toml"),
                b"default_max_bytes = 50000\n",
            )
            .unwrap();
        }
        "ac-traceability" => {
            std::fs::write(project.join("PRD-x.md"), "AC1: foo.\n").unwrap();
            std::fs::create_dir_all(project.join("tests")).unwrap();
            std::fs::write(project.join("tests/t.rs"), "#[test] fn ac1_w() {}").unwrap();
        }
        "schema-compat" => {
            std::fs::create_dir_all(project.join("schemas")).unwrap();
            std::fs::write(
                project.join("schemas/s.json"),
                r#"{"required":["a"],"properties":{"a":{"type":"string"}}}"#,
            )
            .unwrap();
            commit_all(project, "v1");
            commit_all(project, "v2");
        }
        "semver-check" => {
            std::fs::create_dir_all(project.join("src")).unwrap();
            std::fs::write(project.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
            commit_all(project, "v1");
            commit_all(project, "v2");
        }
        "bench-delta" => {
            std::fs::write(
                project.join("extended-gates.bench-baseline.json"),
                r#"{"x":1000.0}"#,
            )
            .unwrap();
            let dir = project.join("target/criterion/x/new");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("estimates.json"),
                r#"{"mean":{"point_estimate":1010.0}}"#,
            )
            .unwrap();
        }
        _ => {}
    }
}

#[test]
fn ac_n4_schema_stability_for_every_light_producer() {
    let mut failures: Vec<String> = Vec::new();
    for (producer, receipt_file, expected_payload) in SHAPES {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        setup_minimal(producer, project);
        if let Err(e) = run_producer(producer, project) {
            failures.push(format!("{producer}: run failed: {e}"));
            continue;
        }
        let value = read_receipt(project, receipt_file);
        let actual = key_set(&value);
        let mut expected: std::collections::BTreeSet<String> = ENVELOPE
            .iter()
            .chain(expected_payload.iter())
            .map(|s| (*s).to_owned())
            .collect();
        // schema_observed/match etc don't appear in producer receipts; only
        // the declared envelope + payload keys do.
        let extra: Vec<&String> = actual.difference(&expected).collect();
        let missing: Vec<&String> = expected.difference(&actual).collect();
        if !extra.is_empty() || !missing.is_empty() {
            failures.push(format!(
                "{producer}: extra={extra:?} missing={missing:?}"
            ));
        }
        expected.clear();
    }
    assert!(
        failures.is_empty(),
        "schema stability violations: {failures:#?}"
    );
}
