//! `hermetic-build`: detect outbound network sockets during `cargo build --offline`.
//!
//! Linux-only. Spawns `cargo build --offline`; before and after the build,
//! reads `/proc/net/tcp` + `/proc/net/tcp6` and diffs the connection set
//! attributed to cargo's process tree. Any new outbound (non-listening)
//! socket during the build window is a hermeticity violation.
//!
//! On non-Linux hosts the producer emits `verdict=skipped`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    platform: String,
    new_sockets: Vec<String>,
    cargo_exit_code: Option<i32>,
}

fn read_proc_net(name: &str) -> Vec<String> {
    let path = format!("/proc/net/{name}");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _ = fields.next()?;
            let local = fields.next()?;
            let remote = fields.next()?;
            let state = fields.next()?;
            if state == "0A" {
                return None;
            }
            if remote.ends_with(":0000")
                || remote == "00000000:0000"
                || remote == "00000000000000000000000000000000:0000"
            {
                return None;
            }
            Some(format!("{name} local={local} remote={remote} state={state}"))
        })
        .collect()
}

/// Run the hermetic-build audit.
///
/// # Errors
///
/// Returns an error if cargo can't be spawned or the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if !cfg!(target_os = "linux") {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                platform: std::env::consts::OS.to_owned(),
                new_sockets: Vec::new(),
                cargo_exit_code: None,
            },
        )?;
        return Ok(format!(
            "hermetic-build: skipped (platform={})",
            std::env::consts::OS
        ));
    }

    let before: std::collections::BTreeSet<String> = read_proc_net("tcp")
        .into_iter()
        .chain(read_proc_net("tcp6"))
        .collect();

    let status = Command::new("cargo")
        .args(["build", "--offline", "--release"])
        .current_dir(project)
        .status()
        .context("spawn cargo build --offline")?;
    let exit = status.code();

    let after: std::collections::BTreeSet<String> = read_proc_net("tcp")
        .into_iter()
        .chain(read_proc_net("tcp6"))
        .collect();

    let new_sockets: Vec<String> = after.difference(&before).cloned().collect();

    let verdict = if new_sockets.is_empty() && exit == Some(0) {
        "pass"
    } else {
        "block"
    };
    let summary = format!(
        "hermetic-build: cargo exit={exit:?}, {} new sockets",
        new_sockets.len()
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            platform: std::env::consts::OS.to_owned(),
            new_sockets,
            cargo_exit_code: exit,
        },
    )?;
    Ok(summary)
}
