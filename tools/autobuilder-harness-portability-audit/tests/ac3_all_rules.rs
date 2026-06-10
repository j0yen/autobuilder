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
fn ac3_all_rules_trigger() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("all_rules.sh");
    fs::write(
        &script,
        r#"#!/bin/bash
CPU=$(cat /proc/cpuinfo | wc -l)
flock 200
TOMORROW=$(date -d "tomorrow" +%Y-%m-%d)
SCRIPT=$(readlink -f "$0")
sed -i 's/foo/bar/g' file.txt
SIZE=$(stat -c %s file.txt)
"#,
    )
    .unwrap();

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
    let rules_found: std::collections::HashSet<&str> = findings
        .iter()
        .filter_map(|f| f["rule"].as_str())
        .collect();

    let expected_rules = [
        "proc-fs",
        "flock",
        "gnu-date",
        "readlink-f",
        "sed-i-empty",
        "stat-c",
    ];

    for rule in &expected_rules {
        assert!(
            rules_found.contains(rule),
            "Expected rule '{rule}' to be triggered. Found rules: {rules_found:?}\nstdout: {stdout}"
        );
    }
}
