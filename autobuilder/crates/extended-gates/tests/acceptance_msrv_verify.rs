//! AC-msrv-verify.{1,2,3,4}: declared rust-version surfaces in the receipt;
//! a missing toolchain is a mandatory-gate failure, not a silent skip; a
//! missing toolchain's receipt carries a `reason` + `remedy`; no declared
//! `rust-version` at all is the one case that stays `skipped`.
//!
//! AC-msrv-verify.2 (planted failure) is rendered as: a project that
//! declares an obviously-impossible MSRV (`99.0`) yields `block` (this
//! test harness has no rustup toolchain `99.0` installed), proving the
//! producer doesn't blindly trust the declared value — and does not
//! rubber-stamp a verification it never ran.

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
fn ac_msrv_verify_2_impossible_msrv_is_blocked_not_passed() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_toml(project, Some("99.0"));
    run_producer("msrv-verify", project).unwrap();
    let v = read_receipt(project, "msrv-verify-receipt.json");
    // verdict must not be "pass" — that would mean rubber-stamping an MSRV
    // the producer can't actually verify. A missing toolchain is now a
    // mandatory-gate failure, not a shrug: it must be "block", not
    // "skipped" either.
    assert_eq!(verdict_of(&v), "block");
}

#[test]
fn ac_msrv_verify_3_missing_toolchain_carries_reason_and_remedy() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_toml(project, Some("99.0"));
    run_producer("msrv-verify", project).unwrap();
    let v = read_receipt(project, "msrv-verify-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    assert_eq!(
        v.get("reason").and_then(serde_json::Value::as_str),
        Some("toolchain-missing: 99.0")
    );
    let remedy = v
        .get("remedy")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(
        remedy.contains("rustup toolchain install 99.0"),
        "remedy didn't name the install command: {remedy:?}"
    );
}

#[test]
fn ac_msrv_verify_4_no_rust_version_declared_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_cargo_toml(project, None);
    run_producer("msrv-verify", project).unwrap();
    let v = read_receipt(project, "msrv-verify-receipt.json");
    // The ONE case that stays skipped: nothing was declared, so there is
    // nothing to verify.
    assert_eq!(verdict_of(&v), "skipped");
}
