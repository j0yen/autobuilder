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
fn ac4_every_finding_has_required_fields() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("shape_test.sh");
    fs::write(
        &script,
        "#!/bin/bash\nJOBS=$(nproc)\nSIZE=$(stat -c %s file.txt)\nSCRIPT=$(readlink -f \"$0\")\n",
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
    assert!(!findings.is_empty(), "Expected findings but got none");

    for (i, f) in findings.iter().enumerate() {
        // file: non-empty string
        let file = f["file"].as_str().unwrap_or_else(|| {
            panic!("finding[{i}]: missing or non-string 'file' field")
        });
        assert!(!file.is_empty(), "finding[{i}]: 'file' field is empty");

        // line: integer >= 1
        let line = f["line"].as_u64().unwrap_or_else(|| {
            panic!("finding[{i}]: missing or non-integer 'line' field")
        });
        assert!(line >= 1, "finding[{i}]: 'line' must be 1-based (got {line})");

        // text: string (may be empty if line is blank, but should be present)
        assert!(
            f["text"].is_string(),
            "finding[{i}]: missing or non-string 'text' field"
        );

        // suggestion: non-empty string
        let suggestion = f["suggestion"].as_str().unwrap_or_else(|| {
            panic!("finding[{i}]: missing or non-string 'suggestion' field")
        });
        assert!(
            !suggestion.is_empty(),
            "finding[{i}]: 'suggestion' field is empty"
        );

        // already_guarded: boolean
        assert!(
            f["already_guarded"].is_boolean(),
            "finding[{i}]: missing or non-boolean 'already_guarded' field"
        );

        // rule: non-empty string
        let rule = f["rule"].as_str().unwrap_or_else(|| {
            panic!("finding[{i}]: missing or non-string 'rule' field")
        });
        assert!(!rule.is_empty(), "finding[{i}]: 'rule' field is empty");
    }
}
