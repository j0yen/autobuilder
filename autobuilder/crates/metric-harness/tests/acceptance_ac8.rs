//! AC8 (SHOULD) — `--timeout-seconds N` kills runaway scripts.
//!
//! Spec (from agent/intent-card.json AC8):
//!   Time-bound the project script with --timeout-seconds N (default 600).
//!   On timeout: kill the process, emit metrics with failure_kind="timeout",
//!   exit 1.
//!
//! READ-ONLY after scaffold.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Instant;
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
fn acceptance_ac8_timeout_kills_script_and_marks_failure_kind() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    // Script sleeps 30s; we will give --timeout-seconds 1.
    write_exec_script(
        &scripts_dir.join("run-metrics.sh"),
        "#!/usr/bin/env bash\nsleep 30\n",
    )?;

    let started = Instant::now();
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .arg("--timeout-seconds")
        .arg("1")
        .output()?;
    let elapsed = started.elapsed();

    assert!(
        elapsed.as_secs() < 10,
        "AC8: harness should kill the script in ~1s; took {}s",
        elapsed.as_secs(),
    );

    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(
        exit_code, 1,
        "AC8: timeout must yield exit 1; got {exit_code}",
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    let failure_kind = doc
        .get("failure_kind")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing failure_kind")?;
    assert_eq!(
        failure_kind, "timeout",
        "AC8: failure_kind must be \"timeout\"; got: {failure_kind}",
    );
    Ok(())
}
