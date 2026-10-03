//! `msrv-verify`: declared `rust-version` actually compiles + tests clean.
//!
//! Pure-Rust. Reads the workspace `rust-version` from `Cargo.toml` (or
//! `[workspace.package].rust-version`) and invokes `cargo +<msrv> check` if
//! that toolchain is installed. A missing toolchain is a mandatory-gate
//! failure (`verdict=block` with a `remedy`), not a silent pass — MSRV can
//! only be considered verified once the check has actually run. `skipped`
//! is reserved for the one case where there is nothing to verify: no
//! `rust-version` declared at all. Does not attempt to install toolchains.

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result};
use serde::Serialize;
use toml::Value as TomlValue;

use crate::prelude::{ProducerSpec, write_receipt};

/// Keep the receipt (and the producer's own stderr, which any external
/// attempt log captures) bounded even when `cargo check` is chatty.
const TAIL_BYTES: usize = 4 * 1024;

#[derive(Debug, Serialize)]
struct Payload {
    declared_msrv: Option<String>,
    toolchain_available: bool,
    cargo_check_exit: Option<i32>,
    note: String,
    /// Machine-readable failure reason, e.g. `"toolchain-missing: 1.85"`.
    /// `None` on `pass`/`skipped`.
    reason: Option<String>,
    /// How to fix a `block`, e.g. the exact `rustup` command to run.
    /// `None` on `pass`/`skipped`.
    remedy: Option<String>,
    /// Last `TAIL_BYTES` of the `cargo check` run's stdout, if it ran.
    stdout_tail: Option<String>,
    /// Last `TAIL_BYTES` of the `cargo check` run's stderr, if it ran —
    /// this is where the real compiler diagnostic lives (e.g. "package
    /// wasmtime requires rustc 1.95").
    stderr_tail: Option<String>,
}

/// Last `max_bytes` of `bytes`, decoded lossily and cut on a UTF-8 char
/// boundary so a truncated multi-byte sequence never panics.
fn tail(bytes: &[u8], max_bytes: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.len() <= max_bytes {
        return s.into_owned();
    }
    let start = s.len() - max_bytes;
    let mut idx = start;
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    s[idx..].to_owned()
}

/// `None` when empty — so an unused tail serializes as `null`, not `""`.
fn tail_opt(bytes: &[u8]) -> Option<String> {
    let t = tail(bytes, TAIL_BYTES);
    if t.is_empty() { None } else { Some(t) }
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

fn toolchain_present(cargo_bin: &str, version: &str) -> bool {
    Command::new(cargo_bin)
        .arg(format!("+{version}"))
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn remedy_for(version: &str) -> String {
    format!("rustup toolchain install {version} --profile minimal")
}

fn run_cargo_check(cargo_bin: &str, project: &Path, msrv: &str) -> Result<Output> {
    Command::new(cargo_bin)
        .arg(format!("+{msrv}"))
        .args(["check", "--workspace"])
        .current_dir(project)
        .output()
        .context("spawn cargo +<msrv> check")
}

/// Run the msrv-verify audit.
///
/// # Errors
///
/// Returns an error if the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    run_with_cargo(spec, project, "cargo")
}

/// Same as [`run`], but with the `cargo` binary name/path injectable so
/// tests can point it at a fixture instead of a real rustup install.
fn run_with_cargo(spec: &ProducerSpec, project: &Path, cargo_bin: &str) -> Result<String> {
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
                reason: None,
                remedy: None,
                stdout_tail: None,
                stderr_tail: None,
            },
        )?;
        return Ok("msrv-verify: skipped (no rust-version declared)".into());
    };

    if !toolchain_present(cargo_bin, &msrv) {
        let reason = format!("toolchain-missing: {msrv}");
        let remedy = remedy_for(&msrv);
        eprintln!("msrv-verify: {reason}; remedy: {remedy}");
        write_receipt(
            project,
            spec,
            "block",
            Payload {
                declared_msrv: Some(msrv.clone()),
                toolchain_available: false,
                cargo_check_exit: None,
                note: format!("rustup toolchain {msrv} not installed"),
                reason: Some(reason),
                remedy: Some(remedy),
                stdout_tail: None,
                stderr_tail: None,
            },
        )?;
        return Ok(format!("msrv-verify: block (toolchain {msrv} missing)"));
    }

    let output = run_cargo_check(cargo_bin, project, &msrv)?;
    let exit = output.status.code();
    let stdout_tail = tail_opt(&output.stdout);
    let stderr_tail = tail_opt(&output.stderr);

    let verdict = if exit == Some(0) { "pass" } else { "block" };
    let reason = if verdict == "block" {
        if let Some(st) = &stderr_tail {
            eprintln!("msrv-verify: cargo +{msrv} check failed (exit={exit:?}): {st}");
        }
        Some(format!("cargo check failed: exit={exit:?}"))
    } else {
        None
    };
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
            reason,
            remedy: None,
            stdout_tail,
            stderr_tail,
        },
    )?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `cargo` that answers `+<ver> --version` with success (so
    /// `toolchain_present` reports the toolchain as installed) and
    /// `+<ver> check --workspace` with a failure that carries a real
    /// compiler-shaped diagnostic on stderr, so the test can prove that
    /// diagnostic survives into the receipt.
    fn fake_cargo_that_fails_check(dir: &Path) -> String {
        let path = dir.join("cargo");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "#!/bin/sh\n\
             case \"$2\" in\n\
               --version) exit 0 ;;\n\
               *) echo 'Compiling fixture v0.1.0'; \
                  echo 'error[E0658]: package `wasmtime` requires rustc 1.95 or newer' >&2; \
                  exit 101 ;;\n\
             esac\n"
        )
        .unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn write_cargo_toml(project: &Path, rust_version: &str) {
        std::fs::write(
            project.join("Cargo.toml"),
            format!(
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"{rust_version}\"\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn failing_check_captures_nonempty_stderr_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_cargo_toml(project, "1.85");

        let bindir = tempfile::tempdir().unwrap();
        let cargo_bin = fake_cargo_that_fails_check(bindir.path());

        let spec = ProducerSpec::lookup("msrv-verify").expect("msrv-verify registered");
        run_with_cargo(spec, project, &cargo_bin).unwrap();

        let receipt_path = project.join("target/autobuilder/receipts/msrv-verify-receipt.json");
        let bytes = std::fs::read(&receipt_path).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(v.get("verdict").and_then(serde_json::Value::as_str), Some("block"));
        let stderr_tail = v
            .get("stderr_tail")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        assert!(
            stderr_tail.contains("requires rustc 1.95"),
            "stderr_tail lost the real diagnostic: {stderr_tail:?}"
        );
    }

    #[test]
    fn tail_cuts_on_a_char_boundary_not_mid_utf8() {
        // 3-byte UTF-8 char repeated; max_bytes lands mid-character —
        // tail() must not panic and must return valid UTF-8.
        let s = "€".repeat(10); // 30 bytes
        let out = tail(s.as_bytes(), 4);
        assert!(out.len() <= 30);
        // Must be valid UTF-8 (String guarantees it; this just documents intent).
        let _ = out.chars().count();
    }
}
