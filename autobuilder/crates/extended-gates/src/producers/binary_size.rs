//! `binary-size`: every `target/release/*` binary is under its configured budget.
//!
//! Budgets come from `<project>/extended-gates.toml::binary_size_budgets`
//! (a table mapping bin name → max bytes). Missing entries default to the
//! `default_max_bytes` key or 50 MiB.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    default_max_bytes: u64,
    measurements: Vec<Measurement>,
    over_budget: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Measurement {
    name: String,
    bytes: u64,
    max_bytes: u64,
}

fn load_budgets(project: &Path) -> (u64, HashMap<String, u64>) {
    let mut map: HashMap<String, u64> = HashMap::new();
    let mut default_max = 50 * 1024 * 1024;
    let cfg_path = project.join("extended-gates.toml");
    let Ok(text) = std::fs::read_to_string(&cfg_path) else {
        return (default_max, map);
    };
    let Ok(value) = text.parse::<TomlValue>() else {
        return (default_max, map);
    };
    if let Some(d) = value
        .get("default_max_bytes")
        .and_then(TomlValue::as_integer)
    {
        if d >= 0 {
            #[allow(clippy::cast_sign_loss)]
            {
                default_max = d as u64;
            }
        }
    }
    if let Some(table) = value
        .get("binary_size_budgets")
        .and_then(TomlValue::as_table)
    {
        for (k, v) in table {
            if let Some(n) = v.as_integer() {
                if n >= 0 {
                    #[allow(clippy::cast_sign_loss)]
                    {
                        map.insert(k.clone(), n as u64);
                    }
                }
            }
        }
    }
    (default_max, map)
}

/// Run the binary-size audit.
///
/// # Errors
///
/// Returns an error if `target/release` can't be read or the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let (default_max, budgets) = load_budgets(project);
    let release_dir = project.join("target/release");
    let mut measurements: Vec<Measurement> = Vec::new();
    let mut over_budget: Vec<String> = Vec::new();

    if !release_dir.exists() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                default_max_bytes: default_max,
                measurements,
                over_budget: vec!["target/release not found".into()],
            },
        )?;
        return Ok("binary-size: skipped (target/release missing)".into());
    }

    let rd = std::fs::read_dir(&release_dir)
        .with_context(|| format!("read {}", release_dir.display()))?;
    for entry in rd.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let bytes = meta.len();
        let max_bytes = budgets.get(&name).copied().unwrap_or(default_max);
        if bytes > max_bytes {
            over_budget.push(name.clone());
        }
        measurements.push(Measurement {
            name,
            bytes,
            max_bytes,
        });
    }

    let verdict = if over_budget.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "binary-size: {} bins measured, {} over budget",
        measurements.len(),
        over_budget.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            default_max_bytes: default_max,
            measurements,
            over_budget,
        },
    )?;
    Ok(summary)
}
