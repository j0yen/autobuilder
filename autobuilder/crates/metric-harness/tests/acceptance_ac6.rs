//! AC6 (MUST) — Only target/autobuilder/* may change.
//!
//! Spec (from agent/intent-card.json AC6):
//!   Operate without modifying anything outside <path>/target/autobuilder/.
//!   Snapshot-hash the project before and after; only target/autobuilder/*
//!   paths may have changed.
//!
//! READ-ONLY after scaffold.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use sha2::{Digest, Sha256};

const HARNESS_BIN: &str = env!("CARGO_BIN_EXE_autobuilder-metric-harness");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn write_exec_script(path: &Path, body: &str) -> std::io::Result<()> {
    std::fs::write(path, body)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

fn snapshot(dir: &Path) -> std::io::Result<Vec<(PathBuf, String)>> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            // Skip target/autobuilder — that's the only directory allowed to change.
            if rel.starts_with("target/autobuilder") {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out)?;
            } else {
                let bytes = std::fs::read(&path)?;
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                let hash = format!("{:x}", hasher.finalize());
                out.push((rel, hash));
            }
        }
        Ok(())
    }
    walk(dir, dir, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

#[test]
fn acceptance_ac6_no_files_outside_target_autobuilder_change() -> TestResult {
    let project = TempDir::new()?;
    let project_path = project.path();
    let scripts_dir = project_path.join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    // Add a few "user" files to the project that must remain untouched.
    std::fs::write(project_path.join("README.md"), "# fixture project\n")?;
    std::fs::create_dir_all(project_path.join("src"))?;
    std::fs::write(project_path.join("src/main.rs"), "fn main() {}\n")?;

    let body = "#!/usr/bin/env bash\n\
                mkdir -p target/autobuilder\n\
                cat > target/autobuilder/metrics.json <<'JSON'\n\
{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{},\"ac_passing_count\":0,\"ac_total_count\":0,\"ac_results\":[],\"audit\":{\"blocking_count\":0,\"advisory_count\":0},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}\n\
JSON\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), body)?;

    let before = snapshot(project_path)?;

    let _ = Command::new(HARNESS_BIN)
        .arg(project_path)
        .arg("--head-sha")
        .arg("deadbeef")
        .output()?;

    let after = snapshot(project_path)?;

    assert_eq!(
        before, after,
        "AC6: files outside target/autobuilder/ must not change",
    );

    // Sanity: AC6 is only meaningful if the harness actually ran. If
    // target/autobuilder/metrics.json wasn't produced, the harness no-oped
    // and the equality above is trivially satisfied.
    assert!(
        project_path.join("target/autobuilder/metrics.json").exists(),
        "AC6: harness did not produce target/autobuilder/metrics.json; cannot verify isolation if nothing ran",
    );
    Ok(())
}
