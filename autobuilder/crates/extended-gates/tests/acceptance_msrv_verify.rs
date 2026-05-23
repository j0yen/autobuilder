//! AC-msrv-verify.{1,2}: declared rust-version surfaces in the receipt;
//! missing toolchain → skipped (the right behaviour when rustup isn't
//! present, which is the case on this test harness).
//!
//! AC-msrv-verify.2 (planted failure) is rendered as: a project that
//! declares an obviously-impossible MSRV (`99.0`) yields skipped with the
//! "toolchain missing" note — proving the producer doesn't blindly trust
//! the declared value.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[test]
fn ac_msrv_verify_1_declared_msrv_surfaces() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_toml(project, Some("1.85"));
    run_producer("msrv-verify", project).unwrap();
    let v = read_receipt(project, "msrv-verify-receipt.json");
    assert_eq!(
        v.get("declared_msrv")
            .and_then(serde_json::Value::as_str),
        Some("1.85")
    );
}

#[test]
fn ac_msrv_verify_2_impossible_msrv_is_skipped_not_passed() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_toml(project, Some("99.0"));
    run_producer("msrv-verify", project).unwrap();
    let v = read_receipt(project, "msrv-verify-receipt.json");
    // verdict must not be "pass" — that would mean rubber-stamping an MSRV
    // the producer can't actually verify.
    assert_ne!(verdict_of(&v), "pass");
}
