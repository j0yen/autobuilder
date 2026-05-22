//! Stage 1 — Intake. Validates an `intent-card.json` against the schema.
//!
//! The conversational 5-Whys interview itself lives in the skill's prompt
//! (`prompts/prd-intake-5whys.md`); this subcommand just enforces the
//! schema contract on the output.

use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Path to the intent-card.json to validate.
    #[arg(long)]
    pub validate: PathBuf,
}

pub(crate) fn run(_args: Args) -> Result<()> {
    unimplemented!("autobuilder intake: schema-validate the intent-card.json against schemas/intent-card.schema.json")
}
