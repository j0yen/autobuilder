//! autobuilder — companion binary for the autobuilder Claude Code skill.
//!
//! Owns the load-bearing parts of the pipeline that should not live in shell:
//! intent-card validation, scaffold materialization, the experiment-loop
//! runner, EvidencePack writing, the 7-receipt risk gate, postmortem
//! aggregation, and the gated self-evolution diff.
//!
//! Each subcommand is currently a stub (`unimplemented!()`) — the skill's
//! shell scripts fall back to minimum-viable behavior in the meantime. The
//! first meta-PRD that autobuilder will dogfood against itself is the
//! `metric-harness` subcommand, per PLAN.md Phase C.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod evolve;
mod gate;
mod intake;
mod loop_runner;
mod metric_harness;
mod postmortem;
mod scaffold;

#[derive(Debug, Parser)]
#[command(
    name = "autobuilder",
    version,
    about = "PRD-driven, rigorously validated Rust code generation."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate an intent-card.json against the schema (Stage 1).
    Intake(intake::Args),

    /// Materialize a project tree from templates/scaffold/ (Stage 2).
    Scaffold(scaffold::Args),

    /// Run the iterate-and-prove loop (Stage 3).
    #[command(name = "loop")]
    LoopCmd(loop_runner::Args),

    /// Run a project's metric harness and emit normalized metrics.json.
    #[command(name = "metric-harness")]
    MetricHarness(metric_harness::Args),

    /// Check the 7-receipt risk gate (Stage 4).
    Gate(gate::Args),

    /// Aggregate run artifacts into a postmortem (Stage 5).
    Postmortem(postmortem::Args),

    /// Aggregate evolution-proposals and prepare a skill-self-diff (gated).
    Evolve(evolve::Args),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Intake(args) => intake::run(args),
        Command::Scaffold(args) => scaffold::run(args),
        Command::LoopCmd(args) => loop_runner::run(args),
        Command::MetricHarness(args) => metric_harness::run(args),
        Command::Gate(args) => gate::run(args),
        Command::Postmortem(args) => postmortem::run(args),
        Command::Evolve(args) => evolve::run(args),
    }
}
