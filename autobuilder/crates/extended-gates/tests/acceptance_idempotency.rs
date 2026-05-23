//! AC-N.3 (idempotency) for the 11 light-weight producers.
//!
//! For each producer: build a minimal happy-path fixture, run the producer
//! twice, strip the two legitimately-volatile fields (`captured_at`,
//! `receipt_digest`), and assert the receipts are byte-identical.
//!
//! Heavy producers (determinism, cold-build-time, mutation-kill,
//! flake-audit, hermetic-build) are covered in `acceptance_heavy_ignored.rs`
//! as `#[ignore]`d tests because their .3 invariant requires real cargo
//! invocations.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn fixture_secrets_scan(project: &std::path::Path) {
    init_git(project);
    std::fs::write(project.join("README.md"), b"clean\n").unwrap();
}

fn fixture_sbom(project: &std::path::Path) {
    init_git(project);
    write_cargo_lock(
        project,
        &[("anyhow", "1.0.0", Some("MIT OR Apache-2.0"))],
    );
}

fn fixture_license_audit(project: &std::path::Path) {
    init_git(project);
    write_cargo_lock(project, &[("anyhow", "1.0.0", Some("MIT"))]);
}

fn fixture_supply_audit(project: &std::path::Path) {
    init_git(project);
    write_cargo_lock(project, &[("anyhow", "1.0.0", None)]);
    // No advisories vendored → producer passes cleanly.
    std::fs::create_dir_all(project.join("vendor/rustsec")).unwrap();
}

fn fixture_msrv_verify(project: &std::path::Path) {
    init_git(project);
    // No rust-version declared → producer skips cleanly.
    write_cargo_toml(project, None);
}

fn fixture_binary_size(project: &std::path::Path) {
    init_git(project);
    let dir = project.join("target/release");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tiny"), vec![0u8; 100]).unwrap();
    std::fs::write(
        project.join("extended-gates.toml"),
        b"default_max_bytes = 50000\n",
    )
    .unwrap();
}

#[cfg(unix)]
fn fixture_cli_surface(project: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    init_git(project);
    let dir = project.join("target/release");
    std::fs::create_dir_all(&dir).unwrap();
    let script = "#!/bin/sh\nprintf 'help\\n'\n";
    let path = dir.join("foo");
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    let snap_dir = project.join("cli-surface-snapshots");
    std::fs::create_dir_all(&snap_dir).unwrap();
    std::fs::write(snap_dir.join("foo.txt"), "help\n").unwrap();
}

fn fixture_ac_traceability(project: &std::path::Path) {
    init_git(project);
    std::fs::write(project.join("PRD-fix.md"), "AC1 (MUST): foo.\n").unwrap();
    std::fs::create_dir_all(project.join("tests")).unwrap();
    std::fs::write(project.join("tests/t.rs"), "#[test] fn ac1_works() {}").unwrap();
}

fn fixture_schema_compat(project: &std::path::Path) {
    init_git(project);
    std::fs::create_dir_all(project.join("schemas")).unwrap();
    std::fs::write(
        project.join("schemas/s.json"),
        r#"{"required":["a"],"properties":{"a":{"type":"string"}}}"#,
    )
    .unwrap();
    commit_all(project, "v1");
    commit_all(project, "v2");
}

fn fixture_semver_check(project: &std::path::Path) {
    init_git(project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    std::fs::write(
        project.join("extended-gates.toml"),
        b"semver_expected_bump = \"patch\"\n",
    )
    .unwrap();
    commit_all(project, "v1");
    commit_all(project, "v2");
}

fn fixture_bench_delta(project: &std::path::Path) {
    init_git(project);
    std::fs::write(
        project.join("extended-gates.bench-baseline.json"),
        r#"{"decode":1000.0}"#,
    )
    .unwrap();
    let dir = project.join("target/criterion/decode/new");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("estimates.json"),
        r#"{"mean":{"point_estimate":1010.0}}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("extended-gates.toml"),
        b"bench_max_regression_pct = 5.0\n",
    )
    .unwrap();
}

fn assert_idempotent(
    producer: &str,
    receipt: &str,
    setup: impl Fn(&std::path::Path),
) {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    setup(project);
    run_producer(producer, project).unwrap();
    let mut a = read_receipt(project, receipt);
    // Sleep briefly so `captured_at` would normally differ — proving the
    // strip is necessary for the equality and that the rest of the receipt
    // is genuinely stable.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    run_producer(producer, project).unwrap();
    let mut b = read_receipt(project, receipt);
    strip_volatile(&mut a);
    strip_volatile(&mut b);
    assert_eq!(
        a, b,
        "{producer} receipt should be byte-identical run-to-run after stripping volatile fields"
    );
}

#[test]
fn ac_secrets_scan_3() {
    assert_idempotent(
        "secrets-scan",
        "secrets-scan-receipt.json",
        fixture_secrets_scan,
    );
}

#[test]
fn ac_sbom_3() {
    assert_idempotent("sbom", "sbom-receipt.json", fixture_sbom);
}

#[test]
fn ac_license_audit_3() {
    assert_idempotent(
        "license-audit",
        "license-audit-receipt.json",
        fixture_license_audit,
    );
}

#[test]
fn ac_supply_audit_3() {
    assert_idempotent(
        "supply-audit",
        "supply-audit-receipt.json",
        fixture_supply_audit,
    );
}

#[test]
fn ac_msrv_verify_3() {
    assert_idempotent(
        "msrv-verify",
        "msrv-verify-receipt.json",
        fixture_msrv_verify,
    );
}

#[test]
fn ac_binary_size_3() {
    assert_idempotent(
        "binary-size",
        "binary-size-receipt.json",
        fixture_binary_size,
    );
}

#[cfg(unix)]
#[test]
fn ac_cli_surface_3() {
    assert_idempotent(
        "cli-surface",
        "cli-surface-receipt.json",
        fixture_cli_surface,
    );
}

#[test]
fn ac_ac_traceability_3() {
    assert_idempotent(
        "ac-traceability",
        "ac-traceability-receipt.json",
        fixture_ac_traceability,
    );
}

#[test]
fn ac_schema_compat_3() {
    assert_idempotent(
        "schema-compat",
        "schema-compat-receipt.json",
        fixture_schema_compat,
    );
}

#[test]
fn ac_semver_check_3() {
    assert_idempotent(
        "semver-check",
        "semver-check-receipt.json",
        fixture_semver_check,
    );
}

#[test]
fn ac_bench_delta_3() {
    assert_idempotent(
        "bench-delta",
        "bench-delta-receipt.json",
        fixture_bench_delta,
    );
}
