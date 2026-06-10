use autobuilder_proposal_aggregator::{run, shellexpand};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "autobuilder-proposal-aggregator")]
#[command(about = "Cluster autobuilder self-evolve proposals into a ranked hardening backlog")]
struct Cli {
    /// Directory containing proposal JSON files
    #[arg(long, default_value = "~/.claude/skills/autobuilder/proposals")]
    proposals_dir: String,

    /// Path to applied.log (default: <proposals-dir>/applied.log)
    #[arg(long)]
    applied_log: Option<String>,

    /// Only show clusters hit by >= N distinct slugs
    #[arg(long, default_value_t = 1)]
    min_recurrence: usize,

    /// Output format: json or human
    #[arg(long, default_value = "json")]
    format: String,
}

fn main() {
    let cli = Cli::parse();

    let proposals_dir = cli.proposals_dir;
    let applied_log = cli.applied_log.unwrap_or_else(|| {
        let expanded = shellexpand(&proposals_dir);
        format!("{expanded}/applied.log")
    });

    match run(&proposals_dir, &applied_log, cli.min_recurrence, &cli.format) {
        Ok(output) => {
            println!("{output}");
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
