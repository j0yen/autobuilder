//! `schema-compat`: receipt JSON schemas added/changed are additive-only.
//!
//! Walks `<project>/schemas/*.json` (JSON-schema documents declaring receipt
//! shapes). For each, fetches the prior version via `git show HEAD~1:...`,
//! parses both, and asserts every `required` field at HEAD~1 is still
//! `required` at HEAD; and every `properties.<name>.type` matches.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    schemas_checked: Vec<SchemaCheck>,
    incompatibilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SchemaCheck {
    path: String,
    additive_only: bool,
    notes: Vec<String>,
}

fn git_show(project: &Path, refspec: &str) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["show", refspec])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn check_one(old: &JsonValue, new: &JsonValue) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    if let (Some(or), Some(nr)) = (
        old.get("required").and_then(JsonValue::as_array),
        new.get("required").and_then(JsonValue::as_array),
    ) {
        let new_set: std::collections::BTreeSet<&str> = nr
            .iter()
            .filter_map(JsonValue::as_str)
            .collect();
        for v in or {
            if let Some(name) = v.as_str() {
                if !new_set.contains(name) {
                    issues.push(format!("required field removed: {name}"));
                }
            }
        }
    }
    if let (Some(op), Some(np)) = (
        old.get("properties").and_then(JsonValue::as_object),
        new.get("properties").and_then(JsonValue::as_object),
    ) {
        for (name, old_prop) in op {
            let Some(new_prop) = np.get(name) else {
                issues.push(format!("property removed: {name}"));
                continue;
            };
            let ot = old_prop.get("type");
            let nt = new_prop.get("type");
            if ot != nt {
                issues.push(format!(
                    "property {name} type changed: {ot:?} -> {nt:?}"
                ));
            }
        }
    }
    issues
}

/// Run the schema-compat audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let schemas_dir = project.join("schemas");
    if !schemas_dir.is_dir() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                schemas_checked: Vec::new(),
                incompatibilities: vec!["no schemas/ directory".into()],
            },
        )?;
        return Ok("schema-compat: skipped (no schemas/)".into());
    }

    let mut checks: Vec<SchemaCheck> = Vec::new();
    let mut incompat: Vec<String> = Vec::new();

    for entry in walkdir::WalkDir::new(&schemas_dir).max_depth(2) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(project)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .into_owned();
        let Ok(new_bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        let Ok(new): serde_json::Result<JsonValue> = serde_json::from_slice(&new_bytes) else {
            continue;
        };
        let refspec = format!("HEAD~1:{rel}");
        let Some(old_text) = git_show(project, &refspec) else {
            checks.push(SchemaCheck {
                path: rel,
                additive_only: true,
                notes: vec!["new schema (no prior version)".into()],
            });
            continue;
        };
        let Ok(old): serde_json::Result<JsonValue> = serde_json::from_str(&old_text) else {
            continue;
        };
        let issues = check_one(&old, &new);
        let additive = issues.is_empty();
        if !additive {
            incompat.push(rel.clone());
        }
        checks.push(SchemaCheck {
            path: rel,
            additive_only: additive,
            notes: issues,
        });
    }

    let verdict = if incompat.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "schema-compat: {} schemas checked, {} incompatible",
        checks.len(),
        incompat.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            schemas_checked: checks,
            incompatibilities: incompat,
        },
    )?;
    Ok(summary)
}
