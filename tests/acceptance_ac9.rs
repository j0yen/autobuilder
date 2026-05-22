//! AC9 (SHOULD) — Synthetic metrics on script crash / metric emission failure.
//!
//! Spec (from agent/intent-card.json AC9):
//!   When project script exits non-zero with no metrics.json produced, emit
//!   a synthetic metrics doc with failure_kind in {build_error,
//!   metric_emission_failure} so the loop can write a FailureCapsule.
//!
//! READ-ONLY after scaffold.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

const HARNESS_BIN: &str = env!("CARGO_BIN_EXE_autobuilder-metric-harness");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn write_exec_script(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    std::fs::write(path, body)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[test]
fn acceptance_ac9_synthetic_metrics_when_script_crashes() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    // Script exits 99 without producing any metrics.json.
    write_exec_script(
        &scripts_dir.join("run-metrics.sh"),
        "#!/usr/bin/env bash\necho 'simulated build error' 1>&2\nexit 99\n",
    )?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(
        exit_code, 1,
        "AC9: crashing script must yield exit 1; got {exit_code}",
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    let failure_kind = doc
        .get("failure_kind")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing failure_kind")?;
    assert!(
        failure_kind == "build_error" || failure_kind == "metric_emission_failure",
        "AC9: failure_kind must be build_error or metric_emission_failure; got: {failure_kind}",
    );
    Ok(())
}
