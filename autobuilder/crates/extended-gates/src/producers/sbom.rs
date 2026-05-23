//! `sbom`: emit a `CycloneDX`-shape SBOM JSON of the workspace deps.
//!
//! Pure-Rust. Parses `Cargo.lock`, materializes a minimal `CycloneDX` 1.5
//! BOM JSON envelope embedded in the receipt's `bom` field.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    components_count: usize,
    bom: serde_json::Value,
}

/// Build the SBOM receipt for `project`.
///
/// # Errors
///
/// Returns an error if Cargo.lock can't be read/parsed or the receipt write
/// fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let lock_path = project.join("Cargo.lock");
    let lock_text = std::fs::read_to_string(&lock_path)
        .with_context(|| format!("read {}", lock_path.display()))?;
    let lock: TomlValue = lock_text
        .parse()
        .context("parse Cargo.lock as TOML for SBOM")?;
    let packages = lock
        .get("package")
        .and_then(TomlValue::as_array)
        .cloned()
        .unwrap_or_default();

    let components: Vec<serde_json::Value> = packages
        .iter()
        .filter_map(|pkg| {
            let name = pkg.get("name").and_then(TomlValue::as_str)?;
            let version = pkg.get("version").and_then(TomlValue::as_str)?;
            let license = pkg.get("license").and_then(TomlValue::as_str);
            let mut component = json!({
                "type": "library",
                "name": name,
                "version": version,
                "purl": format!("pkg:cargo/{name}@{version}"),
                "bom-ref": format!("{name}-{version}"),
            });
            if let Some(license) = license {
                if let Some(obj) = component.as_object_mut() {
                    obj.insert(
                        "licenses".to_owned(),
                        json!([{ "license": { "id": license } }]),
                    );
                }
            }
            Some(component)
        })
        .collect();

    let bom = json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "components": components,
    });

    let count = components.len();
    let summary = format!("sbom: {count} components");
    write_receipt(
        project,
        spec,
        "pass",
        Payload {
            components_count: count,
            bom,
        },
    )?;
    Ok(summary)
}
