//! `cold-build-time`: clean `cargo build --release` wall-time under budget.
//!
//! Budget comes from `extended-gates.toml::cold_build_time_max_seconds`
//! (default: 600). Producer runs `cargo clean` + `cargo build --release`
//! and times the build wall-clock.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    elapsed_seconds: f64,
    max_seconds: f64,
    cargo_exit: Option<i32>,
}

fn load_budget(project: &Path) -> f64 {
    let cfg_path = project.join("extended-gates.toml");
    let Ok(text) = std::fs::read_to_string(&cfg_path) else {
        return 600.0;
    };
    let Ok(value) = text.parse::<TomlValue>() else {
        return 600.0;
    };
    value
        .get("cold_build_time_max_seconds")
        .and_then(TomlValue::as_float)
        .or_else(|| {
            value
                .get("cold_build_time_max_seconds")
                .and_then(TomlValue::as_integer)
                .map(|n| {
                    #[allow(clippy::cast_precision_loss)]
                    {
                        n as f64
                    }
                })
        })
        .unwrap_or(600.0)
}

/// Run the cold-build-time audit.
///
/// # Errors
///
/// Returns an error if cargo can't be spawned or the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if std::env::var("AUTOBUILDER_SKIP_HEAVY").is_ok() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                elapsed_seconds: 0.0,
                max_seconds: 0.0,
                cargo_exit: None,
            },
        )?;
        return Ok("cold-build-time: skipped (AUTOBUILDER_SKIP_HEAVY)".into());
    }
    let max_seconds = load_budget(project);
    let _ = Command::new("cargo")
        .arg("clean")
        .current_dir(project)
        .status();

    let start = Instant::now();
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(project)
        .status()
        .context("spawn cargo build --release")?;
    let elapsed = start.elapsed().as_secs_f64();
    let exit = status.code();

    let verdict = if exit == Some(0) && elapsed <= max_seconds {
        "pass"
    } else {
        "block"
    };
    let summary = format!("cold-build-time: {elapsed:.1}s vs budget {max_seconds:.1}s");
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            elapsed_seconds: elapsed,
            max_seconds,
            cargo_exit: exit,
        },
    )?;
    Ok(summary)
}
