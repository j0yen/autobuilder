//! autobuilder — companion binary for the autobuilder Claude Code skill.
//!
//! Owns the load-bearing parts of the pipeline that should not live in shell:
//! intent-card validation, scaffold materialization, the experiment-loop
//! runner, `EvidencePack` writing, the 7-receipt risk gate, postmortem
//! aggregation, and the gated self-evolution diff.
//!
//! Several subcommands are still stubs that return a typed error explaining
//! what they should do; the skill's shell scripts fall back to minimum-viable
//! behavior in the meantime. The first meta-PRD that autobuilder dogfooded
//! against itself is the `metric-harness` subcommand, per PLAN.md Phase C.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod adversarial;
mod ci_checks;
mod evolve;
mod experiment;
mod gate;
mod intake;
mod loop_runner;
mod metric_harness;
mod postmortem;
mod receipt;
mod reviewer;
mod rollback;
mod scaffold;
mod vti_plan;

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

    /// Verify every commit in HEAD~N..HEAD is git-revert-clean; emit rollback receipt (Stage 4).
    #[command(name = "rollback-plan")]
    RollbackPlan(rollback::Args),

    /// Prepare / finalize the reviewer-agent receipt (Stage 4).
    #[command(name = "reviewer-agent")]
    ReviewerAgent(reviewer::Args),

    /// Route changed paths through agent/proof-lanes.toml; emit vti-plan receipt (Stage 4).
    #[command(name = "vti-plan")]
    VtiPlan(vti_plan::Args),

    /// Verify CI workflow runs on HEAD are green via `gh`; emit ci-checks receipt (Stage 4).
    #[command(name = "ci-checks")]
    CiChecks(ci_checks::Args),

    /// Aggregate run artifacts into a postmortem (Stage 5).
    Postmortem(postmortem::Args),

    /// Aggregate evolution-proposals and prepare a skill-self-diff (gated).
    Evolve(evolve::Args),

    /// Prepare / finalize an adversarial-agent test slot (Stage 3 sub-step).
    Adversarial(adversarial::Args),

    /// Drive a multi-slice campaign from experiment.toml (Stage 2.5).
    Experiment(experiment::Args),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Intake(args) => intake::run(args),
        Command::Scaffold(args) => scaffold::run(args),
        Command::LoopCmd(args) => loop_runner::run(args),
        Command::MetricHarness(args) => metric_harness::run(args),
        Command::Gate(args) => gate::run(args),
        Command::RollbackPlan(args) => rollback::run(args),
        Command::ReviewerAgent(args) => reviewer::run(args),
        Command::VtiPlan(args) => vti_plan::run(args),
        Command::CiChecks(args) => ci_checks::run(args),
        Command::Postmortem(args) => postmortem::run(args),
        Command::Evolve(args) => evolve::run(args),
        Command::Adversarial(args) => adversarial::run(args),
        Command::Experiment(args) => experiment::run(args),
    }
}
