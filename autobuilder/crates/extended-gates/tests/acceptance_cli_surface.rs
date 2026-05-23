//! AC-cli-surface.{1,2}: snapshot match passes; drift blocks.
//!
//! Creates a fake "binary" that's just a shell script writing the snapshot
//! content (the producer treats `--help` output as opaque bytes; whether the
//! producer is rust- or shell-based isn't part of the contract). The test
//! plants drift by mismatching the snapshot vs the script's output.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

#[cfg(unix)]
fn write_shell_bin(project: &std::path::Path, name: &str, help_text: &str) {
    use std::os::unix::fs::PermissionsExt;
    let dir = project.join("target/release");
    std::fs::create_dir_all(&dir).unwrap();
    let script = format!("#!/bin/sh\nprintf '%s' \"{}\"\n", help_text.replace('"', "\\\""));
    let path = dir.join(name);
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
}

fn write_snapshot(project: &std::path::Path, name: &str, content: &str) {
    let dir = project.join("cli-surface-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.txt")), content).unwrap();
}

#[cfg(unix)]
#[test]
fn ac_cli_surface_1_snapshot_match_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    let help = "Usage: foo [OPTIONS]\n\nOptions:\n  --config <PATH>\n";
    write_shell_bin(project, "foo", help);
    write_snapshot(project, "foo", help);
    run_producer("cli-surface", project).unwrap();
    let v = read_receipt(project, "cli-surface-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
}

#[cfg(unix)]
#[test]
fn ac_cli_surface_2_planted_drift_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    let old_help = "Usage: foo [OPTIONS]\n\nOptions:\n  --config <PATH>\n";
    let new_help = "Usage: foo [OPTIONS]\n\nOptions:\n  --cfg <PATH>\n"; // renamed
    write_shell_bin(project, "foo", new_help);
    write_snapshot(project, "foo", old_help);
    run_producer("cli-surface", project).unwrap();
    let v = read_receipt(project, "cli-surface-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let drifted = v
        .get("drifted")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(drifted.iter().any(|s| s.as_str() == Some("foo")));
}
