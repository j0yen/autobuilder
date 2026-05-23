//! `bench-delta`: criterion benches don't regress >X% vs a frozen baseline JSON.
//!
//! Baseline lives at `<project>/extended-gates.bench-baseline.json` (a
//! mapping bench-name → mean nanoseconds). If absent, the producer emits
//! `verdict=skipped`. Otherwise it reads `target/criterion/*/new/estimates.json`
//! (criterion's standard output layout) and compares.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    baseline_path: String,
    max_regression_pct: f64,
    comparisons: Vec<Comparison>,
    regressed: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Comparison {
    bench: String,
    baseline_ns: f64,
    current_ns: f64,
    delta_pct: f64,
}

fn load_threshold(project: &Path) -> f64 {
    let cfg_path = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg_path) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(n) = value
                .get("bench_max_regression_pct")
                .and_then(TomlValue::as_float)
            {
                return n;
            }
        }
    }
    5.0
}

fn read_baseline(project: &Path) -> Option<BTreeMap<String, f64>> {
    let path = project.join("extended-gates.bench-baseline.json");
    let bytes = std::fs::read(&path).ok()?;
    let value: JsonValue = serde_json::from_slice(&bytes).ok()?;
    let obj = value.as_object()?;
    let mut out = BTreeMap::new();
    for (k, v) in obj {
        if let Some(n) = v.as_f64() {
            out.insert(k.clone(), n);
        }
    }
    Some(out)
}

fn collect_current(project: &Path) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let crit = project.join("target/criterion");
    if !crit.is_dir() {
        return out;
    }
    let walker = walkdir::WalkDir::new(&crit).max_depth(4);
    for entry in walker.into_iter().flatten() {
        if entry.file_name() != "estimates.json" {
            continue;
        }
        if !entry.path().to_string_lossy().contains("/new/") {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<JsonValue>(&bytes) else {
            continue;
        };
        let mean = value
            .get("mean")
            .and_then(|m| m.get("point_estimate"))
            .and_then(JsonValue::as_f64);
        let bench = entry
            .path()
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_owned();
        if let Some(mean) = mean {
            out.insert(bench, mean);
        }
    }
    out
}

/// Run the bench-delta audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let Some(baseline) = read_baseline(project) else {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                baseline_path: "extended-gates.bench-baseline.json".into(),
                max_regression_pct: 0.0,
                comparisons: Vec::new(),
                regressed: vec!["no baseline file".into()],
            },
        )?;
        return Ok("bench-delta: skipped (no baseline)".into());
    };

    let threshold = load_threshold(project);
    let current = collect_current(project);

    let mut comparisons: Vec<Comparison> = Vec::new();
    let mut regressed: Vec<String> = Vec::new();
    for (bench, base) in &baseline {
        let Some(cur) = current.get(bench) else {
            continue;
        };
        let delta_pct = ((cur - base) / base) * 100.0;
        if delta_pct > threshold {
            regressed.push(bench.clone());
        }
        comparisons.push(Comparison {
            bench: bench.clone(),
            baseline_ns: *base,
            current_ns: *cur,
            delta_pct,
        });
    }

    let verdict = if regressed.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "bench-delta: {} benches compared, {} regressed > {:.1}%",
        comparisons.len(),
        regressed.len(),
        threshold
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            baseline_path: "extended-gates.bench-baseline.json".into(),
            max_regression_pct: threshold,
            comparisons,
            regressed,
        },
    )?;
    Ok(summary)
}
