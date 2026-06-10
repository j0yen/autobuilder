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
fn ac1_bare_nproc_is_unguarded() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("test.sh");
    fs::write(&script, "#!/bin/bash\nJOBS=$(nproc)\ncargo build --jobs \"$JOBS\"\n").unwrap();

    let output = Command::new(bin_path())
        .arg(dir.path())
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));

    let findings = report["findings"].as_array().unwrap();
    let nproc_findings: Vec<_> = findings
        .iter()
        .filter(|f| f["rule"].as_str() == Some("nproc"))
        .collect();

    assert!(
        !nproc_findings.is_empty(),
        "Expected at least one nproc finding, got none. stdout: {stdout}"
    );

    let f = &nproc_findings[0];
    assert_eq!(
        f["already_guarded"].as_bool(),
        Some(false),
        "Expected already_guarded: false for bare nproc"
    );

    let unguarded = report["summary"]["unguarded"].as_u64().unwrap();
    assert!(
        unguarded >= 1,
        "Expected unguarded >= 1, got {unguarded}"
    );
}
