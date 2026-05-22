//! Stage 2 — Scaffold. Materializes a Rust project tree from
//! `~/.claude/skills/autobuilder/templates/scaffold/` using the intent-card
//! as the source of substitutions.

use anyhow::{Result, anyhow};
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Path to the validated intent-card.json.
    #[arg(long = "intent-card")]
    pub intent_card: PathBuf,

    /// Output directory for the materialized project. Must not exist.
    #[arg(long)]
    pub out: PathBuf,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    Err(anyhow!(
        "autobuilder scaffold: not yet implemented — should copy templates/scaffold/ into <out>, \
         substitute {{intent_slug}} / {{target_kind}}, generate tests/acceptance_<ac>.rs files \
         from intent-card ACs, instantiate AUTOBUILDER_PROGRAM.md from the .tmpl"
    ))
}
