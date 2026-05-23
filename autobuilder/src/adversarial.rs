//! Stage 3 sub-step — adversarial-agent launcher.
//!
//! The actual adversarial test-writing is performed by an independent
//! Claude subagent (see `~/.claude/skills/autobuilder/prompts/adversarial-agent.md`).
//! This binary cannot spawn that subagent — that's the orchestrating
//! Claude session's job. What this module owns:
//!
//! * `prepare` — assemble a deterministic `adversarial-request.json` packet
//!               (head sha, AC id + level + description, the existing
//!               acceptance test source, the implementation source files)
//!               that the orchestrator hands to the subagent as context.
//! * `finalize` — read the subagent's `.rs` output, install it at
//!                `tests/adversarial_<ac_id>.rs`, run `cargo test --test
//!                adversarial_<ac_id>`, and report the outcome (pass /
//!                concern). Concern fires when any adversarial test FAILS
//!                — that's the signal the implementation is weaker than
//!                the AC requires, even though the loop's existing
//!                `acceptance_<ac_id>` test still passes.

use anyhow::{Context, Result, anyhow};
use clap::{Args as ClapArgs, Subcommand};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Action {
    /// Assemble target/autobuilder/adversarial-request-<ac>.json packets.
    Prepare(PrepareArgs),
    /// Install + run an adversarial test file produced by the subagent.
    Finalize(FinalizeArgs),
}

#[derive(Debug, ClapArgs)]
pub(crate) struct PrepareArgs {
    /// Project directory containing agent/intent-card.json.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Single AC id to prepare a packet for (e.g. AC5). When omitted, a
    /// packet is written for every MUST-level AC.
    #[arg(long)]
    pub ac: Option<String>,
}

#[derive(Debug, ClapArgs)]
pub(crate) struct FinalizeArgs {
    /// Project directory.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// AC id this adversarial test attacks (e.g. AC5). Used to derive
    /// the install path `tests/adversarial_<ac_id_lowercase>.rs`.
    #[arg(long)]
    pub ac: String,

    /// Path to the `.rs` file the adversarial subagent produced.
    #[arg(long)]
    pub input: PathBuf,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    match args.action {
        Action::Prepare(a) => prepare(a),
        Action::Finalize(a) => finalize(a),
    }
}

#[derive(Debug, Serialize)]
struct AdversarialRequest {
    schema: &'static str,
    head_sha: String,
    ac_id: String,
    ac_level: String,
    ac_description: String,
    ac_test_predicate: String,
    /// Verbatim contents of the existing `tests/acceptance_<id>.rs` so
    /// the subagent can read what the edit-agent wrote and design
    /// attacks the existing assertions wouldn't catch.
    existing_acceptance_test: String,
    /// Relative paths of every `.rs` under `src/` so the subagent knows
    /// the implementation surface. The orchestrator should also pass
    /// the actual file contents to the subagent, but they're not
    /// duplicated in the packet (they live in src/).
    src_files: Vec<String>,
    captured_at: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `PrepareArgs` is the clap-dispatched contract
fn prepare(args: PrepareArgs) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;
    let head_sha = git_head(&project)?;
    let intent_card_path = project.join("agent/intent-card.json");
    let intent_card_text = fs::read_to_string(&intent_card_path)
        .with_context(|| format!("missing {}", intent_card_path.display()))?;
    let intent_card: Value =
        serde_json::from_str(&intent_card_text).context("intent-card.json is not valid JSON")?;
    let acs = intent_card
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("intent-card.json missing acceptance_criteria array"))?;
    let src_files = list_src_files(&project)?;
    let out_dir = project.join("target/autobuilder");
    fs::create_dir_all(&out_dir).ok();

    let captured_at = crate::receipt::now_rfc3339()?;
    let mut written = 0usize;
    for ac in acs {
        let id = ac.get("id").and_then(Value::as_str).unwrap_or("");
        let level = ac.get("level").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        if let Some(ref only) = args.ac {
            if !only.eq_ignore_ascii_case(id) {
                continue;
            }
        }
        // Default behavior: only emit packets for MUST-level ACs. SHOULD/MAY
        // are skipped because the adversarial-agent overhead per AC is
        // significant and the SHOULD/MAY weakening is already explicit.
        // --ac override emits regardless of level.
        if args.ac.is_none() && level != "MUST" {
            continue;
        }
        let description = ac.get("description").and_then(Value::as_str).unwrap_or("");
        let test_predicate = ac.get("test").and_then(Value::as_str).unwrap_or("");
        let acceptance_path = project.join(format!(
            "tests/acceptance_{}.rs",
            id.to_ascii_lowercase()
        ));
        let acceptance_src = fs::read_to_string(&acceptance_path).unwrap_or_default();
        let request = AdversarialRequest {
            schema: "autobuilder.adversarial_request.v1",
            head_sha: head_sha.clone(),
            ac_id: id.to_owned(),
            ac_level: level.to_owned(),
            ac_description: description.to_owned(),
            ac_test_predicate: test_predicate.to_owned(),
            existing_acceptance_test: acceptance_src,
            src_files: src_files.clone(),
            captured_at: captured_at.clone(),
        };
        let request_path = out_dir.join(format!(
            "adversarial-request-{}.json",
            id.to_ascii_lowercase()
        ));
        fs::write(&request_path, serde_json::to_vec_pretty(&request)?)
            .with_context(|| format!("cannot write {}", request_path.display()))?;
        written += 1;
    }

    if written == 0 {
        return Err(anyhow!(
            "no adversarial-request packets written (no MUST-level ACs in intent-card, or --ac filter matched nothing)"
        ));
    }
    println!(
        "adversarial prepare: wrote {written} packet(s) to {}",
        out_dir.display()
    );
    println!(
        "Next: for each packet, spawn the adversarial subagent with:\n\
         \x20 prompt: ~/.claude/skills/autobuilder/prompts/adversarial-agent.md\n\
         \x20 context: target/autobuilder/adversarial-request-<ac>.json\n\
         Then: autobuilder adversarial finalize --project <p> --ac <id> --input <subagent-output>.rs"
    );
    Ok(())
}

#[allow(clippy::needless_pass_by_value)] // owned `FinalizeArgs` is the clap-dispatched contract
fn finalize(args: FinalizeArgs) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;
    let ac_lower = args.ac.to_ascii_lowercase();
    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("missing adversarial output at {}", args.input.display()))?;
    if raw.trim().is_empty() {
        return Err(anyhow!("adversarial input is empty"));
    }
    // Honor the "SKIPPED" sentinel from prompts/adversarial-agent.md.
    if raw.lines().any(|l| l.trim().starts_with("// SKIPPED:")) {
        println!(
            "adversarial finalize: SKIPPED sentinel detected for {}; no test installed",
            args.ac
        );
        return Ok(());
    }
    let tests_dir = project.join("tests");
    fs::create_dir_all(&tests_dir).ok();
    let install_path = tests_dir.join(format!("adversarial_{ac_lower}.rs"));
    fs::write(&install_path, &raw)
        .with_context(|| format!("cannot install {}", install_path.display()))?;
    // Build first so a compile error is reported clearly, before cargo test.
    let build = Command::new("cargo")
        .arg("test")
        .arg("--no-run")
        .arg("--test")
        .arg(format!("adversarial_{ac_lower}"))
        .current_dir(&project)
        .output()
        .with_context(|| "failed to spawn cargo test --no-run for adversarial install")?;
    if !build.status.success() {
        return Err(anyhow!(
            "adversarial test failed to compile:\n{}",
            String::from_utf8_lossy(&build.stderr).trim()
        ));
    }
    let run = Command::new("cargo")
        .arg("test")
        .arg("--test")
        .arg(format!("adversarial_{ac_lower}"))
        .current_dir(&project)
        .output()
        .with_context(|| "failed to spawn cargo test for adversarial run")?;
    let outcome = if run.status.success() {
        "pass"
    } else {
        "concern"
    };
    println!(
        "adversarial finalize: ac={} outcome={outcome} installed={}",
        args.ac,
        install_path.display()
    );
    if outcome == "concern" {
        // Concern is informational; the orchestrator decides whether to
        // downgrade the iteration's verdict. We return non-zero so a
        // wrapping shell script can branch on it.
        return Err(anyhow!(
            "adversarial test FAILED on the implementation: the AC's English description is not fully satisfied, even though the existing acceptance_{} passes. Iteration should be downgraded from advance to concern.",
            args.ac
        ));
    }
    Ok(())
}

fn git_head(project: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("git rev-parse HEAD failed to spawn")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn list_src_files(project: &Path) -> Result<Vec<String>> {
    let src = project.join("src");
    if !src.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_rs(&src, project, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_rs(dir: &Path, project: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, project, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(rel) = path.strip_prefix(project) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    Ok(())
}
