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

fn run_and_get_findings(dir: &std::path::Path) -> Vec<serde_json::Value> {
    let output = Command::new(bin_path())
        .arg(dir)
        .arg("--format")
        .arg("json")
        .output()
        .expect("failed to run binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("invalid JSON: {e}\nstdout: {stdout}"));

    report["findings"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn ac7_findings_sorted_by_file_then_line() {
    let dir = TempDir::new().unwrap();

    // alpha.sh: nproc on line 2, stat -c on line 3
    let alpha = dir.path().join("alpha.sh");
    fs::write(&alpha, "#!/bin/bash\nJOBS=$(nproc)\nSIZE=$(stat -c %s file.txt)\n").unwrap();

    // beta.sh: readlink -f on line 2, date -d on line 3
    let beta = dir.path().join("beta.sh");
    fs::write(&beta, "#!/bin/bash\nSCRIPT=$(readlink -f \"$0\")\nTOMORROW=$(date -d \"tomorrow\" +%Y-%m-%d)\n").unwrap();

    let findings = run_and_get_findings(dir.path());

    assert!(findings.len() >= 4, "Expected at least 4 findings");

    // Verify sorted by (file, line)
    for i in 1..findings.len() {
        let prev = &findings[i - 1];
        let curr = &findings[i];

        let prev_file = prev["file"].as_str().unwrap();
        let curr_file = curr["file"].as_str().unwrap();
        let prev_line = prev["line"].as_u64().unwrap();
        let curr_line = curr["line"].as_u64().unwrap();

        let in_order = prev_file < curr_file
            || (prev_file == curr_file && prev_line <= curr_line);

        assert!(
            in_order,
            "Findings not sorted: [{prev_file}, line {prev_line}] comes before [{curr_file}, line {curr_line}]"
        );
    }
}

#[test]
fn ac7_output_is_deterministic_across_runs() {
    let dir = TempDir::new().unwrap();

    let script = dir.path().join("multi.sh");
    fs::write(
        &script,
        "#!/bin/bash\nJOBS=$(nproc)\nSCRIPT=$(readlink -f \"$0\")\nSIZE=$(stat -c %s file.txt)\n",
    )
    .unwrap();

    let findings1 = run_and_get_findings(dir.path());
    let findings2 = run_and_get_findings(dir.path());

    assert_eq!(
        findings1.len(),
        findings2.len(),
        "Findings count differs between runs"
    );

    for i in 0..findings1.len() {
        assert_eq!(
            findings1[i]["file"], findings2[i]["file"],
            "finding[{i}] file differs between runs"
        );
        assert_eq!(
            findings1[i]["line"], findings2[i]["line"],
            "finding[{i}] line differs between runs"
        );
        assert_eq!(
            findings1[i]["rule"], findings2[i]["rule"],
            "finding[{i}] rule differs between runs"
        );
    }
}
