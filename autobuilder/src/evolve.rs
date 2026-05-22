//! Gated self-evolution aggregator. Collapses
//! `~/.claude/skills/autobuilder/proposals/evolution-proposal-*.json` across
//! recent runs into a ranked recommendation list and a unified diff against
//! the skill itself. Never auto-applies.

use anyhow::{Result, anyhow};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Only consider proposals newer than this ISO 8601 date.
    #[arg(long)]
    pub since: Option<String>,

    /// Cap the number of top recommendations surfaced.
    #[arg(long, default_value_t = 10)]
    pub max: u32,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    Err(anyhow!(
        "autobuilder evolve: not yet implemented — should aggregate proposals, dedupe against applied.log, \
         rank by total estimated_iters_saved × distinct-run-count, emit \
         evolve-report-<date>.md and evolve-diff-<date>.patch into \
         ~/.claude/skills/autobuilder/proposals/"
    ))
}
