//! AC7 (SHOULD) — `--iteration N` flag passes through to metrics.iteration.
//!
//! Spec (from agent/intent-card.json AC7):
//!   Support --iteration N; pass through to metrics.iteration. Default null
//!   when absent.
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

#[test]
fn acceptance_ac7_iteration_flag_passes_through() -> TestResult {
    let project = make_project()?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .arg("--iteration")
        .arg("42")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    assert_eq!(
        doc.get("iteration").and_then(serde_json::Value::as_i64),
        Some(42),
        "AC7: --iteration 42 must yield metrics.iteration == 42; got: {stdout}",
    );
    Ok(())
}

#[test]
fn acceptance_ac7_iteration_defaults_to_null_when_absent() -> TestResult {
    let project = make_project()?;
    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    let iter = doc.get("iteration").ok_or("iteration missing")?;
    assert!(
        iter.is_null(),
        "AC7: iteration must default to null when --iteration absent; got: {iter}",
    );
    Ok(())
}
