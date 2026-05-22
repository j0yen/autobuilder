//! AC5 (MUST) — Exit 0 only on full success; metrics emitted on failure too.
//!
//! Spec (from agent/intent-card.json AC5):
//!   Exit 0 only if project script exited 0 AND audit.blocking_count == 0
//!   AND ac_results.length == ac_total_count. Otherwise exit 1 — but emit
//!   the metrics document either way so the loop can record the failed
//!   iteration.
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

fn metrics_json(ac_total: usize, ac_results_len: usize, blocking: u32) -> String {
    let mut ac_results = String::from("[");
    for i in 0..ac_results_len {
        if i > 0 {
            ac_results.push(',');
        }
        ac_results.push_str(&format!(
            "{{\"id\":\"AC{}\",\"level\":\"MUST\",\"passing\":true}}",
            i + 1
        ));
    }
    ac_results.push(']');
    format!(
        "{{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{{}},\"ac_passing_count\":{ac_results_len},\"ac_total_count\":{ac_total},\"ac_results\":{ac_results},\"audit\":{{\"blocking_count\":{blocking},\"advisory_count\":0}},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}}"
    )
}

fn write_project_with_metrics(metrics_body: &str, script_exit: i32) -> std::io::Result<TempDir> {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;
    let body = format!(
        "#!/usr/bin/env bash\n\
         mkdir -p target/autobuilder\n\
         cat > target/autobuilder/metrics.json <<'JSON'\n{metrics_body}\nJSON\n\
         exit {script_exit}\n"
    );
    write_exec_script(&scripts_dir.join("run-metrics.sh"), &body)?;
    Ok(project)
}

#[test]
fn acceptance_ac5_exit_0_when_clean() -> TestResult {
    let project = write_project_with_metrics(&metrics_json(3, 3, 0), 0)?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(exit_code, 0, "AC5: clean run must exit 0; got {exit_code}");
    assert!(
        project.path().join("target/autobuilder/metrics.json").exists(),
        "AC5: metrics.json must exist on clean run",
    );
    Ok(())
}

#[test]
fn acceptance_ac5_exit_1_when_blocking_audit() -> TestResult {
    let project = write_project_with_metrics(&metrics_json(3, 3, 2), 0)?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(
        exit_code, 1,
        "AC5: blocking audit must exit 1; got {exit_code}",
    );
    assert!(
        project.path().join("target/autobuilder/metrics.json").exists(),
        "AC5: metrics.json must exist even with blocking audit",
    );
    Ok(())
}

#[test]
fn acceptance_ac5_exit_1_when_ac_results_length_mismatch() -> TestResult {
    // ac_results.length=2 but ac_total_count=10 → mismatch.
    let project = write_project_with_metrics(&metrics_json(10, 2, 0), 0)?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let exit_code = output.status.code().ok_or("no exit code")?;
    assert_eq!(
        exit_code, 1,
        "AC5: ac_results length != ac_total_count must exit 1; got {exit_code}",
    );
    assert!(
        project.path().join("target/autobuilder/metrics.json").exists(),
        "AC5: metrics.json must exist on AC mismatch",
    );
    Ok(())
}
