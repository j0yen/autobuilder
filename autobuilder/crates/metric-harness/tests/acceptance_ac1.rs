//! AC1 (MUST) — Missing/non-executable run-metrics.sh.
//!
//! Spec (from agent/intent-card.json AC1):
//!   Given a project path, invoke <path>/scripts/run-metrics.sh. Return
//!   non-zero (exit 2) if missing or not executable, with stderr
//!   identifying the missing script.
//!
//! READ-ONLY after scaffold. The edit-agent must NOT modify this file.
//! To change behavior, write agent/intent_card_amendment_request.json.

use std::process::Command;
use tempfile::TempDir;

const HARNESS_BIN: &str = env!("CARGO_BIN_EXE_autobuilder-metric-harness");

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn acceptance_ac1_exit_2_when_script_is_missing() -> TestResult {
    let project = TempDir::new()?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let exit_code = output.status.code().ok_or("no exit code")?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        exit_code, 2,
        "AC1: expected exit 2 when scripts/run-metrics.sh missing; got {exit_code}. stderr: {stderr}",
    );
    assert!(
        stderr.contains("scripts/run-metrics.sh"),
        "AC1: stderr should identify scripts/run-metrics.sh; got: {stderr}",
    );
    Ok(())
}

#[test]
fn acceptance_ac1_exit_2_when_script_not_executable() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;
    let script = scripts_dir.join("run-metrics.sh");
    std::fs::write(&script, "#!/usr/bin/env bash\nexit 0\n")?;
    // Intentionally clear the executable bit.
    let mut perms = std::fs::metadata(&script)?.permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&script, perms)?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let exit_code = output.status.code().ok_or("no exit code")?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        exit_code, 2,
        "AC1: expected exit 2 when scripts/run-metrics.sh not executable; got {exit_code}. stderr: {stderr}",
    );
    assert!(
        stderr.contains("scripts/run-metrics.sh"),
        "AC1: stderr should identify scripts/run-metrics.sh; got: {stderr}",
    );
    Ok(())
}
