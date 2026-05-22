//! Metric harness wrapper. Runs `<project>/scripts/run-metrics.sh`,
//! validates the produced `target/autobuilder/metrics.json` against the
//! `autobuilder.metrics.v1` schema, and re-emits a normalized copy to
//! stdout. Returns non-zero on missing harness, missing/invalid metrics
//! doc, or schema violation.
//!
//! This subcommand is the per-project equivalent of `autobuilder loop`'s
//! internal metric step — useful for one-shot inspection without writing
//! a receipt.

use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project directory containing scripts/run-metrics.sh.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Skip the harness invocation and just re-validate an existing metrics.json.
    #[arg(long)]
    pub no_run: bool,

    /// Pretty-print the normalized output (two-space indent).
    #[arg(long)]
    pub pretty: bool,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    if !args.no_run {
        let script = project.join("scripts/run-metrics.sh");
        if !script.is_file() {
            return Err(anyhow!("missing harness at {}", script.display()));
        }
        let status = Command::new("bash")
            .arg(&script)
            .current_dir(&project)
            .status()
            .with_context(|| format!("failed to spawn {}", script.display()))?;
        // The harness itself decides exit status; we propagate via the
        // metrics.json verdict below rather than gating on shell exit, so
        // a failing iteration still produces a usable receipt.
        eprintln!("metric-harness: scripts/run-metrics.sh exited {}", status.code().unwrap_or(-1));
    }

    let metrics_path = project.join("target/autobuilder/metrics.json");
    let text = fs::read_to_string(&metrics_path)
        .with_context(|| format!("missing {}", metrics_path.display()))?;
    let value: Value =
        serde_json::from_str(&text).context("metrics.json is not valid JSON")?;

    let errs = validate_metrics_v1(&value);
    if !errs.is_empty() {
        eprintln!(
            "metric-harness: {} schema violation(s) in {}",
            errs.len(),
            metrics_path.display()
        );
        for (path, msg) in &errs {
            eprintln!("  {path}: {msg}");
        }
        return Err(anyhow!(
            "metrics.json failed autobuilder.metrics.v1 validation"
        ));
    }

    let bytes = if args.pretty {
        serde_json::to_vec_pretty(&value)?
    } else {
        serde_json::to_vec(&value)?
    };
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&bytes)?;
    handle.write_all(b"\n")?;
    Ok(())
}

fn validate_metrics_v1(v: &Value) -> Vec<(String, String)> {
    let mut errs: Vec<(String, String)> = Vec::new();
    let Some(obj) = v.as_object() else {
        errs.push(("/".into(), "expected object at root".into()));
        return errs;
    };

    if obj.get("schema").and_then(Value::as_str) != Some("autobuilder.metrics.v1") {
        errs.push((
            "/schema".into(),
            "expected \"autobuilder.metrics.v1\"".into(),
        ));
    }
    for f in [
        "head_sha",
        "scalars",
        "ac_passing_count",
        "ac_total_count",
        "audit",
        "clippy_warning_count",
        "captured_at",
    ] {
        if !obj.contains_key(f) {
            errs.push((format!("/{f}"), "missing required field".into()));
        }
    }
    if let Some(s) = obj.get("scalars") {
        if !s.is_object() {
            errs.push(("/scalars".into(), "expected object".into()));
        } else if s.as_object().is_some_and(serde_json::Map::is_empty) {
            errs.push((
                "/scalars".into(),
                "must contain at least one entry (the unfakeable metric)".into(),
            ));
        }
    }
    for f in ["ac_passing_count", "ac_total_count", "clippy_warning_count"] {
        if let Some(n) = obj.get(f) {
            if !n.is_i64() && !n.is_u64() {
                errs.push((format!("/{f}"), "expected integer".into()));
            }
        }
    }
    if let Some(audit) = obj.get("audit") {
        if let Some(a) = audit.as_object() {
            for f in ["blocking_count", "advisory_count"] {
                match a.get(f) {
                    Some(n) if n.is_i64() || n.is_u64() => {}
                    Some(_) => errs.push((format!("/audit/{f}"), "expected integer".into())),
                    None => errs.push((format!("/audit/{f}"), "missing".into())),
                }
            }
        } else {
            errs.push(("/audit".into(), "expected object".into()));
        }
    }
    if let Some(h) = obj.get("head_sha") {
        if !h.is_string() {
            errs.push(("/head_sha".into(), "expected string".into()));
        }
    }
    if let Some(t) = obj.get("captured_at") {
        if !t.is_string() {
            errs.push(("/captured_at".into(), "expected string".into()));
        }
    }
    errs
}
