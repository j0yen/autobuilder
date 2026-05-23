//! Stage 3 — single-iteration runner of the iterate-and-prove LOOP.
//!
//! Runs one iteration: invoke the project's `scripts/run-metrics.sh`,
//! parse the emitted `autobuilder.metrics.v1` doc, compare the unfakeable
//! scalar against the previous iteration's value (from `results.tsv`),
//! append a row, write a per-iteration receipt JSON with a sha256 digest,
//! and print a verdict. The caller — a Claude session running the
//! autoresearch-style outer LOOP — is responsible for making the `src/`
//! edits, committing, and acting on the verdict (advance / revert /
//! crash). Keeping the metric/receipt path in Rust keeps shell out of the
//! trust chain (PLAN.md, root motivation).
//!
//! Multi-iteration orchestration, edit-agent invocation, `FailureCapsule`
//! emission, proof-lane routing, and the 7-receipt risk gate are deferred
//! to follow-up work — this is the minimum that makes the metric pipeline
//! load-bearing.

use crate::receipt;
use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use session_trace_receipt as session_trace_lib;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing agent/intent-card.json and scripts/run-metrics.sh.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Iteration number (0 = baseline).
    #[arg(long)]
    pub iteration: u32,

    /// HEAD sha for this iteration; caller supplies (e.g. `git rev-parse HEAD`).
    #[arg(long)]
    pub head_sha: String,

    /// Short description for the results.tsv row.
    #[arg(long, default_value = "")]
    pub description: String,

    /// Wrap the metric-harness spawn with ctrace start/stop and emit a
    /// session-trace receipt at target/autobuilder/receipts/session-trace.json.
    /// Gracefully degrades to verdict=skipped when the ctrace binary is
    /// missing or the start command fails; never aborts the iteration.
    #[arg(long, default_value_t = false)]
    pub trace: bool,

    /// Path or binary name for the ctrace tracer. Defaults to `ctrace` on
    /// PATH; override for non-standard installs.
    #[arg(long, default_value = "ctrace")]
    pub trace_binary: String,
}

#[derive(Debug, Deserialize)]
struct IntentCard {
    unfakeable_metric: UnfakeableMetric,
}

#[derive(Debug, Deserialize)]
struct UnfakeableMetric {
    name: String,
    lower_is_better: bool,
}

#[derive(Debug, Deserialize)]
struct Metrics {
    schema: String,
    #[serde(default)]
    scalars: serde_json::Value,
    #[serde(default)]
    ac_passing_count: i64,
    #[serde(default)]
    ac_total_count: i64,
    #[serde(default)]
    audit: AuditCounts,
    #[serde(default)]
    clippy_warning_count: i64,
}

#[derive(Debug, Default, Deserialize)]
struct AuditCounts {
    #[serde(default)]
    blocking_count: i64,
    #[serde(default)]
    advisory_count: i64,
}

#[derive(Debug, Copy, Clone)]
enum Verdict {
    Baseline,
    Advance,
    Revert,
    Crash,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Advance => "advance",
            Self::Revert => "revert",
            Self::Crash => "crash",
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // single linear iteration pipeline; splitting hides the flow
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let card = read_intent_card(&project)?;
    let metric_name = card.unfakeable_metric.name.clone();
    let lower_is_better = card.unfakeable_metric.lower_is_better;

    // Trace session bracketing the harness spawn. If args.trace is false
    // or the tracer can't start, this is a no-op and we never write a
    // session-trace receipt (gate handles the "missing receipt" case
    // separately via skipped verdict in the receipt or absence of the
    // receipt entirely).
    let mut trace_handle: Option<TraceHandle> = None;
    if args.trace {
        trace_handle = start_trace(&project, &args.trace_binary);
    }

    let script_exit = run_harness(&project)?;

    if let Some(handle) = trace_handle.take() {
        let receipt = finalize_trace(handle, &project, &args, &card)?;
        let receipts_dir = project.join("target/autobuilder/receipts");
        fs::create_dir_all(&receipts_dir).ok();
        let path = receipts_dir.join("session-trace.json");
        let value = serde_json::to_value(&receipt)
            .context("serializing session-trace receipt")?;
        receipt::write(&path, value)
            .with_context(|| format!("writing {}", path.display()))?;
    } else if args.trace {
        // ctrace was requested but never started — emit a skipped receipt
        // so the gate sees a fresh receipt at this HEAD rather than a
        // stale one from a previous run.
        let receipts_dir = project.join("target/autobuilder/receipts");
        fs::create_dir_all(&receipts_dir).ok();
        let receipt = session_trace_lib::skipped_receipt(
            args.head_sha.clone(),
            receipt::now_rfc3339()?,
            "tracer_unavailable: ctrace binary missing or start failed",
        );
        let path = receipts_dir.join("session-trace.json");
        let value = serde_json::to_value(&receipt)
            .context("serializing session-trace skipped receipt")?;
        receipt::write(&path, value)
            .with_context(|| format!("writing {}", path.display()))?;
    }

    let metrics_path = project.join("target/autobuilder/metrics.json");
    let metrics_text = fs::read_to_string(&metrics_path).with_context(|| {
        format!("missing {} after run-metrics.sh", metrics_path.display())
    })?;
    let metrics: Metrics =
        serde_json::from_str(&metrics_text).context("invalid metrics.json")?;
    if metrics.schema != "autobuilder.metrics.v1" {
        return Err(anyhow!(
            "metrics.json schema mismatch: got {} expected autobuilder.metrics.v1",
            metrics.schema
        ));
    }

    let current_metric = extract_scalar(&metrics.scalars, &metric_name).ok_or_else(|| {
        anyhow!("scalars.{metric_name} missing or non-numeric in metrics.json")
    })?;

    let results_tsv = project.join("target/autobuilder/results.tsv");
    let previous = read_previous_metric(&results_tsv)?;

    let verdict = decide_verdict(VerdictInputs {
        iteration: args.iteration,
        script_exit,
        blocking_audit: metrics.audit.blocking_count,
        current: current_metric,
        previous,
        lower_is_better,
    });

    append_results_row(
        &results_tsv,
        ResultsRow {
            head_sha: &args.head_sha,
            iteration: args.iteration,
            metric: current_metric,
            ac_pass: metrics.ac_passing_count,
            ac_total: metrics.ac_total_count,
            verdict: verdict.as_str(),
            description: &args.description,
        },
    )?;

    write_receipt(
        &project,
        Receipt {
            head_sha: &args.head_sha,
            iteration: args.iteration,
            metric_name: &metric_name,
            metric_value: current_metric,
            previous_metric_value: previous,
            verdict: verdict.as_str(),
            ac_passing_count: metrics.ac_passing_count,
            ac_total_count: metrics.ac_total_count,
            blocking_count: metrics.audit.blocking_count,
            advisory_count: metrics.audit.advisory_count,
            clippy_warning_count: metrics.clippy_warning_count,
        },
    )?;

    println!(
        "iter={} head={} {}={current_metric} verdict={}",
        args.iteration,
        args.head_sha,
        metric_name,
        verdict.as_str()
    );

    Ok(())
}

fn read_intent_card(project: &Path) -> Result<IntentCard> {
    let path = project.join("agent/intent-card.json");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("missing intent-card at {}", path.display()))?;
    let card: IntentCard =
        serde_json::from_str(&text).context("invalid intent-card.json")?;
    Ok(card)
}

fn run_harness(project: &Path) -> Result<i32> {
    let script = project.join("scripts/run-metrics.sh");
    if !script.is_file() {
        return Err(anyhow!(
            "missing scripts/run-metrics.sh at {}",
            script.display()
        ));
    }
    let status = Command::new("bash")
        .arg(&script)
        .current_dir(project)
        .status()
        .with_context(|| format!("failed to spawn {}", script.display()))?;
    Ok(status.code().unwrap_or(-1))
}

fn extract_scalar(scalars: &serde_json::Value, name: &str) -> Option<f64> {
    let v = scalars.get(name)?;
    v.as_f64().or_else(|| {
        v.as_i64().map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let f = i as f64;
            f
        })
    })
}

/// Most recent non-crash metric from results.tsv. Crashed iterations are
/// excluded because their metric reflects an aborted state — using it as
/// `previous` makes a clean re-run of the same SHA look like a no-op
/// advance/revert decision (`previous == current`) when the producer is
/// actually back in business.
fn read_previous_metric(results_tsv: &Path) -> Result<Option<f64>> {
    if !results_tsv.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(results_tsv)?;
    let mut lines = content.lines();
    let Some(header) = lines.next() else {
        return Ok(None);
    };
    // Bind column lookups to the header row so the schema can grow without
    // silently breaking the crash filter (`status` column index drifts).
    // `append_results_row` writes this header — keep them in sync.
    let cols_in_header: Vec<&str> = header.split('\t').collect();
    let metric_idx = cols_in_header
        .iter()
        .position(|c| *c == "metric")
        .ok_or_else(|| anyhow!("results.tsv header missing `metric` column"))?;
    let status_idx = cols_in_header
        .iter()
        .position(|c| *c == "status")
        .ok_or_else(|| anyhow!("results.tsv header missing `status` column"))?;

    let last_data_line = lines
        .filter(|l| !l.is_empty())
        .filter(|l| !is_crash_row(l, status_idx))
        .next_back();
    let Some(line) = last_data_line else {
        return Ok(None);
    };
    let cols: Vec<&str> = line.split('\t').collect();
    let metric_str = cols.get(metric_idx).ok_or_else(|| {
        anyhow!(
            "results.tsv row has fewer columns than header (need {metric_idx}+1): `{line}`"
        )
    })?;
    let metric: f64 = metric_str
        .parse()
        .with_context(|| format!("results.tsv metric column not numeric: {metric_str}"))?;
    Ok(Some(metric))
}

fn is_crash_row(line: &str, status_idx: usize) -> bool {
    line.split('\t').nth(status_idx) == Some("crash")
}

struct VerdictInputs {
    iteration: u32,
    script_exit: i32,
    blocking_audit: i64,
    current: f64,
    previous: Option<f64>,
    lower_is_better: bool,
}

#[allow(clippy::needless_pass_by_value)] // `VerdictInputs` is plain-data; by-value reads cleaner at the call site
fn decide_verdict(i: VerdictInputs) -> Verdict {
    if i.blocking_audit > 0 {
        return Verdict::Crash;
    }
    // If the project script aborted and we got no prior baseline to compare,
    // call it a crash. (If we DO have a prior baseline, we still got metrics
    // back — script_exit != 0 is fine; the loop sees the metric, not the
    // exit code, to decide advance/revert.)
    if i.script_exit != 0 && i.previous.is_none() {
        return Verdict::Crash;
    }
    if i.iteration == 0 {
        return Verdict::Baseline;
    }
    let Some(prev) = i.previous else {
        return Verdict::Baseline;
    };
    let improved = if i.lower_is_better {
        i.current < prev
    } else {
        i.current > prev
    };
    if improved {
        Verdict::Advance
    } else {
        Verdict::Revert
    }
}

struct ResultsRow<'a> {
    head_sha: &'a str,
    iteration: u32,
    metric: f64,
    ac_pass: i64,
    ac_total: i64,
    verdict: &'a str,
    description: &'a str,
}

#[allow(clippy::needless_pass_by_value)] // `ResultsRow<'_>` is cheap; by-value reads cleaner at the call site
fn append_results_row(path: &Path, row: ResultsRow<'_>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let need_header = !path.is_file();
    let short = row
        .head_sha
        .get(..7.min(row.head_sha.len()))
        .unwrap_or(row.head_sha);
    let safe_desc: String = row
        .description
        .chars()
        .map(|c| if c == '\t' || c == '\n' { ' ' } else { c })
        .collect();
    let line = format!(
        "{short}\t{metric}\t{iter}\t{pass}\t{total}\t{verdict}\t{desc}\n",
        metric = row.metric,
        iter = row.iteration,
        pass = row.ac_pass,
        total = row.ac_total,
        verdict = row.verdict,
        desc = safe_desc,
    );
    let mut content = if need_header {
        String::from("commit\tmetric\titeration\tac_pass\tac_total\tstatus\tdescription\n")
    } else {
        fs::read_to_string(path)?
    };
    content.push_str(&line);
    fs::write(path, content)?;
    Ok(())
}

struct Receipt<'a> {
    head_sha: &'a str,
    iteration: u32,
    metric_name: &'a str,
    metric_value: f64,
    previous_metric_value: Option<f64>,
    verdict: &'a str,
    ac_passing_count: i64,
    ac_total_count: i64,
    blocking_count: i64,
    advisory_count: i64,
    clippy_warning_count: i64,
}

#[derive(Serialize)]
struct ReceiptDoc<'a> {
    schema: &'static str,
    head_sha: &'a str,
    iteration: u32,
    metric_name: &'a str,
    metric_value: f64,
    previous_metric_value: Option<f64>,
    verdict: &'a str,
    ac_passing_count: i64,
    ac_total_count: i64,
    audit: AuditDoc,
    clippy_warning_count: i64,
    captured_at: String,
    receipt_digest: String,
}

#[derive(Serialize)]
struct AuditDoc {
    blocking_count: i64,
    advisory_count: i64,
}

#[allow(clippy::needless_pass_by_value)] // `Receipt<'_>` is cheap; by-value reads cleaner at the call site
fn write_receipt(project: &Path, r: Receipt<'_>) -> Result<()> {
    let captured_at = receipt::now_rfc3339()?;
    let doc = ReceiptDoc {
        schema: "autobuilder.iteration_receipt.v1",
        head_sha: r.head_sha,
        iteration: r.iteration,
        metric_name: r.metric_name,
        metric_value: r.metric_value,
        previous_metric_value: r.previous_metric_value,
        verdict: r.verdict,
        ac_passing_count: r.ac_passing_count,
        ac_total_count: r.ac_total_count,
        audit: AuditDoc {
            blocking_count: r.blocking_count,
            advisory_count: r.advisory_count,
        },
        clippy_warning_count: r.clippy_warning_count,
        captured_at,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project
        .join("target/autobuilder/receipts")
        .join(format!("{}.json", r.head_sha));
    receipt::write(&receipt_path, value)
}

/// Live state of a ctrace session: the absolute path of the NDJSON log
/// (so the post-harness teardown knows where to read from) and the
/// tracer binary name (so stop can use the same binary that started).
struct TraceHandle {
    log_path: PathBuf,
    binary: String,
}

/// Attempt to spawn `<binary> start --root <our_pid> --log <log>`. Returns
/// None on any failure path (binary missing, sudo denied, bpftrace
/// missing) so the caller can emit a skipped receipt instead of crashing
/// the iteration. The PRD's degraded-gracefully rule.
fn start_trace(project: &Path, binary: &str) -> Option<TraceHandle> {
    let log = project.join("target/autobuilder/session-trace.ndjson");
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent).ok();
    }
    let pid = std::process::id().to_string();
    let output = Command::new(binary)
        .args(["start", "--root", pid.as_str(), "--log"])
        .arg(&log)
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "trace start failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    Some(TraceHandle {
        log_path: log,
        binary: binary.to_owned(),
    })
}

/// Stop the tracer and read the captured NDJSON log. Evaluates against
/// the intent-card's `hard_constraints` (via
/// `session_trace_lib::HardConstraints::from_intent_card`) and returns a
/// populated [`session_trace_lib::SessionTraceReceipt`].
#[allow(clippy::needless_pass_by_value)] // handle is consumed (we tear down its log path); explicit by-value reads cleaner
fn finalize_trace(
    handle: TraceHandle,
    project: &Path,
    args: &Args,
    _card: &IntentCard,
) -> Result<session_trace_lib::SessionTraceReceipt> {
    // Best-effort stop; if it fails, we still try to read whatever log
    // exists. The receipt's log_sha256 captures whatever we ended up with.
    let _ = Command::new(&handle.binary).arg("stop").output();
    let intent_card_path = project.join("agent/intent-card.json");
    let intent_card_text = fs::read_to_string(&intent_card_path)
        .with_context(|| format!("re-reading {}", intent_card_path.display()))?;
    let intent_card_value: serde_json::Value =
        serde_json::from_str(&intent_card_text).context("intent-card.json is not valid JSON")?;
    let constraints = session_trace_lib::HardConstraints::from_intent_card(&intent_card_value);
    let tracer = session_trace_lib::tracer_info_from_log(&handle.log_path)
        .with_context(|| format!("reading trace log {}", handle.log_path.display()))?;
    let log_text = fs::read_to_string(&handle.log_path).unwrap_or_default();
    let events = session_trace_lib::parse_ndjson(&log_text);
    let evaluation = session_trace_lib::evaluate(&events, &constraints);
    let receipt = session_trace_lib::build_receipt(
        args.head_sha.clone(),
        receipt::now_rfc3339()?,
        tracer,
        evaluation,
        Vec::new(),
    );
    Ok(receipt)
}
