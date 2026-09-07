//! AC6 (P1) — `scripts/run-metrics.sh` must succeed against a fresh git
//! worktree of this repo, not just the long-lived checkout every other
//! test and gate run exercises. This is the "fresh-checkout" failure
//! class rustbuild-gate-debt's harness hit: a script that only works
//! because some prior run already left a built binary or a populated
//! `target/` dir lying around is not actually verified.
//!
//! `#[ignore]`d by default (real `cargo build --release` + a real `git
//! worktree add`, same convention as `acceptance_heavy_ignored.rs` in
//! `crates/extended-gates/tests/` for the other heavyweight, real-process
//! ACs) — run explicitly with `cargo test --release --test
//! fresh_worktree_run_metrics -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The outer git repo root (one level up from this crate's manifest dir,
/// which is `<repo_root>/autobuilder`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate dir has a parent")
        .to_path_buf()
}

fn run(dir: &Path, program: &str, args: &[&str]) -> std::process::Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn {program} {args:?} in {dir:?}: {e}"))
}

#[test]
#[ignore]
fn ac6_run_metrics_exits_zero_from_fresh_worktree() {
    let repo = repo_root();

    // A fresh worktree must live outside the main worktree's directory
    // tree; put it in a tempdir sibling.
    let holder = tempfile::tempdir().unwrap();
    let worktree = holder.path().join("autobuilder-fresh-wt");

    let head = run(&repo, "git", &["rev-parse", "HEAD"]);
    assert!(head.status.success(), "git rev-parse HEAD failed");
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    let add = run(
        &repo,
        "git",
        &[
            "worktree",
            "add",
            "--detach",
            worktree.to_str().unwrap(),
            &head_sha,
        ],
    );
    assert!(
        add.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    // Ensure the worktree is always removed, even on assertion panic.
    struct Cleanup<'a> {
        repo: &'a Path,
        worktree: &'a Path,
    }
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = Command::new("git")
                .args([
                    "worktree",
                    "remove",
                    "--force",
                    self.worktree.to_str().unwrap(),
                ])
                .current_dir(self.repo)
                .output();
        }
    }
    let _cleanup = Cleanup {
        repo: &repo,
        worktree: &worktree,
    };

    // A fresh checkout has no target/ dir at all — build the binary
    // run-metrics.sh looks for, exactly as CI would before a gate run.
    let crate_dir = worktree.join("autobuilder");
    let build = run(&crate_dir, "cargo", &["build", "--release", "--bin", "autobuilder"]);
    assert!(
        build.status.success(),
        "cargo build --release --bin autobuilder failed in fresh worktree:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let script = worktree.join("scripts/run-metrics.sh");
    assert!(script.is_file(), "run-metrics.sh missing from fresh worktree checkout");

    let metrics = run(
        &worktree,
        "bash",
        &[script.to_str().unwrap(), worktree.to_str().unwrap()],
    );
    assert!(
        metrics.status.success(),
        "run-metrics.sh exited non-zero from a fresh worktree:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&metrics.stdout),
        String::from_utf8_lossy(&metrics.stderr)
    );
}
