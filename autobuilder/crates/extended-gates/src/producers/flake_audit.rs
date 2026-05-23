//! `flake-audit`: `cargo test` rerun K times produces identical outcomes.
//!
//! Runs `cargo test --quiet` K times (default K=3, configurable via
//! `extended-gates.toml::flake_audit_runs`) and asserts every run's exit
//! code is identical. Heavy; respects `AUTOBUILDER_SKIP_HEAVY`.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    runs: usize,
    exit_codes: Vec<Option<i32>>,
    deterministic: bool,
}

fn runs_count(project: &Path) -> usize {
    let cfg = project.join("extended-gates.toml");
    if let Ok(text) = std::fs::read_to_string(&cfg) {
        if let Ok(value) = text.parse::<TomlValue>() {
            if let Some(n) = value.get("flake_audit_runs").and_then(TomlValue::as_integer) {
                if (1..=20).contains(&n) {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    return n as usize;
                }
            }
        }
    }
    3
}

/// Run the flake-audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if std::env::var("AUTOBUILDER_SKIP_HEAVY").is_ok() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                runs: 0,
                exit_codes: Vec::new(),
                deterministic: true,
            },
        )?;
        return Ok("flake-audit: skipped (AUTOBUILDER_SKIP_HEAVY)".into());
    }
    let runs = runs_count(project);
    let mut codes: Vec<Option<i32>> = Vec::with_capacity(runs);
    for _ in 0..runs {
        let status = Command::new("cargo")
            .args(["test", "--quiet"])
            .current_dir(project)
            .status();
        codes.push(status.ok().and_then(|s| s.code()));
    }
    let deterministic = codes
        .windows(2)
        .all(|w| w.first() == w.get(1));
    let verdict = if deterministic && codes.first().is_some_and(|c| *c == Some(0)) {
        "pass"
    } else if codes.is_empty() {
        "skipped"
    } else {
        "block"
    };
    let summary = format!(
        "flake-audit: {runs} runs, exit codes {codes:?}, deterministic={deterministic}"
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            runs,
            exit_codes: codes,
            deterministic,
        },
    )?;
    Ok(summary)
}
