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
fn ac5_strict_exits_4_when_unguarded() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("unguarded.sh");
    fs::write(&script, "#!/bin/bash\nJOBS=$(nproc)\ncargo build\n").unwrap();

    let status = Command::new(bin_path())
        .arg(dir.path())
        .arg("--strict")
        .status()
        .expect("failed to run binary");

    assert_eq!(
        status.code(),
        Some(4),
        "Expected exit code 4 with --strict and unguarded findings"
    );
}

#[test]
fn ac5_strict_exits_0_when_all_guarded() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("guarded.sh");
    fs::write(
        &script,
        "#!/bin/bash\nJOBS=$(nproc 2>/dev/null || sysctl -n hw.logicalcpu || echo 4)\ncargo build\n",
    )
    .unwrap();

    let status = Command::new(bin_path())
        .arg(dir.path())
        .arg("--strict")
        .status()
        .expect("failed to run binary");

    assert_eq!(
        status.code(),
        Some(0),
        "Expected exit code 0 with --strict when all findings are guarded"
    );
}

#[test]
fn ac5_strict_exits_0_when_no_findings() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("clean.sh");
    fs::write(&script, "#!/bin/bash\necho hello\n").unwrap();

    let status = Command::new(bin_path())
        .arg(dir.path())
        .arg("--strict")
        .status()
        .expect("failed to run binary");

    assert_eq!(
        status.code(),
        Some(0),
        "Expected exit code 0 with --strict when no findings"
    );
}
