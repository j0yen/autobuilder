//! Hermetic integration tests for `autobuilder publish` (Stage 6).
//!
//! One test per MUST acceptance criterion (ACs 1–9 from
//! `PRD-autobuilder-publish.md`). All tests are network-free: the
//! `wm-publish` / `wm-push` wrappers are replaced with stub scripts placed on
//! a test-local `$PATH` that record their args to a file and return a canned
//! result. `REPOS.md` is resolved via a per-test `$WINTERMUTE_HOME`. AC10
//! (live publish) is a documented `#[ignore]` test.
//!
//! The binary is exercised as a black box via `CARGO_BIN_EXE_autobuilder`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown, clippy::too_many_lines)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn autobuilder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_autobuilder"))
}

fn fixture_card() -> String {
    fs::read_to_string("tests/fixtures/publish/intent-card.json").unwrap()
}

/// A fully wired hermetic publish environment.
struct Env {
    _tmp: TempDir,
    project: PathBuf,
    bin_dir: PathBuf,
    wintermute_home: PathBuf,
    publish_log: PathBuf,
    push_log: PathBuf,
}

/// Run a git command in `dir`, asserting success.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Build a hermetic environment:
/// - temp project with `agent/intent-card.json` (the fixture, slug overridable)
/// - a real git repo with a stale `main` baseline then a working branch
///   `autobuilder/<slug>`
/// - stub `wm-publish` / `wm-push` on a test-local bin dir
/// - a `$WINTERMUTE_HOME` containing a fixture REPOS.md
///
/// `publish_exit` / `publish_stdout` control the stub wm-publish behavior so
/// idempotency (already-exists) can be exercised.
fn make_env(slug: &str, on_working_branch: bool, publish_exit: i32, publish_stdout: &str) -> Env {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let project = root.join("project");
    let agent = project.join("agent");
    fs::create_dir_all(&agent).unwrap();

    // intent-card (rewrite the slug so wm-publish allow-list / regex is moot —
    // the stub doesn't check, but the receipt/url should reflect the slug).
    let mut card = fixture_card();
    card = card.replace("publish-fixture", slug);
    fs::write(agent.join("intent-card.json"), card).unwrap();

    // git repo: stale `main` baseline, then a working branch.
    git(&project, &["init", "-q"]);
    git(&project, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    fs::write(project.join("baseline.txt"), "iter-0 scaffold baseline\n").unwrap();
    git(&project, &["add", "."]);
    git(
        &project,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.com",
            "commit",
            "-q",
            "-m",
            "iter-0 scaffold baseline",
        ],
    );
    if on_working_branch {
        git(&project, &["checkout", "-q", "-b", &format!("autobuilder/{slug}")]);
        // a working commit on the autobuilder branch
        fs::write(project.join("src.txt"), "work\n").unwrap();
        git(&project, &["add", "."]);
        git(
            &project,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.com",
                "commit",
                "-q",
                "-m",
                "iter-1 work",
            ],
        );
    }

    // stub wrappers on a test-local PATH.
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let publish_log = root.join("wm-publish.log");
    let push_log = root.join("wm-push.log");

    write_stub(
        &bin_dir.join("wm-publish"),
        &publish_log,
        publish_exit,
        publish_stdout,
    );
    write_stub(&bin_dir.join("wm-push"), &push_log, 0, "wm-push: ok\n");

    // $WINTERMUTE_HOME with a fixture REPOS.md.
    let wintermute_home = root.join("wintermute");
    fs::create_dir_all(&wintermute_home).unwrap();
    fs::write(
        wintermute_home.join("REPOS.md"),
        "# Wintermute ecosystem repos\n\n## Pipeline / meta\n\n| Repo | Binary | What it does |\n|---|---|---|\n",
    )
    .unwrap();

    Env {
        _tmp: tmp,
        project,
        bin_dir,
        wintermute_home,
        publish_log,
        push_log,
    }
}

/// Write a stub wrapper that appends its args to `log_path` and exits `exit`.
fn write_stub(path: &Path, log_path: &Path, exit: i32, stdout: &str) {
    let script = format!(
        "#!/bin/sh\necho \"$@\" >> \"{log}\"\nprintf '%s' '{out}'\nexit {exit}\n",
        log = log_path.display(),
        out = stdout.replace('\'', "'\\''"),
    );
    fs::write(path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }
}

/// Invoke `autobuilder publish` against the env with the given extra args.
fn run_publish(env: &Env, extra: &[&str]) -> std::process::Output {
    let path_var = format!(
        "{}:{}",
        env.bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut args = vec!["publish", "--project"];
    let project = env.project.to_string_lossy().into_owned();
    args.push(&project);
    args.extend_from_slice(extra);
    autobuilder()
        .args(&args)
        .env("PATH", path_var)
        .env("WINTERMUTE_HOME", &env.wintermute_home)
        // Hermetic: the default-owner assertions must not see an ambient
        // AUTOBUILDER_GH_OWNER from the developer's shell. Tests that exercise a
        // custom owner pass `--owner` explicitly instead.
        .env_remove("AUTOBUILDER_GH_OWNER")
        .output()
        .unwrap()
}

type FileStat = (PathBuf, std::time::SystemTime, u64);

fn walk(dir: &Path, out: &mut Vec<FileStat>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let md = e.metadata().unwrap();
        if md.is_dir() {
            walk(&p, out);
        } else {
            out.push((p, md.modified().unwrap(), md.len()));
        }
    }
}

/// Recursively collect (path, mtime, len) for every file under `dir`.
fn snapshot(dir: &Path) -> Vec<FileStat> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// AC1 — `publish --help` lists the subcommand and all flags.
// ---------------------------------------------------------------------------
#[test]
fn ac1_help_lists_subcommand_and_flags() {
    let out = autobuilder().args(["publish", "--help"]).output().unwrap();
    assert!(out.status.success(), "publish --help should exit 0");
    let help = String::from_utf8_lossy(&out.stdout);
    for flag in [
        "--project",
        "--slug",
        "--visibility",
        "--license-year",
        "--category",
        "--force",
        "--dry-run",
    ] {
        assert!(help.contains(flag), "help missing {flag}; got: {help}");
    }

    // And the top-level help lists `publish`.
    let top = autobuilder().arg("--help").output().unwrap();
    let top_help = String::from_utf8_lossy(&top.stdout);
    assert!(top_help.contains("publish"), "top-level help missing publish");
}

// ---------------------------------------------------------------------------
// AC2 — `--dry-run` prints the ordered plan, makes zero writes + zero network.
// ---------------------------------------------------------------------------
#[test]
fn ac2_dry_run_no_writes_no_network() {
    let env = make_env("ac2-slug", true, 0, "wm-publish: created\n");
    let before = snapshot(&env.project);
    let out = run_publish(&env, &["--license-year", "2026", "--dry-run"]);
    assert!(out.status.success(), "dry-run should exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // ordered plan mentions each step
    for needle in ["README", "LICENSE", "branch", "wm-publish", "wm-push", "REPOS.md"] {
        assert!(stdout.contains(needle), "plan missing {needle}: {stdout}");
    }
    // zero filesystem writes
    let after = snapshot(&env.project);
    assert_eq!(before, after, "dry-run mutated the project tree");
    // zero network: no wrapper invoked
    assert!(!env.publish_log.exists(), "wm-publish was invoked in dry-run");
    assert!(!env.push_log.exists(), "wm-push was invoked in dry-run");
}

// ---------------------------------------------------------------------------
// AC3 — README has root_motivation overview + each MUST AC as a list item.
// ---------------------------------------------------------------------------
#[test]
fn ac3_readme_contents() {
    let env = make_env("ac3-slug", true, 0, "wm-publish: created\n");
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(out.status.success(), "publish failed: {}", String::from_utf8_lossy(&out.stderr));
    let readme = fs::read_to_string(env.project.join("README.md")).unwrap();
    assert!(
        readme.contains("deterministic fixture slice"),
        "README missing root_motivation overview: {readme}"
    );
    assert!(readme.contains("AC1"), "README missing AC1");
    assert!(readme.contains("AC2"), "README missing AC2");
    // SHOULD-level AC3 must not be listed in the MUST list.
    assert!(
        !readme.contains("**AC3**"),
        "README incorrectly listed SHOULD-level AC3"
    );
}

// ---------------------------------------------------------------------------
// AC4 — LICENSE-MIT + LICENSE-APACHE written with holder "Joe Yen" + year.
// ---------------------------------------------------------------------------
#[test]
fn ac4_licenses_written_with_holder_and_year() {
    let env = make_env("ac4-slug", true, 0, "wm-publish: created\n");
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(out.status.success());
    let mit = fs::read_to_string(env.project.join("LICENSE-MIT")).unwrap();
    let apache = fs::read_to_string(env.project.join("LICENSE-APACHE")).unwrap();
    assert!(mit.contains("Joe Yen") && mit.contains("2026"), "MIT bad: {mit}");
    assert!(apache.contains("Joe Yen") && apache.contains("2026"), "Apache bad: {apache}");
}

// ---------------------------------------------------------------------------
// AC5 — from autobuilder/<slug>, after a non-dry-run publish the branch is
//        `main` and the stale `main` baseline is gone (renamed, not added).
// ---------------------------------------------------------------------------
#[test]
fn ac5_branch_normalized_to_main() {
    let env = make_env("ac5-slug", true, 0, "wm-publish: created\n");
    // sanity: we start on the autobuilder branch
    assert_eq!(
        git_stdout(&env.project, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "autobuilder/ac5-slug"
    );
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(out.status.success(), "publish failed: {}", String::from_utf8_lossy(&out.stderr));
    // now on main
    assert_eq!(
        git_stdout(&env.project, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "main"
    );
    // the autobuilder/<slug> branch no longer exists (it was renamed away)
    let branches = git_stdout(&env.project, &["branch", "--list"]);
    assert!(
        !branches.contains("autobuilder/ac5-slug"),
        "stale autobuilder branch still present: {branches}"
    );
}

// ---------------------------------------------------------------------------
// AC6 — invokes wm-publish + wm-push from $PATH; does NOT call gh repo
//        create / git push directly. (Runtime PATH-shim record + static grep.)
// ---------------------------------------------------------------------------
#[test]
fn ac6_shells_out_to_wrappers_not_gh_or_push() {
    let env = make_env("ac6-slug", true, 0, "wm-publish: created\n");
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(out.status.success(), "publish failed: {}", String::from_utf8_lossy(&out.stderr));
    // both wrappers recorded an invocation
    let pub_args = fs::read_to_string(&env.publish_log).unwrap();
    let push_args = fs::read_to_string(&env.push_log).unwrap();
    assert!(pub_args.contains("--slug ac6-slug"), "wm-publish not invoked with slug: {pub_args}");
    assert!(push_args.contains("--slug ac6-slug"), "wm-push not invoked with slug: {push_args}");

    // static grep: the publish source must not call gh repo create / git push.
    let src = fs::read_to_string("src/publish.rs").unwrap();
    // strip comment lines so the doc-comment description of the boundary
    // (which legitimately mentions the forbidden strings) doesn't trip the grep.
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("gh repo create"),
        "publish source contains a direct `gh repo create` call"
    );
    assert!(
        !code.contains("\"push\""),
        "publish source contains a direct git `push` argument"
    );
    assert!(
        !code.contains("repo create"),
        "publish source contains a direct gh repo-create call"
    );
}

// ---------------------------------------------------------------------------
// AC7 — idempotent create: stub wm-publish reports already-exists → publish
//        does not error, skips creation, still re-syncs README/REPOS.md.
// ---------------------------------------------------------------------------
#[test]
fn ac7_idempotent_when_repo_already_exists() {
    let env = make_env("ac7-slug", true, 1, "wm-publish: name already exists on this account\n");
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(
        out.status.success(),
        "publish should succeed when repo already exists: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // README + REPOS.md still synced
    assert!(env.project.join("README.md").exists(), "README not synced on idempotent run");
    let repos = fs::read_to_string(env.wintermute_home.join("REPOS.md")).unwrap();
    assert!(repos.contains("ac7-slug"), "REPOS.md not synced on idempotent run");
    // receipt records preexisting=true
    let receipt = fs::read_to_string(
        env.project.join("target/autobuilder/receipts/publish-receipt.json"),
    )
    .unwrap();
    assert!(receipt.contains("\"repo_preexisting\": true"), "receipt did not record preexisting: {receipt}");
}

// ---------------------------------------------------------------------------
// AC8 — REPOS.md gains exactly one line for the slug; re-run does not dupe.
// ---------------------------------------------------------------------------
#[test]
fn ac8_repos_md_single_idempotent_line() {
    let env = make_env("ac8-slug", true, 0, "wm-publish: created\n");
    let out1 = run_publish(&env, &["--license-year", "2026", "--category", "runtime"]);
    assert!(out1.status.success());
    // re-run: branch already main, repo "created" again (stub returns success);
    // REPOS.md must not gain a second line.
    let out2 = run_publish(&env, &["--license-year", "2026", "--category", "runtime"]);
    assert!(out2.status.success(), "second publish failed: {}", String::from_utf8_lossy(&out2.stderr));
    let repos = fs::read_to_string(env.wintermute_home.join("REPOS.md")).unwrap();
    let count = repos.matches("github.com/j0yen/ac8-slug)").count();
    assert_eq!(count, 1, "expected exactly one REPOS.md line, found {count}:\n{repos}");
}

// ---------------------------------------------------------------------------
// AC9 — publish-receipt.json written and matches the publish-receipt/v1 shape.
// ---------------------------------------------------------------------------
#[test]
fn ac9_receipt_written_and_conforms() {
    let env = make_env("ac9-slug", true, 0, "wm-publish: created\n");
    let out = run_publish(&env, &["--license-year", "2026"]);
    assert!(out.status.success(), "publish failed: {}", String::from_utf8_lossy(&out.stderr));
    let path = env.project.join("target/autobuilder/receipts/publish-receipt.json");
    let text = fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let obj = v.as_object().unwrap();
    for field in [
        "schema",
        "slug",
        "repo_url",
        "branch",
        "head_sha",
        "readme_generated",
        "license_generated",
        "repos_md_updated",
        "repo_preexisting",
        "dry_run",
        "verdict",
        "captured_at",
        "receipt_digest",
    ] {
        assert!(obj.contains_key(field), "receipt missing field {field}");
    }
    assert_eq!(obj["schema"], "publish-receipt/v1");
    assert_eq!(obj["branch"], "main");
    assert_eq!(obj["verdict"], "published");
    assert_eq!(obj["slug"], "ac9-slug");
    assert_eq!(obj["repo_url"], "https://github.com/j0yen/ac9-slug");
    assert_eq!(obj["dry_run"], false);
    assert!(obj["receipt_digest"].as_str().unwrap().starts_with("sha256:"));
}

// ---------------------------------------------------------------------------
// --owner — overrides the default owner in the stamped repo URL / README /
//           REPOS.md. Default (no flag, no env) stays `j0yen`; covered by AC8/AC9.
// ---------------------------------------------------------------------------
#[test]
fn owner_flag_overrides_default_in_url_readme_and_repos_md() {
    let env = make_env("owner-slug", true, 0, "wm-publish: created\n");
    let out = run_publish(
        &env,
        &["--license-year", "2026", "--owner", "joeyen-atscale"],
    );
    assert!(out.status.success(), "publish failed: {}", String::from_utf8_lossy(&out.stderr));

    // receipt repo_url carries the custom owner
    let receipt = fs::read_to_string(
        env.project.join("target/autobuilder/receipts/publish-receipt.json"),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(v["repo_url"], "https://github.com/joeyen-atscale/owner-slug");

    // README install line carries the custom owner, never the j0yen default
    let readme = fs::read_to_string(env.project.join("README.md")).unwrap();
    assert!(
        readme.contains("https://github.com/joeyen-atscale/owner-slug"),
        "README missing custom owner:\n{readme}"
    );
    assert!(!readme.contains("j0yen"), "README leaked default owner:\n{readme}");

    // REPOS.md row carries the custom owner
    let repos = fs::read_to_string(env.wintermute_home.join("REPOS.md")).unwrap();
    assert!(
        repos.contains("github.com/joeyen-atscale/owner-slug)"),
        "REPOS.md missing custom owner:\n{repos}"
    );
}

// ---------------------------------------------------------------------------
// AC10 — DEFERRED (live/network). A real end-to-end publish creates the
//         public <owner>/<slug> repo and pushes main. Cannot run hermetically.
// ---------------------------------------------------------------------------
#[test]
#[ignore = "AC10: live-network — requires real gh auth + a genuinely new slug; verified once out-of-band"]
fn ac10_live_publish_creates_real_repo() {
    // Intentionally empty: documented live-verification criterion, exercised
    // once supervised through the subcommand against a real fresh slice.
}
