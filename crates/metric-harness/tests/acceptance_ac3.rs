//! AC3 (MUST) — Schema validation of metrics.json; exit 3 on failure.
//!
//! Spec (from agent/intent-card.json AC3):
//!   Parse <path>/target/autobuilder/metrics.json, validate against the
//!   autobuilder.metrics.v1 schema embedded as a build-time const. Return
//!   exit 3 with a JSON diagnostic on validation failure.
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
fn acceptance_ac3_exit_3_on_malformed_metrics_json() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let script_body = "#!/usr/bin/env bash\n\
                       mkdir -p target/autobuilder\n\
                       echo '{ this is not even valid json' > target/autobuilder/metrics.json\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), script_body)?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let exit_code = output.status.code().ok_or("no exit code")?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        exit_code, 3,
        "AC3: expected exit 3 on malformed JSON; got {exit_code}. stdout: {stdout}",
    );

    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())?;
    assert!(
        parsed.is_object(),
        "AC3: stdout should be a JSON diagnostic object; got: {stdout}",
    );
    Ok(())
}

#[test]
fn acceptance_ac3_exit_3_on_schema_violation() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let script_body = "#!/usr/bin/env bash\n\
                       mkdir -p target/autobuilder\n\
                       echo '{\"schema\":\"wrong.schema.v9\"}' > target/autobuilder/metrics.json\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), script_body)?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(
        exit_code, 3,
        "AC3: expected exit 3 on schema violation; got {exit_code}",
    );
    Ok(())
}
