//! AC8 (P1, PRD-autobuilder-rollback-tag-aware) — `rollback-plan --explain`
//! prints the resolved model, base tag, and rollback target on one line.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::indexing_slicing, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com", "commit", "-q", "-m", msg],
    );
}

fn write_cargo_toml(dir: &Path, name: &str, version: &str) {
    fs::write(dir.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n")).unwrap();
}

#[test]
fn ac8_explain_prints_model_base_tag_and_target_on_one_line() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write_cargo_toml(&project, "svc", "0.1.0");
    fs::write(agent.join("intent-card.json"), r#"{"rollback_model": "redeploy-tag"}"#).unwrap();
    commit_all(&project, "initial: v0.1.0");
    git(&project, &["tag", "v0.1.0"]);

    write_cargo_toml(&project, "svc", "0.2.0");
    commit_all(&project, "bump to 0.2.0");
    git(&project, &["tag", "v0.2.0"]);

    let out = autobuilder()
        .args([
            "rollback-plan",
            "--project",
            project.to_str().unwrap(),
            "--base",
            "v0.1.0",
            "--explain",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);

    let explain_line = stdout
        .lines()
        .find(|l| l.contains("--explain"))
        .unwrap_or_else(|| panic!("no --explain line in stdout: {stdout}"));
    assert!(explain_line.contains("model=redeploy-tag"), "{explain_line}");
    assert!(explain_line.contains("v0.1.0"), "{explain_line}");
    assert!(explain_line.contains("target="), "{explain_line}");
}
