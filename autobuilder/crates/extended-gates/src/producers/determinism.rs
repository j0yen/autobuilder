//! `determinism`: two cold `cargo build --release` runs produce identical artifact sha256.
//!
//! Pure-Rust. Invokes `cargo` twice with `cargo clean` between runs, sha256s
//! the artifact set, and compares. Cross-platform via the toolchain.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    artifacts: Vec<Artifact>,
    mismatched: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Artifact {
    path: String,
    digest_run_a: String,
    digest_run_b: String,
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn cargo(project: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("cargo")
        .arg("-q")
        .args(args)
        .current_dir(project)
        .status()
        .with_context(|| format!("spawn cargo {args:?}"))?;
    if !status.success() {
        anyhow::bail!("cargo {args:?} exited with {status}");
    }
    Ok(())
}

fn collect_release_bins(project: &Path) -> Vec<PathBuf> {
    let release_dir = project.join("target/release");
    let Ok(rd) = std::fs::read_dir(&release_dir) else {
        return Vec::new();
    };
    rd.flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_none()
                || p.extension().and_then(|s| s.to_str()) == Some("rlib")
        })
        .collect()
}

/// Run the determinism audit.
///
/// # Errors
///
/// Returns an error if cargo build fails or the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if std::env::var("AUTOBUILDER_SKIP_HEAVY").is_ok() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                artifacts: Vec::new(),
                mismatched: vec!["AUTOBUILDER_SKIP_HEAVY set".into()],
            },
        )?;
        return Ok("determinism: skipped (AUTOBUILDER_SKIP_HEAVY)".into());
    }

    cargo(project, &["clean"])?;
    cargo(project, &["build", "--release"])?;
    let mut run_a: Vec<(PathBuf, String)> = Vec::new();
    for p in collect_release_bins(project) {
        let d = sha256_file(&p)?;
        run_a.push((p, d));
    }

    cargo(project, &["clean"])?;
    cargo(project, &["build", "--release"])?;
    let mut artifacts: Vec<Artifact> = Vec::new();
    let mut mismatched: Vec<String> = Vec::new();
    for (p, da) in run_a {
        let db = sha256_file(&p).unwrap_or_default();
        let rel = p
            .strip_prefix(project)
            .unwrap_or(&p)
            .to_string_lossy()
            .into_owned();
        if da != db {
            mismatched.push(rel.clone());
        }
        artifacts.push(Artifact {
            path: rel,
            digest_run_a: da,
            digest_run_b: db,
        });
    }

    let verdict = if mismatched.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "determinism: {} artifacts, {} mismatched",
        artifacts.len(),
        mismatched.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            artifacts,
            mismatched,
        },
    )?;
    Ok(summary)
}
