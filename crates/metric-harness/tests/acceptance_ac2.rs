//! AC2 (MUST) — stdout+stderr captured to target/autobuilder/run.log.
//!
//! Spec (from agent/intent-card.json AC2):
//!   Capture project script's stdout AND stderr into
//!   <path>/target/autobuilder/run.log.
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
fn acceptance_ac2_run_log_contains_both_streams() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let script_body = "#!/usr/bin/env bash\n\
                       set -e\n\
                       mkdir -p target/autobuilder\n\
                       echo 'STDOUT_MARKER_LINE'\n\
                       echo 'STDERR_MARKER_LINE' 1>&2\n\
                       cat > target/autobuilder/metrics.json <<'JSON'\n\
{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{},\"ac_passing_count\":0,\"ac_total_count\":0,\"ac_results\":[],\"audit\":{\"blocking_count\":0,\"advisory_count\":0},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}\n\
JSON\n";

    write_exec_script(&scripts_dir.join("run-metrics.sh"), script_body)?;

    let _ = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let run_log_path = project.path().join("target/autobuilder/run.log");
    assert!(
        run_log_path.exists(),
        "AC2: expected run.log at {}",
        run_log_path.display(),
    );

    let run_log = std::fs::read_to_string(&run_log_path)?;
    assert!(
        run_log.contains("STDOUT_MARKER_LINE"),
        "AC2: run.log missing stdout marker. Contents: {run_log}",
    );
    assert!(
        run_log.contains("STDERR_MARKER_LINE"),
        "AC2: run.log missing stderr marker. Contents: {run_log}",
    );
    Ok(())
}
