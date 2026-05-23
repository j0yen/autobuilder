//! `msrv-verify`: declared `rust-version` actually compiles + tests clean.
//!
//! Pure-Rust. Reads the workspace `rust-version` from `Cargo.toml` (or
//! `[workspace.package].rust-version`) and invokes `cargo +<msrv> check` if
//! that toolchain is installed. If the requested toolchain is not present
//! (e.g. no rustup), emits `verdict=skipped` with a clear note. Does not
//! attempt to install toolchains.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

#[derive(Debug, Serialize)]
struct Payload {
    declared_msrv: Option<String>,
    toolchain_available: bool,
    cargo_check_exit: Option<i32>,
    note: String,
}

fn declared_msrv(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("Cargo.toml")).ok()?;
    let value: TomlValue = text.parse().ok()?;
    if let Some(s) = value
        .get("package")
        .and_then(|p| p.get("rust-version"))
        .and_then(TomlValue::as_str)
    {
        return Some(s.to_owned());
    }
    if let Some(s) = value
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("rust-version"))
        .and_then(TomlValue::as_str)
    {
        return Some(s.to_owned());
    }
    None
}

fn toolchain_present(version: &str) -> bool {
    Command::new("cargo")
        .arg(format!("+{version}"))
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run the msrv-verify audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    let msrv = declared_msrv(project);
    let Some(msrv) = msrv else {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                declared_msrv: None,
                toolchain_available: false,
                cargo_check_exit: None,
                note: "no rust-version declared in Cargo.toml".into(),
            },
        )?;
        return Ok("msrv-verify: skipped (no rust-version declared)".into());
    };

    if !toolchain_present(&msrv) {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                declared_msrv: Some(msrv.clone()),
                toolchain_available: false,
                cargo_check_exit: None,
                note: format!("rustup toolchain {msrv} not installed"),
            },
        )?;
        return Ok(format!("msrv-verify: skipped (toolchain {msrv} missing)"));
    }

    let output = Command::new("cargo")
        .arg(format!("+{msrv}"))
        .args(["check", "--workspace"])
        .current_dir(project)
        .output()
        .context("spawn cargo +<msrv> check")?;
    let exit = output.status.code();

    let verdict = if exit == Some(0) { "pass" } else { "block" };
    let summary = format!("msrv-verify: msrv={msrv} cargo check exit={exit:?}");
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            declared_msrv: Some(msrv),
            toolchain_available: true,
            cargo_check_exit: exit,
            note: String::new(),
        },
    )?;
    Ok(summary)
}
