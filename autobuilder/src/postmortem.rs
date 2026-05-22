//! Stage 5 — Postmortem aggregator. Reads `target/autobuilder/{results.tsv,
//! receipts/*, failure-capsules/*}`, produces `target/autobuilder/postmortem.md`
//! (via the postmortem-writer prompt), and queues a machine-readable
//! `evolution-proposal-*.json` in `~/.claude/skills/autobuilder/proposals/`.

use anyhow::{Result, anyhow};
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing target/autobuilder/.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    Err(anyhow!(
        "autobuilder postmortem: not yet implemented — should load results.tsv + receipts + capsules, \
         render postmortem.md, write evolution-proposal-<slug>-<ts>.json into \
         ~/.claude/skills/autobuilder/proposals/"
    ))
}
