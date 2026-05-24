//! Stage 2.5 — Experiment manifest. Validates an `experiment.toml` against
//! `autobuilder.experiment_manifest.v1` and (per later slices) drives a
//! multi-slice campaign through the per-iteration loop.
//!
//! Where `intake.rs` validates a single PRD's intent-card, this module
//! validates a campaign-level manifest that references one or more
//! intent-cards in order. The hand-validation style mirrors `intake.rs`
//! exactly: every error reported at once, JSON-pointer-style paths,
//! no JSON Schema engine.

use anyhow::{Context, Result, anyhow};
use clap::{Args as ClapArgs, Subcommand};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use toml::Value;

use crate::intake;
use crate::loop_runner;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: ExperimentCmd,
}

#[derive(Debug, Subcommand)]
enum ExperimentCmd {
    /// Validate an experiment.toml manifest against the schema.
    Validate {
        /// Path to the experiment.toml manifest.
        #[arg(long)]
        manifest: PathBuf,
    },

    /// Run the multi-slice campaign described by an experiment.toml.
    ///
    /// Walks each slice in order, invokes the per-iteration loop on each
    /// (only iteration 0 / baseline today; full edit-agent loop lands in
    /// a follow-up), and applies the configured transition policy between
    /// slices. Until the native edit-agent crate ships, `--no-edit-agent`
    /// is required and the driver will not progress past baseline.
    Run {
        /// Path to the experiment.toml manifest.
        #[arg(long)]
        manifest: PathBuf,

        /// Run only iteration 0 per slice; do not invoke an edit-agent
        /// between iterations. Required until the native edit-agent
        /// crate lands.
        #[arg(long)]
        no_edit_agent: bool,
    },
}

const SCHEMA_ID: &str = "autobuilder.experiment_manifest.v1";

const REQUIRED_TOP: &[&str] = &["schema", "campaign", "edit_agent", "slices"];
const ALLOWED_TOP: &[&str] = &["schema", "campaign", "edit_agent", "slices"];

const CAMPAIGN_REQUIRED: &[&str] = &[
    "slug",
    "prd_source",
    "max_wall_clock_minutes",
    "max_total_iterations",
];
const CAMPAIGN_ALLOWED: &[&str] = &[
    "slug",
    "prd_source",
    "max_wall_clock_minutes",
    "max_total_iterations",
];

const EDIT_AGENT_REQUIRED: &[&str] = &["model", "max_tokens_per_call"];
const EDIT_AGENT_ALLOWED: &[&str] = &[
    "model",
    "api_key_env",
    "max_tokens_per_call",
    "fallback_to_signal_mode",
];

const SLICE_REQUIRED: &[&str] = &["id", "intent_card", "transition"];
const SLICE_ALLOWED: &[&str] = &["id", "intent_card", "max_iterations", "transition"];

/// Claude model ids the edit-agent may target. Synced manually with the
/// system prompt's "most recent Claude model family" list; out-of-list
/// values fail validation rather than silently mistargeting.
const KNOWN_MODELS: &[&str] = &[
    "claude-opus-4-7",
    "claude-sonnet-4-6",
    "claude-haiku-4-5-20251001",
];

const TRANSITIONS: &[&str] = &["reset", "advance-commit", "continue"];

#[allow(clippy::needless_pass_by_value)] // owned Args matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    match args.command {
        ExperimentCmd::Validate { manifest } => {
            let (slug, slice_count) = validate_file(&manifest)?;
            println!(
                "experiment validate: {} valid (slug={slug} slices={slice_count})",
                manifest.display()
            );
            Ok(())
        }
        ExperimentCmd::Run {
            manifest,
            no_edit_agent,
        } => run_campaign(&manifest, no_edit_agent),
    }
}

/// Strongly-typed view of a validated experiment.toml.
///
/// Built via serde *after* hand-validation has accepted the manifest;
/// the validation pass owns error messages, the typed view owns
/// post-validation traversal.
#[derive(Debug, Deserialize)]
struct Manifest {
    campaign: Campaign,
    #[allow(dead_code)] // consumed by the edit-agent in a later slice
    edit_agent: EditAgent,
    slices: Vec<Slice>,
}

#[derive(Debug, Deserialize)]
struct Campaign {
    slug: String,
    max_wall_clock_minutes: u64,
    max_total_iterations: u32,
}

#[derive(Debug, Deserialize)]
struct EditAgent {
    #[allow(dead_code)] // consumed in S3
    model: String,
    #[serde(default = "default_api_key_env")]
    #[allow(dead_code)] // consumed in S3
    api_key_env: String,
    #[allow(dead_code)] // consumed in S3
    max_tokens_per_call: u32,
    #[serde(default)]
    #[allow(dead_code)] // consumed in S3
    fallback_to_signal_mode: bool,
}

fn default_api_key_env() -> String {
    "ANTHROPIC_API_KEY".to_owned()
}

#[derive(Debug, Deserialize)]
struct Slice {
    id: String,
    intent_card: String,
    #[serde(default = "default_max_iterations")]
    #[allow(dead_code)] // consumed by the iterate-until-pass loop in S3; only baseline iteration runs today
    max_iterations: u32,
    transition: String,
}

fn default_max_iterations() -> u32 {
    4
}

/// Outcome of one slice's run inside a campaign.
///
/// Held in memory by the campaign driver and rolled up into the
/// campaign-receipt in a later slice. Fields are crate-private —
/// downstream consumers will live in this module.
#[derive(Debug)]
pub(crate) struct SliceOutcome {
    pub(crate) id: String,
    pub(crate) baseline_sha: String,
    pub(crate) verdict: String,
    pub(crate) iterations_run: u32,
}

#[allow(clippy::too_many_lines)] // single linear campaign-driver pipeline; splitting hides the flow
fn run_campaign(manifest_path: &Path, no_edit_agent: bool) -> Result<()> {
    if !no_edit_agent {
        return Err(anyhow!(
            "--no-edit-agent is required; native edit-agent invocation lands in a follow-up slice"
        ));
    }

    // Validate first so the error shape matches `experiment validate` exactly.
    let _summary = validate_file(manifest_path)?;

    let text = fs::read_to_string(manifest_path)
        .with_context(|| format!("re-reading {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&text)
        .context("manifest re-parse after validation must succeed")?;

    let manifest_dir = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

    let start_time = Instant::now();
    let wall_clock_budget = std::time::Duration::from_secs(60 * manifest.campaign.max_wall_clock_minutes);
    let mut total_iterations: u32 = 0;
    let mut outcomes: Vec<SliceOutcome> = Vec::new();
    let mut first_project: Option<PathBuf> = None;

    for slice in &manifest.slices {
        if start_time.elapsed() > wall_clock_budget {
            return Err(anyhow!(
                "campaign wall-clock budget ({} min) exhausted before slice {}",
                manifest.campaign.max_wall_clock_minutes,
                slice.id
            ));
        }
        if total_iterations >= manifest.campaign.max_total_iterations {
            return Err(anyhow!(
                "campaign max_total_iterations ({}) reached before slice {}",
                manifest.campaign.max_total_iterations,
                slice.id
            ));
        }

        let intent_card_path = manifest_dir.join(&slice.intent_card);
        let project = derive_project_dir(&intent_card_path)?;
        if first_project.is_none() {
            first_project = Some(project.clone());
        }
        let baseline_sha = git_head_sha(&project)?;

        let loop_args = loop_runner::Args {
            project: project.clone(),
            iteration: 0,
            head_sha: baseline_sha.clone(),
            description: format!("experiment slice {} baseline (no-edit-agent)", slice.id),
            trace: false,
            trace_binary: "ctrace".to_owned(),
            adversarial: false,
        };
        loop_runner::run(loop_args)
            .with_context(|| format!("slice {} iteration 0 failed", slice.id))?;
        total_iterations += 1;

        let verdict = read_iteration_verdict(&project, &baseline_sha)?;

        apply_transition(&project, &slice.transition, &baseline_sha)
            .with_context(|| format!("slice {} transition `{}` failed", slice.id, slice.transition))?;

        outcomes.push(SliceOutcome {
            id: slice.id.clone(),
            baseline_sha,
            verdict,
            iterations_run: 1,
        });
    }

    let wall_clock_seconds = start_time.elapsed().as_secs();

    // Write the intermediate outcomes file at the first slice's project root
    // (campaigns conventionally root all slices under one project tree). The
    // `experiment` producer in `autobuilder-extended-gates` reads this file
    // to emit the digest-bound campaign receipt.
    if let Some(project) = first_project.as_ref() {
        write_outcomes_file(
            project,
            &manifest.campaign.slug,
            total_iterations,
            wall_clock_seconds,
            &outcomes,
        )?;
    }

    println!(
        "experiment run: campaign `{}` completed — {} slice(s), {} iteration(s), {}s wall-clock",
        manifest.campaign.slug,
        outcomes.len(),
        total_iterations,
        wall_clock_seconds,
    );
    for o in &outcomes {
        println!(
            "  slice {} baseline={} verdict={} iterations={}",
            o.id, o.baseline_sha, o.verdict, o.iterations_run
        );
    }
    Ok(())
}

fn write_outcomes_file(
    project: &Path,
    campaign_slug: &str,
    total_iterations: u32,
    wall_clock_seconds: u64,
    outcomes: &[SliceOutcome],
) -> Result<()> {
    let dir = project.join("target/autobuilder");
    fs::create_dir_all(&dir).context("creating target/autobuilder/")?;
    let path = dir.join("experiment-outcomes.json");
    let slices: Vec<serde_json::Value> = outcomes
        .iter()
        .map(|o| {
            serde_json::json!({
                "id": o.id,
                "baseline_sha": o.baseline_sha,
                "verdict": o.verdict,
                "iterations_run": o.iterations_run,
            })
        })
        .collect();
    let doc = serde_json::json!({
        "schema": "autobuilder.experiment_outcomes.v1",
        "campaign_slug": campaign_slug,
        "total_iterations": total_iterations,
        "wall_clock_seconds": wall_clock_seconds,
        "slices": slices,
    });
    let text = serde_json::to_string_pretty(&doc).context("serializing outcomes doc")?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Derive the project root from an intent-card path. Assumes the convention
/// `<project>/agent/intent-card.json`; rejects anything else so the
/// `loop_runner`'s `project.join("agent/intent-card.json")` lookup
/// resolves to the same file the manifest pointed at.
fn derive_project_dir(intent_card_path: &Path) -> Result<PathBuf> {
    let parent = intent_card_path.parent().ok_or_else(|| {
        anyhow!(
            "intent_card {} has no parent directory",
            intent_card_path.display()
        )
    })?;
    if parent.file_name().and_then(|n| n.to_str()) != Some("agent") {
        return Err(anyhow!(
            "intent_card must live at <project>/agent/intent-card.json; got parent {}",
            parent.display()
        ));
    }
    let project = parent.parent().ok_or_else(|| {
        anyhow!(
            "intent_card {} parent has no grandparent (cannot derive project root)",
            intent_card_path.display()
        )
    })?;
    Ok(project.to_path_buf())
}

fn git_head_sha(project: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project)
        .output()
        .context("spawning `git rev-parse HEAD`")?;
    if !out.status.success() {
        return Err(anyhow!(
            "git rev-parse HEAD in {} failed: {}",
            project.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Read the per-iteration verdict by parsing the receipt the `loop_runner`
/// just wrote at `<project>/target/autobuilder/receipts/<sha>.json`.
/// Falls back to `"unknown"` if the receipt is missing or malformed —
/// the caller chooses how to react.
fn read_iteration_verdict(project: &Path, head_sha: &str) -> Result<String> {
    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(format!("{head_sha}.json"));
    let text = fs::read_to_string(&receipt_path)
        .with_context(|| format!("reading iteration receipt {}", receipt_path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).context("iteration receipt is not valid JSON")?;
    Ok(value
        .get("verdict")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_owned())
}

/// Apply the slice's transition policy to its project's git state.
///
/// - `reset`: hard-reset to the slice's baseline SHA. Destructive — any
///   iteration commits and any working-tree changes are discarded.
/// - `advance-commit`: no-op. The loop is expected to have committed every
///   advance iteration; the slice's natural end-state is HEAD-as-is.
/// - `continue`: no-op. Debug-mode pass-through.
pub(crate) fn apply_transition(
    project: &Path,
    policy: &str,
    baseline_sha: &str,
) -> Result<()> {
    match policy {
        "reset" => {
            let out = Command::new("git")
                .args(["reset", "--hard", baseline_sha])
                .current_dir(project)
                .output()
                .context("spawning `git reset --hard`")?;
            if !out.status.success() {
                return Err(anyhow!(
                    "git reset --hard {baseline_sha} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Ok(())
        }
        "advance-commit" | "continue" => Ok(()),
        other => Err(anyhow!(
            "unknown transition policy `{other}`; expected one of {TRANSITIONS:?}"
        )),
    }
}

/// Validate an `experiment.toml` against `autobuilder.experiment_manifest.v1`.
///
/// On failure, prints each error to stderr (one per line with a TOML-path
/// prefix) and returns `Err`. On success returns `(campaign_slug,
/// slice_count)` so callers can log a one-line summary.
#[allow(clippy::too_many_lines)] // single linear schema-validation pipeline; mirrors intake::validate_file
pub(crate) fn validate_file(path: &Path) -> Result<(String, usize)> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("missing experiment manifest at {}", path.display()))?;
    let value: Value = text
        .parse::<Value>()
        .context("experiment manifest is not valid TOML")?;

    let mut errs: Vec<String> = Vec::new();
    let Some(table) = value.as_table() else {
        return Err(anyhow!(
            "experiment manifest must be a TOML table, got {}",
            kind(&value)
        ));
    };

    for f in REQUIRED_TOP {
        if !table.contains_key(*f) {
            errs.push(format!("/: missing required field `{f}`"));
        }
    }
    check_additional_properties(table.keys(), ALLOWED_TOP, "/", &mut errs);

    if let Some(s) = table.get("schema") {
        match s.as_str() {
            Some(v) if v == SCHEMA_ID => {}
            Some(other) => errs.push(format!(
                "/schema: expected \"{SCHEMA_ID}\", got \"{other}\""
            )),
            None => errs.push(format!("/schema: expected string, got {}", kind(s))),
        }
    }

    if let Some(c) = table.get("campaign") {
        check_campaign(c, &mut errs);
    }

    if let Some(e) = table.get("edit_agent") {
        check_edit_agent(e, &mut errs);
    }

    let manifest_dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let slice_count = if let Some(s) = table.get("slices") {
        check_slices(s, manifest_dir, &mut errs)
    } else {
        0
    };

    if !errs.is_empty() {
        eprintln!(
            "experiment validate: {} schema violation(s) in {}",
            errs.len(),
            path.display()
        );
        for line in &errs {
            eprintln!("  {line}");
        }
        return Err(anyhow!(
            "experiment manifest failed {SCHEMA_ID} validation"
        ));
    }

    let slug = value
        .get("campaign")
        .and_then(|c| c.get("slug"))
        .and_then(Value::as_str)
        .unwrap_or("(no slug)")
        .to_owned();
    Ok((slug, slice_count))
}

fn check_campaign(v: &Value, errs: &mut Vec<String>) {
    let Some(t) = v.as_table() else {
        errs.push(format!("/campaign: expected table, got {}", kind(v)));
        return;
    };
    for f in CAMPAIGN_REQUIRED {
        if !t.contains_key(*f) {
            errs.push(format!("/campaign: missing required `{f}`"));
        }
    }
    check_additional_properties(t.keys(), CAMPAIGN_ALLOWED, "/campaign", errs);

    if let Some(s) = t.get("slug") {
        match s.as_str() {
            Some(slug) => {
                if slug.is_empty() || slug.len() > 63 {
                    errs.push(format!(
                        "/campaign/slug: length {} not in [1, 63]",
                        slug.len()
                    ));
                }
            }
            None => errs.push(format!("/campaign/slug: expected string, got {}", kind(s))),
        }
    }
    if let Some(p) = t.get("prd_source") {
        if !p.is_str() {
            errs.push(format!(
                "/campaign/prd_source: expected string, got {}",
                kind(p)
            ));
        }
    }
    check_positive_int(t.get("max_wall_clock_minutes"), "/campaign/max_wall_clock_minutes", errs);
    check_positive_int(t.get("max_total_iterations"), "/campaign/max_total_iterations", errs);
}

fn check_edit_agent(v: &Value, errs: &mut Vec<String>) {
    let Some(t) = v.as_table() else {
        errs.push(format!("/edit_agent: expected table, got {}", kind(v)));
        return;
    };
    for f in EDIT_AGENT_REQUIRED {
        if !t.contains_key(*f) {
            errs.push(format!("/edit_agent: missing required `{f}`"));
        }
    }
    check_additional_properties(t.keys(), EDIT_AGENT_ALLOWED, "/edit_agent", errs);

    if let Some(m) = t.get("model") {
        match m.as_str() {
            Some(model) if KNOWN_MODELS.contains(&model) => {}
            Some(model) => errs.push(format!(
                "/edit_agent/model: must be one of {KNOWN_MODELS:?}, got \"{model}\""
            )),
            None => errs.push(format!("/edit_agent/model: expected string, got {}", kind(m))),
        }
    }
    if let Some(k) = t.get("api_key_env") {
        if !k.is_str() {
            errs.push(format!(
                "/edit_agent/api_key_env: expected string, got {}",
                kind(k)
            ));
        }
    }
    check_positive_int(t.get("max_tokens_per_call"), "/edit_agent/max_tokens_per_call", errs);
    if let Some(f) = t.get("fallback_to_signal_mode") {
        if !f.is_bool() {
            errs.push(format!(
                "/edit_agent/fallback_to_signal_mode: expected bool, got {}",
                kind(f)
            ));
        }
    }
}

fn check_slices(v: &Value, manifest_dir: Option<&Path>, errs: &mut Vec<String>) -> usize {
    let Some(arr) = v.as_array() else {
        errs.push(format!("/slices: expected array, got {}", kind(v)));
        return 0;
    };
    if arr.is_empty() {
        errs.push("/slices: must have at least 1 item".to_owned());
    }
    for (i, slice) in arr.iter().enumerate() {
        let prefix = format!("/slices/{i}");
        let Some(t) = slice.as_table() else {
            errs.push(format!("{prefix}: expected table, got {}", kind(slice)));
            continue;
        };
        for f in SLICE_REQUIRED {
            if !t.contains_key(*f) {
                errs.push(format!("{prefix}: missing required `{f}`"));
            }
        }
        check_additional_properties(t.keys(), SLICE_ALLOWED, &prefix, errs);

        if let Some(id) = t.get("id") {
            match id.as_str() {
                Some(s) if s.starts_with('S') && s[1..].chars().all(|c| c.is_ascii_digit()) && s.len() >= 2 => {}
                Some(s) => errs.push(format!(
                    "{prefix}/id: must match ^S[0-9]+$, got \"{s}\""
                )),
                None => errs.push(format!("{prefix}/id: expected string, got {}", kind(id))),
            }
        }

        if let Some(ic) = t.get("intent_card") {
            match ic.as_str() {
                Some(rel) => {
                    let resolved = manifest_dir.map_or_else(|| PathBuf::from(rel), |d| d.join(rel));
                    if !resolved.exists() {
                        errs.push(format!(
                            "{prefix}/intent_card: file not found at {}",
                            resolved.display()
                        ));
                    } else if let Err(e) = intake::validate_file(&resolved) {
                        errs.push(format!(
                            "{prefix}/intent_card: failed intent_card.v1 validation: {e}"
                        ));
                    }
                }
                None => errs.push(format!("{prefix}/intent_card: expected string, got {}", kind(ic))),
            }
        }

        if let Some(mi) = t.get("max_iterations") {
            check_positive_int(Some(mi), &format!("{prefix}/max_iterations"), errs);
        }

        if let Some(tr) = t.get("transition") {
            match tr.as_str() {
                Some(s) if TRANSITIONS.contains(&s) => {}
                Some(s) => errs.push(format!(
                    "{prefix}/transition: must be one of {TRANSITIONS:?}, got \"{s}\""
                )),
                None => errs.push(format!("{prefix}/transition: expected string, got {}", kind(tr))),
            }
        }
    }
    arr.len()
}

fn check_positive_int(v: Option<&Value>, path: &str, errs: &mut Vec<String>) {
    let Some(v) = v else { return };
    match v.as_integer() {
        Some(n) if n >= 1 => {}
        Some(n) => errs.push(format!("{path}: must be ≥ 1, got {n}")),
        None => errs.push(format!("{path}: expected integer, got {}", kind(v))),
    }
}

fn check_additional_properties<'a, I: Iterator<Item = &'a String>>(
    keys: I,
    allowed: &[&str],
    path: &str,
    errs: &mut Vec<String>,
) {
    for key in keys {
        if !allowed.contains(&key.as_str()) {
            errs.push(format!("{path}: additional property `{key}` is not allowed"));
        }
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        let identity = [
            ("user.email", "test@example.com"),
            ("user.name", "Test"),
        ];
        run_git(p, &["init", "-q"]);
        for (k, v) in identity {
            run_git(p, &["config", k, v]);
        }
        std::fs::write(p.join("README"), "a").unwrap();
        run_git(p, &["add", "README"]);
        run_git(p, &["commit", "-q", "-m", "baseline"]);
        let baseline = git_head_sha(p).unwrap();
        (dir, baseline)
    }

    fn run_git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn apply_transition_reset_rewinds_to_baseline() {
        let (dir, baseline) = init_repo();
        let p = dir.path();
        // Add a second commit; HEAD moves forward.
        std::fs::write(p.join("README"), "b").unwrap();
        run_git(p, &["add", "README"]);
        run_git(p, &["commit", "-q", "-m", "second"]);
        let advanced = git_head_sha(p).unwrap();
        assert_ne!(advanced, baseline);

        apply_transition(p, "reset", &baseline).unwrap();
        assert_eq!(git_head_sha(p).unwrap(), baseline);
    }

    #[test]
    fn apply_transition_advance_commit_is_noop() {
        let (dir, baseline) = init_repo();
        let p = dir.path();
        std::fs::write(p.join("README"), "b").unwrap();
        run_git(p, &["add", "README"]);
        run_git(p, &["commit", "-q", "-m", "second"]);
        let advanced = git_head_sha(p).unwrap();
        apply_transition(p, "advance-commit", &baseline).unwrap();
        assert_eq!(git_head_sha(p).unwrap(), advanced);
    }

    #[test]
    fn apply_transition_continue_is_noop() {
        let (dir, baseline) = init_repo();
        let p = dir.path();
        std::fs::write(p.join("README"), "b").unwrap();
        run_git(p, &["add", "README"]);
        run_git(p, &["commit", "-q", "-m", "second"]);
        let advanced = git_head_sha(p).unwrap();
        apply_transition(p, "continue", &baseline).unwrap();
        assert_eq!(git_head_sha(p).unwrap(), advanced);
    }

    #[test]
    fn apply_transition_unknown_policy_errors() {
        let (dir, baseline) = init_repo();
        let err = apply_transition(dir.path(), "rebase-and-pray", &baseline).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("rebase-and-pray") && msg.contains("expected one of"),
            "expected unknown-policy error, got: {msg}"
        );
    }

    #[test]
    fn derive_project_dir_accepts_canonical_layout() {
        let tmp = TempDir::new().unwrap();
        let card = tmp.path().join("agent").join("intent-card.json");
        let project = derive_project_dir(&card).unwrap();
        assert_eq!(project, tmp.path());
    }

    #[test]
    fn derive_project_dir_rejects_wrong_parent() {
        let tmp = TempDir::new().unwrap();
        let card = tmp.path().join("not-agent").join("intent-card.json");
        let err = derive_project_dir(&card).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("intent_card must live at <project>/agent/intent-card.json"),
            "expected canonical-layout error, got: {msg}"
        );
    }
}
