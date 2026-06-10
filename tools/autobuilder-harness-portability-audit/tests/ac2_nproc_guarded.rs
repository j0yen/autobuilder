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
fn ac2_nproc_with_sysctl_fallback_is_guarded() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("guarded.sh");
    fs::write(
        &script,
        "#!/bin/bash\nJOBS=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu || echo 4)\ncargo build --jobs \"$JOBS\"\n",
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
    let nproc_findings: Vec<_> = findings
        .iter()
        .filter(|f| f["rule"].as_str() == Some("nproc"))
        .collect();

    assert!(
        !nproc_findings.is_empty(),
        "Expected nproc finding to be reported (even if guarded)"
    );

    for f in &nproc_findings {
        assert_eq!(
            f["already_guarded"].as_bool(),
            Some(true),
            "Expected already_guarded: true for nproc with sysctl fallback"
        );
    }

    let unguarded = report["summary"]["unguarded"].as_u64().unwrap();
    assert_eq!(
        unguarded, 0,
        "Expected unguarded == 0 when all nproc lines are guarded, got {unguarded}"
    );
}
