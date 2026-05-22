//! autobuilder-metric-harness — normalize and digest the metrics produced by a
//! project's `scripts/run-metrics.sh`.
//!
//! Contract is fixed by the acceptance tests in `tests/acceptance_ac*.rs`. See
//! `agent/intent-card.json` for the AC list and the unfakeable-metric name.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "autobuilder.metrics.v1";
const DEFAULT_TIMEOUT_SECS: u64 = 600;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// CLI arguments — see `agent/intent-card.json` for the locked contract.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Path to the project root containing `scripts/run-metrics.sh`.
    project_path: PathBuf,

    /// HEAD SHA recorded in the emitted metrics document.
    #[arg(long, default_value = "unknown")]
    head_sha: String,

    /// Iteration number (defaults to null when absent).
    #[arg(long)]
    iteration: Option<i64>,

    /// Kill the project script after this many seconds.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout_seconds: u64,

    /// Pretty-print the JSON document on stdout.
    #[arg(long)]
    pretty: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(code) => code,
        Err(err) => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "autobuilder-metric-harness: {err}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &Args) -> io::Result<ExitCode> {
    let script_path = args.project_path.join("scripts/run-metrics.sh");

    if !is_executable_file(&script_path) {
        let mut stderr = io::stderr().lock();
        let _ = writeln!(
            stderr,
            "autobuilder-metric-harness: scripts/run-metrics.sh missing or not executable at {}",
            script_path.display(),
        );
        return Ok(ExitCode::from(2));
    }

    let out_dir = args.project_path.join("target/autobuilder");
    fs::create_dir_all(&out_dir)?;
    let log_path = out_dir.join("run.log");
    let metrics_path = out_dir.join("metrics.json");

    // Pre-clear so we can distinguish "script wrote nothing" from a stale file.
    if metrics_path.exists() {
        let _ = fs::remove_file(&metrics_path);
    }

    let (script_exit, timed_out) =
        run_script(&script_path, &args.project_path, &log_path, args.timeout_seconds)?;

    if timed_out {
        let doc = synthetic_doc(args, "timeout");
        write_doc(&doc, args.pretty, &metrics_path)?;
        return Ok(ExitCode::from(1));
    }

    let raw = match fs::read_to_string(&metrics_path) {
        Ok(text) => text,
        Err(_) => {
            let kind = if script_exit == Some(0) {
                "metric_emission_failure"
            } else {
                "build_error"
            };
            let doc = synthetic_doc(args, kind);
            write_doc(&doc, args.pretty, &metrics_path)?;
            return Ok(ExitCode::from(1));
        }
    };

    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            emit_diagnostic(&json!({
                "error": "invalid_metrics_json",
                "detail": err.to_string(),
                "path": metrics_path.to_string_lossy(),
            }));
            return Ok(ExitCode::from(3));
        }
    };

    let parsed_obj = match parsed {
        Value::Object(map) => map,
        other => {
            emit_diagnostic(&json!({
                "error": "metrics_not_object",
                "got_kind": kind_of(&other),
            }));
            return Ok(ExitCode::from(3));
        }
    };

    if parsed_obj.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        emit_diagnostic(&json!({
            "error": "schema_mismatch",
            "expected": SCHEMA,
            "got": parsed_obj.get("schema").cloned().unwrap_or(Value::Null),
        }));
        return Ok(ExitCode::from(3));
    }

    let mut doc = parsed_obj;
    doc.insert("head_sha".to_string(), Value::String(args.head_sha.clone()));
    doc.insert(
        "iteration".to_string(),
        args.iteration.map_or(Value::Null, Value::from),
    );
    doc.insert("captured_at".to_string(), Value::String(now_rfc3339()));
    doc.remove("output_digest");

    let digest = compute_digest(&Value::Object(doc.clone()));
    doc.insert("output_digest".to_string(), Value::String(digest));

    let script_ok = script_exit == Some(0);
    let blocking_zero = doc
        .get("audit")
        .and_then(|a| a.get("blocking_count"))
        .and_then(Value::as_u64)
        == Some(0);
    let total = doc
        .get("ac_total_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let results_len: u64 = doc
        .get("ac_results")
        .and_then(Value::as_array)
        .map_or(0, |a| u64::try_from(a.len()).unwrap_or(u64::MAX));

    let exit_code = if script_ok && blocking_zero && results_len == total {
        ExitCode::from(0)
    } else {
        ExitCode::from(1)
    };

    let final_value = Value::Object(doc);
    write_doc(&final_value, args.pretty, &metrics_path)?;
    Ok(exit_code)
}

fn synthetic_doc(args: &Args, failure_kind: &str) -> Value {
    let mut map = Map::new();
    map.insert("schema".into(), Value::String(SCHEMA.into()));
    map.insert("head_sha".into(), Value::String(args.head_sha.clone()));
    map.insert(
        "iteration".into(),
        args.iteration.map_or(Value::Null, Value::from),
    );
    map.insert("scalars".into(), Value::Object(Map::new()));
    map.insert("ac_passing_count".into(), Value::from(0));
    map.insert("ac_total_count".into(), Value::from(0));
    map.insert("ac_results".into(), Value::Array(Vec::new()));
    map.insert(
        "audit".into(),
        json!({ "blocking_count": 0, "advisory_count": 0 }),
    );
    map.insert("clippy_warning_count".into(), Value::from(0));
    map.insert("test_coverage_pct".into(), Value::Null);
    map.insert("doc_coverage_pct".into(), Value::Null);
    map.insert("proptest_density".into(), Value::Null);
    map.insert("captured_at".into(), Value::String(now_rfc3339()));
    map.insert("failure_kind".into(), Value::String(failure_kind.into()));

    let digest = compute_digest(&Value::Object(map.clone()));
    map.insert("output_digest".into(), Value::String(digest));
    Value::Object(map)
}

fn write_doc(doc: &Value, pretty: bool, metrics_path: &Path) -> io::Result<()> {
    let sorted = sort_keys(doc);
    let text = if pretty {
        serde_json::to_string_pretty(&sorted).unwrap_or_default()
    } else {
        serde_json::to_string(&sorted).unwrap_or_default()
    };
    fs::write(metrics_path, &text)?;
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{text}")?;
    Ok(())
}

fn emit_diagnostic(value: &Value) {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{text}");
}

fn compute_digest(doc_without_digest: &Value) -> String {
    let canonical = serde_json::to_vec(&sort_keys(doc_without_digest)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("sha256:{:x}", hasher.finalize())
}

fn sort_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(child) = map.get(key) {
                    sorted.insert(key.clone(), sort_keys(child));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| {
        meta.is_file() && (meta.permissions().mode() & 0o111) != 0
    })
}

fn run_script(
    script_path: &Path,
    cwd: &Path,
    log_path: &Path,
    timeout_secs: u64,
) -> io::Result<(Option<i32>, bool)> {
    let log = fs::File::create(log_path)?;
    let log_clone = log.try_clone()?;
    let mut child = Command::new("bash")
        .arg(script_path)
        .current_dir(cwd)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_clone))
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok((None, true));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0u64, |d| d.as_secs());
    let days_i64 = i64::try_from(secs / 86_400).unwrap_or(0);
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;
    let (year, month, day) = civil_from_days(days_i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year_in_era = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year_final = if month <= 2 { year_in_era + 1 } else { year_in_era };
    let year_out = i32::try_from(year_final).unwrap_or(0);
    let month_out = u32::try_from(month).unwrap_or(1);
    let day_out = u32::try_from(day).unwrap_or(1);
    (year_out, month_out, day_out)
}
