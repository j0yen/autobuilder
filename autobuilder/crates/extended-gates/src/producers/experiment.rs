//! `experiment`: roll up a multi-slice campaign's per-slice outcomes into
//! one `autobuilder.experiment_receipt.v1`.
//!
//! Unlike the other 16 producers in this crate, this one does not perform
//! an independent audit. It reads `target/autobuilder/experiment-outcomes.json`
//! (written by `autobuilder experiment run` when a campaign completes)
//! and emits the corresponding receipt with a verdict derived from the
//! slices' iteration verdicts. When no outcomes file is present — i.e.
//! the project hasn't run a campaign — the producer emits `verdict=skipped`
//! with `skip_reason="no campaign outcomes file"`, so the 25-receipt gate
//! stays structurally complete on iterations that didn't opt into
//! `experiment run`.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Deserialize)]
struct OutcomesDoc {
    campaign_slug: String,
    total_iterations: u32,
    wall_clock_seconds: u64,
    slices: Vec<SliceOutcome>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SliceOutcome {
    id: String,
    baseline_sha: String,
    verdict: String,
    iterations_run: u32,
}

#[derive(Debug, Serialize)]
struct Payload {
    campaign_slug: String,
    total_iterations: u32,
    wall_clock_seconds: u64,
    slice_count: usize,
    slices: Vec<SliceOutcome>,
    skip_reason: Option<String>,
}

const SLICE_PASS_VERDICTS: &[&str] = &["baseline", "advance"];

/// Run the experiment-receipt producer for the given project.
///
/// # Errors
/// Returns an error if the outcomes file is present but malformed, or if
/// the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let outcomes_path = project.join("target/autobuilder/experiment-outcomes.json");
    if !outcomes_path.is_file() {
        let payload = Payload {
            campaign_slug: String::new(),
            total_iterations: 0,
            wall_clock_seconds: 0,
            slice_count: 0,
            slices: Vec::new(),
            skip_reason: Some(format!(
                "no campaign outcomes file at {}",
                outcomes_path.display()
            )),
        };
        write_receipt(project, spec, "skipped", payload)?;
        return Ok(format!(
            "experiment: skipped (no outcomes file at {})",
            outcomes_path.display()
        ));
    }

    let text = std::fs::read_to_string(&outcomes_path)
        .with_context(|| format!("reading {}", outcomes_path.display()))?;
    let outcomes: OutcomesDoc = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", outcomes_path.display()))?;

    let any_bad = outcomes
        .slices
        .iter()
        .any(|s| !SLICE_PASS_VERDICTS.contains(&s.verdict.as_str()));
    let verdict: &'static str = if any_bad { "block" } else { "pass" };

    let slice_count = outcomes.slices.len();
    let summary = format!(
        "experiment: {verdict} (campaign `{}`, {slice_count} slice(s), {} iterations, {}s wall-clock)",
        outcomes.campaign_slug, outcomes.total_iterations, outcomes.wall_clock_seconds
    );
    let payload = Payload {
        campaign_slug: outcomes.campaign_slug,
        total_iterations: outcomes.total_iterations,
        wall_clock_seconds: outcomes.wall_clock_seconds,
        slice_count,
        slices: outcomes.slices,
        skip_reason: None,
    };
    write_receipt(project, spec, verdict, payload)?;
    Ok(summary)
}
