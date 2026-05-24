//! Shared types and helpers used by every producer.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use serde::Serialize;

use crate::PRODUCER_SPECS;
use crate::producers;

/// One producer's registration row in [`crate::PRODUCER_SPECS`].
#[derive(Debug, Clone, Copy)]
pub struct ProducerSpec {
    /// Producer name (kebab-case, e.g. `"supply-audit"`).
    pub name: &'static str,
    /// The `"schema"` string the producer must write into its receipt JSON.
    pub schema: &'static str,
    /// Filename written under `target/autobuilder/receipts/`.
    pub file_name: &'static str,
    /// Verdict strings that count as passing for this producer.
    pub pass_verdicts: &'static [&'static str],
}

impl ProducerSpec {
    /// Lookup a producer by its kebab-case name. Used by the bin entries.
    #[must_use]
    pub fn lookup(name: &str) -> Option<&'static Self> {
        PRODUCER_SPECS.iter().find(|s| s.name == name)
    }
}

/// Common envelope every producer's receipt JSON includes.
///
/// `head_sha`, `captured_at`, and `receipt_digest` are populated by
/// [`autobuilder_receipt::write`]'s digest-binding write; the producer only
/// needs to supply the verdict + payload via [`write_receipt`].
#[derive(Debug, Serialize)]
pub struct ReceiptEnvelope<P: Serialize> {
    /// Receipt schema string (e.g. `"autobuilder.supply_audit_receipt.v1"`).
    pub schema: &'static str,
    /// Aggregate verdict for this audit run.
    pub verdict: &'static str,
    /// HEAD sha at audit time (populated by writer).
    pub head_sha: String,
    /// RFC3339 timestamp at audit time (populated by writer).
    pub captured_at: String,
    /// sha256 over the canonicalized JSON with this field set to `""`.
    pub receipt_digest: String,
    /// Producer-specific payload (one struct per producer).
    #[serde(flatten)]
    pub payload: P,
}

/// Per-producer payload; producers serialize one of these and call
/// [`write_receipt`].
pub trait Payload: Serialize {}
impl<T: Serialize> Payload for T {}

/// Run a named producer against the given project directory. The producer
/// writes `target/autobuilder/receipts/<spec.file_name>` and returns a brief
/// human-readable summary line.
///
/// # Errors
///
/// Returns an error if the spec is unknown or if the producer fails (IO,
/// parse, or audit-internal error).
pub fn run_producer(name: &str, project: &Path) -> Result<String> {
    let spec = ProducerSpec::lookup(name)
        .ok_or_else(|| anyhow!("unknown producer: {name}; check PRODUCER_SPECS"))?;

    let project = project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", project.display()))?;

    match spec.name {
        "supply-audit" => producers::supply_audit::run(spec, &project),
        "license-audit" => producers::license_audit::run(spec, &project),
        "secrets-scan" => producers::secrets_scan::run(spec, &project),
        "sbom" => producers::sbom::run(spec, &project),
        "determinism" => producers::determinism::run(spec, &project),
        "hermetic-build" => producers::hermetic_build::run(spec, &project),
        "msrv-verify" => producers::msrv_verify::run(spec, &project),
        "binary-size" => producers::binary_size::run(spec, &project),
        "cold-build-time" => producers::cold_build_time::run(spec, &project),
        "bench-delta" => producers::bench_delta::run(spec, &project),
        "semver-check" => producers::semver_check::run(spec, &project),
        "cli-surface" => producers::cli_surface::run(spec, &project),
        "schema-compat" => producers::schema_compat::run(spec, &project),
        "ac-traceability" => producers::ac_traceability::run(spec, &project),
        "mutation-kill" => producers::mutation_kill::run(spec, &project),
        "flake-audit" => producers::flake_audit::run(spec, &project),
        "experiment" => producers::experiment::run(spec, &project),
        other => Err(anyhow!("producer {other} is in PRODUCER_SPECS but not wired in run_producer")),
    }
}

/// Compute the canonical receipt path inside a project tree.
#[must_use]
pub fn receipt_path(project: &Path, spec: &ProducerSpec) -> PathBuf {
    project
        .join("target/autobuilder/receipts")
        .join(spec.file_name)
}

/// Write a producer receipt JSON via [`autobuilder_receipt::write`].
///
/// The writer fills in `head_sha`, `captured_at`, and `receipt_digest`. The
/// producer constructs the rest of the JSON object.
///
/// # Errors
///
/// Returns an error if the path can't be created or the digest-binding
/// write fails.
pub fn write_receipt<P: Payload>(
    project: &Path,
    spec: &ProducerSpec,
    verdict: &'static str,
    payload: P,
) -> Result<()> {
    if !spec.pass_verdicts.contains(&verdict) && verdict != "block" {
        return Err(anyhow!(
            "producer {} attempted to emit invalid verdict {verdict:?}; allowed: {:?} or \"block\"",
            spec.name,
            spec.pass_verdicts
        ));
    }
    let path = receipt_path(project, spec);
    let head_sha = git_rev_parse_head(project)
        .with_context(|| format!("resolve HEAD for {} receipt", spec.name))?;
    let captured_at = autobuilder_receipt::now_rfc3339()
        .context("RFC3339 timestamp for receipt envelope")?;
    let envelope = ReceiptEnvelope {
        schema: spec.schema,
        verdict,
        head_sha,
        captured_at,
        receipt_digest: String::new(),
        payload,
    };
    let value = serde_json::to_value(&envelope)
        .with_context(|| format!("serialize {} envelope", spec.name))?;
    autobuilder_receipt::write(&path, value)
        .with_context(|| format!("write {} receipt to {}", spec.name, path.display()))?;
    Ok(())
}

/// Resolve `git rev-parse HEAD` for the given project directory.
///
/// Returns the 40-char hex sha. Falls back to `"0000000000000000000000000000000000000000"`
/// when the project isn't a git repository (e.g. test fixtures), so receipts
/// still write a structurally valid envelope rather than aborting.
///
/// # Errors
///
/// Returns an error if `git` cannot be spawned or returns non-UTF-8 output.
pub fn git_rev_parse_head(project: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "HEAD"])
        .output();
    let Ok(output) = output else {
        return Ok("0".repeat(40));
    };
    if !output.status.success() {
        return Ok("0".repeat(40));
    }
    let s = String::from_utf8(output.stdout).context("git stdout not UTF-8")?;
    Ok(s.trim().to_owned())
}
