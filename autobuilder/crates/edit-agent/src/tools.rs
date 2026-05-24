//! Tool implementations the agentic loop dispatches to.
//!
//! Each tool returns `ToolOutput { text, is_error }`. The agentic loop
//! wraps the output in a `ToolResult` content block and feeds it back to
//! the model. Errors surfaced via `is_error: true` are non-fatal — the
//! model sees them and decides how to recover. Errors that bubble up via
//! `Result::Err` instead are fatal to the session.

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::sandbox::Sandbox;

const MAX_READ_BYTES: usize = 200_000;
const MAX_TOOL_OUTPUT_BYTES: usize = 60_000;
const BASH_TIMEOUT_SECONDS: u64 = 120;

/// Materialised tool output: text payload + soft error flag.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: truncate(text.into()),
            is_error: false,
        }
    }
    pub fn err(text: impl Into<String>) -> Self {
        Self {
            text: truncate(text.into()),
            is_error: true,
        }
    }
}

fn truncate(mut s: String) -> String {
    if s.len() > MAX_TOOL_OUTPUT_BYTES {
        s.truncate(MAX_TOOL_OUTPUT_BYTES);
        s.push_str("\n…[truncated]");
    }
    s
}

/// Dispatch a tool call by name.
///
/// # Errors
/// Returns an error for unknown tool names. Tool-level failures (file not
/// found, command non-zero exit, etc.) are returned as soft errors via
/// `ToolOutput::err` rather than `Err`.
pub fn dispatch(name: &str, input: &Value, sandbox: &Sandbox) -> Result<ToolOutput> {
    match name {
        "read_file" => Ok(read_file(input, sandbox)),
        "write_file" => Ok(write_file(input, sandbox)),
        "edit_file" => Ok(edit_file(input, sandbox)),
        "bash" => Ok(bash(input, sandbox)),
        other => Err(anyhow!("unknown tool `{other}`")),
    }
}

fn str_field<'a>(input: &'a Value, key: &str) -> Result<&'a str, ToolOutput> {
    input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolOutput::err(format!("missing string field `{key}` in tool input")))
}

fn read_file(input: &Value, sandbox: &Sandbox) -> ToolOutput {
    let path = match str_field(input, "path") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = match sandbox.resolve_for_read(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::err(format!("sandbox: {e}")),
    };
    match fs::read_to_string(&resolved) {
        Ok(s) if s.len() > MAX_READ_BYTES => ToolOutput::ok(format!(
            "{}\n…[file is {} bytes; truncated to {MAX_READ_BYTES}]",
            &s[..MAX_READ_BYTES.min(s.len())],
            s.len()
        )),
        Ok(s) => ToolOutput::ok(s),
        Err(e) => ToolOutput::err(format!("read {}: {e}", resolved.display())),
    }
}

fn write_file(input: &Value, sandbox: &Sandbox) -> ToolOutput {
    let path = match str_field(input, "path") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let content = match str_field(input, "content") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = match sandbox.resolve_for_write(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::err(format!("sandbox: {e}")),
    };
    if let Err(e) = fs::write(&resolved, content) {
        return ToolOutput::err(format!("write {}: {e}", resolved.display()));
    }
    ToolOutput::ok(format!("wrote {} bytes to {}", content.len(), resolved.display()))
}

fn edit_file(input: &Value, sandbox: &Sandbox) -> ToolOutput {
    let path = match str_field(input, "path") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let old_string = match str_field(input, "old_string") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let new_string = match str_field(input, "new_string") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let resolved = match sandbox.resolve_for_read(path) {
        Ok(p) => p,
        Err(e) => return ToolOutput::err(format!("sandbox: {e}")),
    };
    let existing = match fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => return ToolOutput::err(format!("read {}: {e}", resolved.display())),
    };
    let occurrences = existing.matches(old_string).count();
    if occurrences == 0 {
        return ToolOutput::err(format!(
            "edit {}: `old_string` not found",
            resolved.display()
        ));
    }
    if occurrences > 1 {
        return ToolOutput::err(format!(
            "edit {}: `old_string` matched {occurrences} times; pass a more specific string",
            resolved.display()
        ));
    }
    let updated = existing.replacen(old_string, new_string, 1);
    if let Err(e) = fs::write(&resolved, &updated) {
        return ToolOutput::err(format!("write {}: {e}", resolved.display()));
    }
    ToolOutput::ok(format!(
        "edited {}: replaced 1 occurrence ({} → {} bytes)",
        resolved.display(),
        old_string.len(),
        new_string.len()
    ))
}

fn bash(input: &Value, sandbox: &Sandbox) -> ToolOutput {
    let command = match str_field(input, "command") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let cwd = sandbox.root();
    match run_bash(command, cwd) {
        Ok(text) => ToolOutput::ok(text),
        Err(e) => ToolOutput::err(format!("bash: {e}")),
    }
}

fn run_bash(command: &str, cwd: &Path) -> Result<String> {
    let mut child = Command::new("bash")
        .args(["-c", command])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning bash -c")?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(BASH_TIMEOUT_SECONDS);
    loop {
        if let Some(status) = child.try_wait().context("polling bash child")? {
            let out = child.wait_with_output().context("collecting bash output")?;
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let exit = status.code().unwrap_or(-1);
            return Ok(format!(
                "exit_code: {exit}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            ));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            return Err(anyhow!(
                "bash command timed out after {BASH_TIMEOUT_SECONDS} seconds"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn sandbox_for(p: &Path) -> Sandbox {
        Sandbox::new(p).unwrap()
    }

    #[test]
    fn read_file_returns_content() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch("read_file", &json!({"path": "a.txt"}), &s).unwrap();
        assert!(!out.is_error, "got error: {}", out.text);
        assert_eq!(out.text, "hello");
    }

    #[test]
    fn read_file_missing_yields_soft_error() {
        let tmp = TempDir::new().unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch("read_file", &json!({"path": "nope.txt"}), &s).unwrap();
        assert!(out.is_error);
    }

    #[test]
    fn write_file_creates_file() {
        let tmp = TempDir::new().unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch(
            "write_file",
            &json!({"path": "b.txt", "content": "world"}),
            &s,
        )
        .unwrap();
        assert!(!out.is_error, "got error: {}", out.text);
        assert_eq!(std::fs::read_to_string(tmp.path().join("b.txt")).unwrap(), "world");
    }

    #[test]
    fn write_file_traversal_is_rejected() {
        let outer = TempDir::new().unwrap();
        let inner = outer.path().join("in");
        std::fs::create_dir_all(&inner).unwrap();
        let s = sandbox_for(&inner);
        let out = dispatch(
            "write_file",
            &json!({"path": "../escape.txt", "content": "x"}),
            &s,
        )
        .unwrap();
        assert!(out.is_error);
        assert!(out.text.contains("sandbox"));
    }

    #[test]
    fn edit_file_replaces_unique_string() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("c.txt"), "alpha BETA gamma").unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch(
            "edit_file",
            &json!({"path": "c.txt", "old_string": "BETA", "new_string": "beta"}),
            &s,
        )
        .unwrap();
        assert!(!out.is_error, "got error: {}", out.text);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("c.txt")).unwrap(),
            "alpha beta gamma"
        );
    }

    #[test]
    fn edit_file_ambiguous_match_errors() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("d.txt"), "x x x").unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch(
            "edit_file",
            &json!({"path": "d.txt", "old_string": "x", "new_string": "y"}),
            &s,
        )
        .unwrap();
        assert!(out.is_error);
        assert!(out.text.contains("matched 3 times"));
    }

    #[test]
    fn edit_file_missing_match_errors() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("e.txt"), "abc").unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch(
            "edit_file",
            &json!({"path": "e.txt", "old_string": "zzz", "new_string": "y"}),
            &s,
        )
        .unwrap();
        assert!(out.is_error);
        assert!(out.text.contains("not found"));
    }

    #[test]
    fn bash_echoes_to_stdout() {
        let tmp = TempDir::new().unwrap();
        let s = sandbox_for(tmp.path());
        let out = dispatch("bash", &json!({"command": "echo hi"}), &s).unwrap();
        assert!(!out.is_error, "got error: {}", out.text);
        assert!(out.text.contains("hi"));
        assert!(out.text.contains("exit_code: 0"));
    }

    #[test]
    fn dispatch_unknown_tool_errors() {
        let tmp = TempDir::new().unwrap();
        let s = sandbox_for(tmp.path());
        let err = dispatch("teleport", &json!({}), &s).unwrap_err();
        assert!(format!("{err}").contains("unknown tool"));
    }
}
