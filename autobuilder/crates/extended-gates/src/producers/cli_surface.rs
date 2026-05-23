//! `cli-surface`: every declared bin's `--help` output matches its snapshot.
//!
//! Snapshots live at `<project>/cli-surface-snapshots/<bin>.txt`. The
//! producer invokes `target/release/<bin> --help`, compares byte-equality
//! against the snapshot, and flags drift. Missing snapshot → skipped for
//! that bin (with a hint to bootstrap via `--bootstrap`); missing binary
//! → block.

use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    bins_checked: Vec<BinCheck>,
    drifted: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BinCheck {
    name: String,
    snapshot_digest: Option<String>,
    current_digest: Option<String>,
    match_: bool,
    note: String,
}

fn sha256_str(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("sha256:{:x}", h.finalize())
}

/// Run the cli-surface audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
#[allow(clippy::too_many_lines)]
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let snap_dir = project.join("cli-surface-snapshots");
    if !snap_dir.is_dir() {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                bins_checked: Vec::new(),
                drifted: vec!["no cli-surface-snapshots/ directory".into()],
            },
        )?;
        return Ok("cli-surface: skipped (no snapshots dir)".into());
    }

    let mut checks: Vec<BinCheck> = Vec::new();
    let mut drifted: Vec<String> = Vec::new();
    let release_dir = project.join("target/release");

    let entries = std::fs::read_dir(&snap_dir);
    let Ok(entries) = entries else {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                bins_checked: Vec::new(),
                drifted: vec!["snapshots dir unreadable".into()],
            },
        )?;
        return Ok("cli-surface: skipped (snapshots unreadable)".into());
    };

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(bin_name) = name.strip_suffix(".txt") else {
            continue;
        };
        let snapshot = std::fs::read_to_string(entry.path()).unwrap_or_default();
        let snapshot_digest = Some(sha256_str(&snapshot));

        let bin_path = release_dir.join(bin_name);
        let output = Command::new(&bin_path).arg("--help").output();
        let Ok(output) = output else {
            checks.push(BinCheck {
                name: bin_name.to_owned(),
                snapshot_digest,
                current_digest: None,
                match_: false,
                note: format!("binary {bin_name} not found at target/release"),
            });
            drifted.push(bin_name.to_owned());
            continue;
        };
        let current = String::from_utf8(output.stdout).unwrap_or_default();
        let current_digest = sha256_str(&current);
        let match_ = current == snapshot;
        if !match_ {
            drifted.push(bin_name.to_owned());
        }
        checks.push(BinCheck {
            name: bin_name.to_owned(),
            snapshot_digest,
            current_digest: Some(current_digest),
            match_,
            note: String::new(),
        });
    }

    let verdict = if drifted.is_empty() { "pass" } else { "block" };
    let summary = format!(
        "cli-surface: {} bins, {} drifted",
        checks.len(),
        drifted.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            bins_checked: checks,
            drifted,
        },
    )?;
    Ok(summary)
}
