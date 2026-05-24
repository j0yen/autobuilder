//! AC-experiment.{1,2,3,4}: campaign roll-up producer.
//!
//! AC1 happy-path: outcomes file with all baseline/advance slices → verdict=pass.
//! AC2 planted failure: outcomes file with a crash slice → verdict=block.
//! AC3 skipped: no outcomes file → verdict=skipped.
//! AC4 schema: receipt declares `autobuilder.experiment_receipt.v1`
//!     and round-trips through the digest-binding writer.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation
)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;
use serde_json::json;
use std::path::Path;

fn write_outcomes(project: &Path, slices: &[serde_json::Value]) {
    std::fs::create_dir_all(project.join("target/autobuilder")).unwrap();
    let doc = json!({
        "schema": "autobuilder.experiment_outcomes.v1",
        "campaign_slug": "test-campaign",
        "total_iterations": slices.len() as u32,
        "wall_clock_seconds": 12,
        "slices": slices,
    });
    std::fs::write(
        project.join("target/autobuilder/experiment-outcomes.json"),
        serde_json::to_string_pretty(&doc).unwrap(),
    )
    .unwrap();
}

#[test]
fn ac_experiment_1_happy_path_all_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_outcomes(
        project,
        &[
            json!({"id": "S1", "baseline_sha": "abc", "verdict": "baseline", "iterations_run": 1}),
            json!({"id": "S2", "baseline_sha": "def", "verdict": "advance", "iterations_run": 2}),
        ],
    );

    run_producer("experiment", project).unwrap();

    let v = read_receipt(project, "experiment-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
    assert_eq!(
        v.get("schema").and_then(serde_json::Value::as_str),
        Some("autobuilder.experiment_receipt.v1")
    );
    assert_eq!(
        v.get("campaign_slug").and_then(serde_json::Value::as_str),
        Some("test-campaign")
    );
    assert_eq!(
        v.get("slice_count").and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        v.get("total_iterations").and_then(serde_json::Value::as_u64),
        Some(2)
    );
}

#[test]
fn ac_experiment_2_planted_crash_yields_block() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_outcomes(
        project,
        &[
            json!({"id": "S1", "baseline_sha": "abc", "verdict": "baseline", "iterations_run": 1}),
            json!({"id": "S2", "baseline_sha": "def", "verdict": "crash", "iterations_run": 1}),
        ],
    );

    run_producer("experiment", project).unwrap();
    let v = read_receipt(project, "experiment-receipt.json");
    assert_eq!(verdict_of(&v), "block");
}

#[test]
fn ac_experiment_3_no_outcomes_file_yields_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    // Deliberately do NOT write experiment-outcomes.json.

    run_producer("experiment", project).unwrap();
    let v = read_receipt(project, "experiment-receipt.json");
    assert_eq!(verdict_of(&v), "skipped");
    let skip_reason = v
        .get("skip_reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    assert!(
        skip_reason.contains("no campaign outcomes file"),
        "unexpected skip_reason: {skip_reason}"
    );
}

#[test]
fn ac_experiment_4_idempotent_modulo_timestamp() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_outcomes(
        project,
        &[json!({"id": "S1", "baseline_sha": "abc", "verdict": "baseline", "iterations_run": 1})],
    );

    run_producer("experiment", project).unwrap();
    let mut first = read_receipt(project, "experiment-receipt.json");
    run_producer("experiment", project).unwrap();
    let mut second = read_receipt(project, "experiment-receipt.json");

    strip_volatile(&mut first);
    strip_volatile(&mut second);
    assert_eq!(
        first, second,
        "experiment receipts must be byte-identical modulo captured_at + receipt_digest"
    );
}
