//! AC-bench-delta.{1,2}: bench within threshold → pass; bench regressed
//! beyond threshold → block.
//!
//! Synthesizes the criterion `target/criterion/<bench>/new/estimates.json`
//! layout the producer reads. No real benches run; the producer's contract
//! is "given baseline + current numbers, do the right thing."

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod fixtures;
use fixtures::*;

use autobuilder_extended_gates::run_producer;

fn write_baseline(project: &std::path::Path, name: &str, mean_ns: f64) {
    std::fs::write(
        project.join("extended-gates.bench-baseline.json"),
        format!("{{\"{name}\":{mean_ns}}}"),
    )
    .unwrap();
}

fn write_current_estimate(project: &std::path::Path, name: &str, mean_ns: f64) {
    let dir = project.join(format!("target/criterion/{name}/new"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("estimates.json"),
        format!("{{\"mean\":{{\"point_estimate\":{mean_ns}}}}}"),
    )
    .unwrap();
}

#[test]
fn ac_bench_delta_1_within_threshold_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_baseline(project, "decode", 1000.0);
    write_current_estimate(project, "decode", 1010.0); // +1%
    std::fs::write(
        project.join("extended-gates.toml"),
        b"bench_max_regression_pct = 5.0\n",
    )
    .unwrap();
    run_producer("bench-delta", project).unwrap();
    let v = read_receipt(project, "bench-delta-receipt.json");
    assert_eq!(verdict_of(&v), "pass");
}

#[test]
fn ac_bench_delta_2_regressed_bench_blocks() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path();
    init_git(project);
    write_baseline(project, "decode", 1000.0);
    write_current_estimate(project, "decode", 1500.0); // +50%
    std::fs::write(
        project.join("extended-gates.toml"),
        b"bench_max_regression_pct = 5.0\n",
    )
    .unwrap();
    run_producer("bench-delta", project).unwrap();
    let v = read_receipt(project, "bench-delta-receipt.json");
    assert_eq!(verdict_of(&v), "block");
    let regressed = v
        .get("regressed")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    assert!(regressed.iter().any(|s| s.as_str() == Some("decode")));
}
