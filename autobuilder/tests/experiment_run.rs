//! Integration test for `autobuilder experiment run --no-edit-agent`.
//!
//! Builds a self-contained fixture in a temp directory:
//!   <tmp>/.git/ — initialized git repo
//!   <tmp>/proj-a/agent/intent-card.json
//!   <tmp>/proj-a/scripts/run-metrics.sh — emits a fixed metrics.json
//!   <tmp>/proj-b/{same shape}
//!   <tmp>/experiment.toml
//!
//! Then invokes the built `autobuilder experiment run` binary and asserts
//! the campaign walks both slices, writes per-iteration receipts, and
//! applies the transition policies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::similar_names)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write_project(root: &Path, slug: &str) {
    let proj = root.join(slug);
    fs::create_dir_all(proj.join("agent")).unwrap();
    fs::create_dir_all(proj.join("scripts")).unwrap();
    let intent_card = format!(
        r#"{{
  "schema": "autobuilder.intent_card.v1",
  "prd_source": "PRD.md",
  "intent_slug": "{slug}",
  "root_motivation": "Fixture project used only by experiment_run integration tests; emits a fixed metric so the iterate-and-prove loop has a stable input.",
  "user_persona": "The autobuilder experiment-run integration test harness.",
  "unfakeable_metric": {{
    "name": "ac_passing_count",
    "lower_is_better": false,
    "harness_command": "scripts/run-metrics.sh",
    "target": 1
  }},
  "acceptance_criteria": [
    {{"id": "AC1", "level": "MUST", "description": "No-op fixture AC.", "test": "tests/acceptance_ac1.rs"}}
  ],
  "scope": ["fixture-only"],
  "non_goals": ["being-built"],
  "hard_constraints": {{"rust_edition": "2024", "target_kind": "cli", "deny_unsafe": true}},
  "five_whys_trace": [
    {{"why": 1, "q": "why fixture", "a": "to test the campaign driver"}}
  ],
  "created_at": "2026-05-23T12:00:00Z"
}}"#
    );
    fs::write(proj.join("agent/intent-card.json"), intent_card).unwrap();

    let harness = r#"#!/bin/bash
set -e
mkdir -p target/autobuilder
cat > target/autobuilder/metrics.json <<'JSON'
{
  "schema": "autobuilder.metrics.v1",
  "head_sha": "fixture",
  "scalars": {"ac_passing_count": 1},
  "ac_passing_count": 1,
  "ac_total_count": 1,
  "audit": {"blocking_count": 0, "advisory_count": 0},
  "clippy_warning_count": 0,
  "captured_at": "2026-05-23T12:00:00Z"
}
JSON
exit 0
"#;
    let script_path = proj.join("scripts/run-metrics.sh");
    fs::write(&script_path, harness).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
}

fn write_manifest(root: &Path, transition: &str) {
    let manifest = format!(
        r#"schema = "autobuilder.experiment_manifest.v1"

[campaign]
slug = "smoke-fixture"
prd_source = "PRD.md"
max_wall_clock_minutes = 5
max_total_iterations = 8

[edit_agent]
model = "claude-opus-4-7"
max_tokens_per_call = 16000

[[slices]]
id = "S1"
intent_card = "proj-a/agent/intent-card.json"
transition = "{transition}"

[[slices]]
id = "S2"
intent_card = "proj-b/agent/intent-card.json"
transition = "{transition}"
"#
    );
    fs::write(root.join("experiment.toml"), manifest).unwrap();
}

fn init_repo(root: &Path) {
    run_git(root, &["init", "-q"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "Test"]);
    run_git(root, &["add", "-A"]);
    run_git(root, &["commit", "-q", "-m", "baseline"]);
}

/// AC1 (MUST) — a 2-slice campaign runs to completion with --no-edit-agent,
/// writes one iteration receipt per slice, and reports verdict=baseline.
#[test]
fn ac1_happy_path_two_slices() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_project(root, "proj-a");
    write_project(root, "proj-b");
    write_manifest(root, "advance-commit");
    init_repo(root);

    let manifest = root.join("experiment.toml");
    let out = autobuilder()
        .args([
            "experiment",
            "run",
            "--manifest",
            manifest.to_str().unwrap(),
            "--no-edit-agent",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "campaign failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("campaign `smoke-fixture` completed"), "stdout: {stdout}");
    assert!(stdout.contains("slice S1 baseline="), "stdout: {stdout}");
    assert!(stdout.contains("slice S2 baseline="), "stdout: {stdout}");
    assert!(stdout.contains("verdict=baseline"), "stdout: {stdout}");

    let proj_a_receipts = root.join("proj-a/target/autobuilder/receipts");
    let proj_b_receipts = root.join("proj-b/target/autobuilder/receipts");
    assert!(proj_a_receipts.is_dir(), "proj-a receipts missing");
    assert!(proj_b_receipts.is_dir(), "proj-b receipts missing");

    let a_entries: Vec<_> = fs::read_dir(&proj_a_receipts)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect();
    assert!(
        a_entries.iter().any(|n| {
            let s = n.to_string_lossy();
            s.ends_with(".json") && s != "session-trace.json"
        }),
        "proj-a: expected an iteration receipt (<sha>.json), got {a_entries:?}"
    );
}

/// AC2 (MUST) — omitting --no-edit-agent fails fast with a typed error.
#[test]
fn ac2_missing_no_edit_agent_flag_fails() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_project(root, "proj-a");
    write_manifest(root, "advance-commit");
    init_repo(root);

    let manifest = root.join("experiment.toml");
    let out = autobuilder()
        .args([
            "experiment",
            "run",
            "--manifest",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected failure without --no-edit-agent");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--no-edit-agent is required"),
        "expected typed error, got: {stderr}"
    );
}

/// AC3 (MUST) — `transition = "reset"` rewinds HEAD to the slice's
/// baseline after the slice runs, observable via `git log --oneline`.
#[test]
fn ac3_reset_transition_rewinds_head() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_project(root, "proj-a");
    write_project(root, "proj-b");
    write_manifest(root, "reset");
    init_repo(root);

    let baseline_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    let baseline = String::from_utf8_lossy(&baseline_out.stdout).trim().to_owned();

    let manifest = root.join("experiment.toml");
    let out = autobuilder()
        .args([
            "experiment",
            "run",
            "--manifest",
            manifest.to_str().unwrap(),
            "--no-edit-agent",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "campaign with reset failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    let after = String::from_utf8_lossy(&after_out.stdout).trim().to_owned();
    assert_eq!(after, baseline, "reset transition must leave HEAD at baseline");
}
