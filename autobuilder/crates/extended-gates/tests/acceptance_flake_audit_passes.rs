//! AC-flake-audit-passes: `--passes N` overrides the configured run count
//! and the receipt gains a `passes` field recording the number actually
//! run.
//!
//! Uses a fake "cargo" shell-script shim on `PATH` (exit 0 instantly) so the
//! test doesn't shell out to the real toolchain -- matches the
//! `write_shell_bin` fake-binary idiom used by `acceptance_cli_surface.rs`,
//! applied to `cargo` instead of a producer binary.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::undocumented_unsafe_blocks,
    unsafe_code
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::ProducerSpec;
use autobuilder_extended_gates::producers::flake_audit::run_with_options;

#[cfg(unix)]
fn write_fake_cargo(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("cargo");
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
}

#[cfg(unix)]
#[test]
fn ac_flake_audit_passes_1_receipt_has_passes_field() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);

    let fakebin = tmp.path().join("fakebin");
    write_fake_cargo(&fakebin);

    let spec = ProducerSpec::lookup("flake-audit").expect("flake-audit registered");

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{orig_path}", fakebin.display());
    // SAFETY: this is the only test in this binary; no other thread reads
    // or writes PATH concurrently. Set, run, then restore.
    unsafe {
        std::env::set_var("PATH", &new_path);
    }
    let result = run_with_options(spec, project, Some(1), None);
    unsafe {
        std::env::set_var("PATH", &orig_path);
    }
    result.unwrap();

    let v = read_receipt(project, "flake-audit-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(v.get("passes").and_then(serde_json::Value::as_u64), Some(1));
    assert_eq!(
        v.get("exit_codes").and_then(serde_json::Value::as_array).map(Vec::len),
        Some(1)
    );
    assert_eq!(v.get("deterministic").and_then(serde_json::Value::as_bool), Some(true));
}

#[cfg(unix)]
#[test]
fn ac_flake_audit_only_changed_since_scopes_to_tests() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    std::fs::create_dir_all(project.join("tests")).unwrap();
    std::fs::write(project.join("tests/alpha.rs"), b"#[test] fn t() {}\n").unwrap();
    commit_all(project, "base");
    let base = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let base_sha = String::from_utf8_lossy(&base.stdout).trim().to_owned();

    std::fs::write(project.join("tests/alpha.rs"), b"#[test] fn t2() {}\n").unwrap();
    commit_all(project, "touch alpha");

    let fakebin = tmp.path().join("fakebin");
    write_fake_cargo(&fakebin);
    let spec = ProducerSpec::lookup("flake-audit").expect("flake-audit registered");

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{orig_path}", fakebin.display());
    // SAFETY: this is the only test in this binary; no other thread reads
    // or writes PATH concurrently. Set, run, then restore.
    unsafe {
        std::env::set_var("PATH", &new_path);
    }
    let result = run_with_options(spec, project, Some(1), Some(&base_sha));
    unsafe {
        std::env::set_var("PATH", &orig_path);
    }
    result.unwrap();

    let v = read_receipt(project, "flake-audit-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(v.get("changed_scope").and_then(serde_json::Value::as_str), Some("tests"));
    assert_eq!(
        v.get("changed_tests").and_then(serde_json::Value::as_array).map(|a| a.len()),
        Some(1)
    );
}

#[cfg(unix)]
#[test]
fn ac_flake_audit_unknown_base_is_infra_class_block() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);

    let spec = ProducerSpec::lookup("flake-audit").expect("flake-audit registered");
    let result = run_with_options(spec, project, Some(1), Some("not-a-real-rev"));
    result.unwrap();

    let v = read_receipt(project, "flake-audit-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    assert_eq!(v.get("changed_scope").and_then(serde_json::Value::as_str), Some("error"));
    assert!(v.get("git_diff_error").and_then(serde_json::Value::as_str).is_some());
}
