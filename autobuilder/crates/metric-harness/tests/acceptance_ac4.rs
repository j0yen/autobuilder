//! AC4 (MUST) — Normalized metrics doc to stdout AND target/autobuilder/metrics.json.
//!
//! Spec (from agent/intent-card.json AC4):
//!   Emit a normalized autobuilder.metrics.v1 document to stdout AND
//!   overwrite <path>/target/autobuilder/metrics.json with the same content.
//!   Include head_sha (from --head-sha flag), iteration (from --iteration N),
//!   scalars, ac_passing_count, ac_total_count, audit.{blocking_count,
//!   advisory_count}, clippy_warning_count, captured_at (RFC3339),
//!   output_digest (sha256 of canonical JSON excluding output_digest itself).
//!
//! READ-ONLY after scaffold.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::TempDir;

const HARNESS_BIN: &str = env!("CARGO_BIN_EXE_autobuilder-metric-harness");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn write_exec_script(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    std::fs::write(path, body)?;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

#[test]
fn acceptance_ac4_emits_normalized_doc_to_stdout_and_disk() -> TestResult {
    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let body = "#!/usr/bin/env bash\n\
                mkdir -p target/autobuilder\n\
                cat > target/autobuilder/metrics.json <<'JSON'\n\
{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{\"acceptance_tests_passing_count\":0},\"ac_passing_count\":0,\"ac_total_count\":0,\"ac_results\":[],\"audit\":{\"blocking_count\":0,\"advisory_count\":0},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}\n\
JSON\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), body)?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("cafebabe")
        .arg("--iteration")
        .arg("7")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout_doc: serde_json::Value = serde_json::from_str(stdout.trim())?;

    // Required fields.
    for field in [
        "schema",
        "head_sha",
        "iteration",
        "scalars",
        "ac_passing_count",
        "ac_total_count",
        "audit",
        "clippy_warning_count",
        "captured_at",
        "output_digest",
    ] {
        assert!(
            stdout_doc.get(field).is_some(),
            "AC4: stdout doc missing required field `{field}`. Doc: {stdout_doc}",
        );
    }

    assert_eq!(
        stdout_doc.get("schema").and_then(serde_json::Value::as_str),
        Some("autobuilder.metrics.v1"),
        "AC4: schema field must be autobuilder.metrics.v1",
    );
    assert_eq!(
        stdout_doc.get("head_sha").and_then(serde_json::Value::as_str),
        Some("cafebabe"),
        "AC4: head_sha must come from --head-sha flag",
    );
    assert_eq!(
        stdout_doc.get("iteration").and_then(serde_json::Value::as_i64),
        Some(7),
        "AC4: iteration must come from --iteration flag",
    );

    let audit = stdout_doc.get("audit").ok_or("missing audit")?;
    assert!(audit.get("blocking_count").is_some(), "AC4: audit.blocking_count required");
    assert!(audit.get("advisory_count").is_some(), "AC4: audit.advisory_count required");

    let digest = stdout_doc
        .get("output_digest")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing output_digest")?;
    assert!(
        digest.starts_with("sha256:") && digest.len() > "sha256:".len(),
        "AC4: output_digest must be sha256:<hex>; got: {digest}",
    );

    // Disk equals stdout.
    let on_disk = std::fs::read_to_string(project.path().join("target/autobuilder/metrics.json"))?;
    let on_disk_doc: serde_json::Value = serde_json::from_str(&on_disk)?;
    assert_eq!(
        on_disk_doc, stdout_doc,
        "AC4: on-disk metrics.json must equal stdout content",
    );
    Ok(())
}

#[test]
fn acceptance_ac4_output_digest_matches_canonical_recomputation() -> TestResult {
    use sha2::{Digest, Sha256};

    let project = TempDir::new()?;
    let scripts_dir = project.path().join("scripts");
    std::fs::create_dir_all(&scripts_dir)?;

    let body = "#!/usr/bin/env bash\n\
                mkdir -p target/autobuilder\n\
                cat > target/autobuilder/metrics.json <<'JSON'\n\
{\"schema\":\"autobuilder.metrics.v1\",\"head_sha\":\"unknown\",\"iteration\":null,\"scalars\":{\"acceptance_tests_passing_count\":3},\"ac_passing_count\":3,\"ac_total_count\":10,\"ac_results\":[],\"audit\":{\"blocking_count\":0,\"advisory_count\":0},\"clippy_warning_count\":0,\"test_coverage_pct\":null,\"doc_coverage_pct\":null,\"proptest_density\":null,\"captured_at\":\"2026-05-21T00:00:00Z\"}\n\
JSON\n";
    write_exec_script(&scripts_dir.join("run-metrics.sh"), body)?;

    let output = Command::new(HARNESS_BIN)
        .arg(project.path())
        .arg("--head-sha")
        .arg("abc123")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut doc: serde_json::Value = serde_json::from_str(stdout.trim())?;
    let claimed_digest = doc
        .get("output_digest")
        .and_then(serde_json::Value::as_str)
        .ok_or("missing output_digest")?
        .to_owned();

    // Re-canonicalize: remove output_digest, sort keys recursively, sha256.
    if let Some(obj) = doc.as_object_mut() {
        obj.remove("output_digest");
    }
    let canonical_bytes = canonical_json_bytes(&doc);
    let mut hasher = Sha256::new();
    hasher.update(&canonical_bytes);
    let expected = format!("sha256:{:x}", hasher.finalize());

    assert_eq!(
        claimed_digest, expected,
        "AC4: output_digest must equal sha256 of canonical JSON excluding output_digest itself",
    );
    Ok(())
}

fn canonical_json_bytes(value: &serde_json::Value) -> Vec<u8> {
    fn sort(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let mut sorted = serde_json::Map::new();
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for k in keys {
                    if let Some(v) = map.get(k) {
                        sorted.insert(k.clone(), sort(v));
                    }
                }
                serde_json::Value::Object(sorted)
            }
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(sort).collect())
            }
            other => other.clone(),
        }
    }
    let sorted = sort(value);
    serde_json::to_vec(&sorted).unwrap_or_default()
}
