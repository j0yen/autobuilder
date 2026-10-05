//! Integration tests for `autobuilder rollback-plan`'s squash-range-revert
//! hotfix (revert-commits mode).
//!
//! The daemon lands every branch as one squash commit, so the real future
//! revert is `git revert <squash-sha>` against the whole `base..HEAD`
//! range, not a per-commit revert of each original commit. These tests
//! build real git fixtures (temp dirs + `git init` + commits, same pattern
//! as `tests/rollback_tag_ac1_redeploy_pass_with_merges.rs`) and exercise
//! the binary as a black box via `CARGO_BIN_EXE_autobuilder`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

/// Run a git command in `dir`, asserting success.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap();
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// Run `autobuilder rollback-plan --project <dir> --base <base> [extra]`.
fn run_rollback_plan(dir: &Path, base: &str, extra: &[&str]) -> std::process::Output {
    let project = dir.to_string_lossy().into_owned();
    let mut args = vec!["rollback-plan", "--project", &project, "--base", base];
    args.extend_from_slice(extra);
    autobuilder().args(&args).output().unwrap()
}

fn receipt(dir: &Path) -> serde_json::Value {
    let bytes = fs::read(dir.join("target/autobuilder/receipts/rollback-plan.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn rollback_md(dir: &Path) -> String {
    fs::read_to_string(dir.join("target/autobuilder/rollback.md")).unwrap()
}

/// Build the stacked-commits fixture shared by tests (a) and (c):
///
/// `fork` (f.txt = line1/line2/line3) on branch `main`, then on a `feature`
/// branch: `c1` edits line2, `c2` edits line3 (reverting `c1` alone against
/// the post-`c2` HEAD conflicts in this tiny file — same hunk, no context
/// separation — even though the *net* base..HEAD diff reverts cleanly),
/// `c3` an unrelated file add. HEAD ends on `feature`; `main` stays at
/// `fork` (a direct ancestor of HEAD). No `agent/deploy-manifest.toml`, so
/// this crate resolves to `revert-commits` mode (the default).
fn stacked_commits_fixture() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write(dir, "f.txt", "line1\nline2\nline3\n");
    commit(dir, "fork");
    git(dir, &["checkout", "-q", "-b", "feature"]);
    write(dir, "f.txt", "line1\nCHANGED-A\nline3\n");
    commit(dir, "c1: change line2");
    write(dir, "f.txt", "line1\nCHANGED-A\nCHANGED-B\n");
    commit(dir, "c2: change line3");
    write(dir, "notes.txt", "unrelated\n");
    commit(dir, "c3: unrelated file");
    tmp
}

// ---------------------------------------------------------------------------
// (a) Range revert is clean even though an interior commit is not
// individually revertable → verdict `pass`, `range_revertable: true`.
// ---------------------------------------------------------------------------
#[test]
fn squash_strategy_passes_on_range_revert_despite_individual_commit_block() {
    let tmp = stacked_commits_fixture();
    let dir = tmp.path();

    let out = run_rollback_plan(dir, "main", &[]);
    let doc = receipt(dir);

    assert!(
        out.status.success(),
        "expected pass (default --strategy squash); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(doc["verdict"], "pass");
    assert_eq!(doc["range_revertable"], true);
    assert_eq!(doc["land_strategy"], "squash");
    // The fixture is specifically designed so at least one commit in the
    // stack is NOT individually revertable, confirming the range-level
    // check -- not the (still-failing) per-commit rule -- is what passed it.
    assert!(
        doc["blocking_count"].as_u64().unwrap() > 0,
        "fixture should contain an individually-non-revertable commit; receipt: {doc}"
    );

    let md = rollback_md(dir);
    assert!(md.contains("## Squash revert"), "rollback.md missing Squash revert section:\n{md}");
    assert!(md.contains("clean"), "rollback.md should report a clean range revert:\n{md}");
}

// ---------------------------------------------------------------------------
// (b) `base` has drifted past the branch's fork point with a commit that
// conflicts with the range → verdict `block`, `range_revertable: false`.
// ---------------------------------------------------------------------------
#[test]
fn squash_strategy_blocks_when_drifted_base_conflicts_with_range() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    write(dir, "f.txt", "line1\nline2\nline3\n");
    commit(dir, "fork");

    // feature branch: same stacked edits as the (a) fixture.
    git(dir, &["checkout", "-q", "-b", "feature"]);
    write(dir, "f.txt", "line1\nCHANGED-A\nline3\n");
    commit(dir, "c1: change line2");
    write(dir, "f.txt", "line1\nCHANGED-A\nCHANGED-B\n");
    commit(dir, "c2: change line3");
    write(dir, "notes.txt", "unrelated\n");
    commit(dir, "c3: unrelated file");

    // main moves on, independently, with a commit touching the same line2
    // that the feature range also touches.
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "f.txt", "line1\nMAIN-CHANGED\nline3\n");
    commit(dir, "m1: main-side edit of the same line");

    // HEAD = feature tip; --base = main (now past the fork point).
    git(dir, &["checkout", "-q", "feature"]);

    let out = run_rollback_plan(dir, "main", &[]);
    let doc = receipt(dir);

    assert!(
        !out.status.success(),
        "expected block (drifted base conflicts with the range); receipt: {doc}"
    );
    assert_eq!(doc["verdict"], "block");
    assert_eq!(doc["range_revertable"], false);
    assert_eq!(doc["land_strategy"], "squash");

    let md = rollback_md(dir);
    assert!(
        md.contains("CONFLICTS"),
        "rollback.md should report a conflicting range revert:\n{md}"
    );
    assert!(md.contains("f.txt"), "rollback.md should list the conflicting file:\n{md}");
}

// ---------------------------------------------------------------------------
// (c) Same fixture as (a), but `--strategy merge` reproduces today's
// per-commit-only verdict (ignores `range_revertable`) → `block`.
// ---------------------------------------------------------------------------
#[test]
fn merge_strategy_ignores_range_revertable_and_reproduces_original_block() {
    let tmp = stacked_commits_fixture();
    let dir = tmp.path();

    let out = run_rollback_plan(dir, "main", &["--strategy", "merge"]);
    let doc = receipt(dir);

    assert!(
        !out.status.success(),
        "expected block under --strategy merge (per-commit rule only); receipt: {doc}"
    );
    assert_eq!(doc["verdict"], "block");
    assert_eq!(doc["land_strategy"], "merge");
    // range_revertable is still computed/reported informationally, but must
    // not have been consulted for the verdict.
    assert_eq!(doc["range_revertable"], true);
    assert!(doc["blocking_count"].as_u64().unwrap() > 0);
}
