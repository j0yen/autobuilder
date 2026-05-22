//! AC10 (MAY) — `--pretty` for human-readable stdout.
//!
//! Spec (from agent/intent-card.json AC10):
//!   Support --pretty for human-readable stdout; default is single-line
//!   canonical JSON. Verifiable by counting newlines in output.
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

fn make_project() -> std::io::Result<TempDir> {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;
    let body = "#!/usr/bin/env bash\n\
                mkdir -p target/autobuilder\n\
                cat > target/autobuilder/metrics.json <<'JSON'\n\
{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{},\"ac_passing_count\":0,\"ac_total_count\":0,\"ac_results\":[],\"audit\":{\"blocking_count\":0,\"advisory_count\":0},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}\n\
JSON\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), body)?;
    Ok(project)
}

fn count_payload_newlines(stdout: &str) -> usize {
    // Strip a single trailing newline (cargo/println convention) before counting.
    let trimmed = stdout.strip_suffix('\n').unwrap_or(stdout);
    trimmed.matches('\n').count()
}

#[test]
fn acceptance_ac10_default_is_single_line() -> TestResult {
    let project = make_project()?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Must be valid JSON so empty stdout cannot trivially satisfy "single line".
    let _doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    let newlines = count_payload_newlines(&stdout);
    assert_eq!(
        newlines, 0,
        "AC10: default output must be single-line canonical JSON; saw {newlines} interior newlines. stdout: {stdout}",
    );
    Ok(())
}

#[test]
fn acceptance_ac10_pretty_introduces_newlines() -> TestResult {
    let project = make_project()?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .arg("--pretty")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let newlines = count_payload_newlines(&stdout);
    assert!(
        newlines > 0,
        "AC10: --pretty must produce multi-line output; saw {newlines} interior newlines. stdout: {stdout}",
    );
    Ok(())
}
