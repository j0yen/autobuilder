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

use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    let card = read_intent_card(&project)?;
    let metric_name = card.unfakeable_metric.name;
    let lower_is_better = card.unfakeable_metric.lower_is_better;

    let script_exit = run_harness(&project)?;

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

fn read_previous_metric(results_tsv: &Path) -> Result<Option<f64>> {
    if !results_tsv.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(results_tsv)?;
    let last_data_line = content
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with("commit\t"))
        .next_back();
    let Some(line) = last_data_line else {
        return Ok(None);
    };
    let mut cols = line.split('\t');
    let _commit = cols.next();
    let metric_str = cols.next().ok_or_else(|| {
        anyhow!("results.tsv last row malformed: no metric column in `{line}`")
    })?;
    let metric: f64 = metric_str
        .parse()
        .with_context(|| format!("results.tsv metric column not numeric: {metric_str}"))?;
    Ok(Some(metric))
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
    let captured_at = now_rfc3339()?;
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
    let mut value = serde_json::to_value(&doc)?;
    set_digest(&mut value)?;

    let receipts_dir = project.join("target/autobuilder/receipts");
    fs::create_dir_all(&receipts_dir)?;
    let receipt_path = receipts_dir.join(format!("{}.json", r.head_sha));
    let bytes = serde_json::to_vec_pretty(&value)?;
    fs::write(&receipt_path, bytes)?;
    Ok(())
}

fn set_digest(value: &mut serde_json::Value) -> Result<()> {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "receipt_digest".into(),
            serde_json::Value::String(String::new()),
        );
    }
    let canonical = canonical_json_bytes(value);
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let obj = value
        .as_object_mut()
        .ok_or_else(|| anyhow!("receipt is not a JSON object"))?;
    obj.insert("receipt_digest".into(), serde_json::Value::String(digest));
    Ok(())
}

fn canonical_json_bytes(value: &serde_json::Value) -> Vec<u8> {
    let sorted = sort_keys(value);
    serde_json::to_vec(&sorted).unwrap_or_default()
}

fn now_rfc3339() -> Result<String> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs();
    Ok(secs_to_rfc3339(secs))
}

fn secs_to_rfc3339(secs: u64) -> String {
    let day = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;
    // i64 fits all plausible day counts (u64/86400 ≤ 2^52); narrow safely.
    let days = i64::try_from(day).unwrap_or(0);
    let (year, month, mday) = civil_from_days(days);
    format!("{year:04}-{month:02}-{mday:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Howard Hinnant's `civil_from_days` (public domain).
// Converts days-since-1970-01-01 to (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let day_of_year = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let d = u32::try_from(day_of_year - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

fn sort_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = map.get(k) {
                    sorted.insert(k.clone(), sort_keys(v));
                }
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_keys).collect())
        }
        other => other.clone(),
    }
}
