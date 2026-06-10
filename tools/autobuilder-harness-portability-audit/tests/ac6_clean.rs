use std::process::Command;
use tempfile::TempDir;
use std::fs;

fn bin_path() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("autobuilder-harness-portability-audit")
}

#[test]
fn ac6_clean_dir_yields_no_findings() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("clean.sh");
    fs::write(
        &script,
        "#!/bin/bash\n# No Linux-only idioms here\necho 'hello world'\ndate +%Y-%m-%d\n",
    )
    .unwrap();

    let output = Command::new(bin_path())
        .arg(dir.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit 0 for clean dir"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));

    let findings = report["findings"].as_array().unwrap();
    assert!(
        findings.is_empty(),
        "Expected empty findings for clean dir, got: {findings:?}"
    );
}

#[test]
fn ac6_empty_dir_yields_no_findings() {
    let dir = TempDir::new().unwrap();

    let output = Command::new(bin_path())
        .arg(dir.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit 0 for empty dir"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));

    let findings = report["findings"].as_array().unwrap();
    assert!(
        findings.is_empty(),
        "Expected empty findings for empty dir"
    );
}
