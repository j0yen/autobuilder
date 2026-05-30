//! Stage 6 — Publish a finished slice as its own `github.com/j0yen/<slug>` repo.
//!
//! Codifies the four manual steps documented in the skill's `SKILL.md`
//! §"Stage 6 — Publish" into a deterministic, idempotent, re-runnable
//! subcommand. The seven design steps (resolve → README/LICENSE → branch
//! normalize → commit → create+push → REPOS.md → receipt) mirror
//! `PRD-autobuilder-publish.md`.
//!
//! Safety boundary (PRD AC6): this module **shells out to the `wm-publish`
//! and `wm-push` wrappers resolved from `$PATH`** for the remote-create and
//! push steps. It must NEVER invoke `gh repo create` or `git push` directly
//! — those wrappers own the slug/allow-list/remote-URL/fast-forward checks
//! that keep publishing honest. A static test greps this source to enforce
//! the absence of those forbidden direct calls.
//!
//! Reproducibility (PRD): the LICENSE year is always supplied explicitly via
//! `--license-year` (or `$AUTOBUILDER_DATE`'s leading year); the publish path
//! never reads a wall clock to stamp a year, so a given input reproduces a
//! given output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::Args as ClapArgs;
use serde::Serialize;
use serde_json::Value;

use crate::intake;
use crate::receipt;

/// Repository visibility for the created remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Visibility {
    /// Public repository (default).
    Public,
    /// Private repository.
    Private,
}

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Project root. Must contain `agent/intent-card.json`.
    #[arg(long, default_value = ".")]
    pub project: PathBuf,

    /// Repo slug. Defaults to the intent-card `intent_slug`.
    #[arg(long)]
    pub slug: Option<String>,

    /// GitHub owner (user or org) the slice is published under. Falls back to
    /// `$AUTOBUILDER_GH_OWNER`, then the default `j0yen`. The actual remote is
    /// still created by the `wm-publish`/`wm-push` wrappers; this controls the
    /// owner stamped into repo URLs, the README install line, and `REPOS.md`.
    #[arg(long)]
    pub owner: Option<String>,

    /// Repository visibility.
    #[arg(long, value_enum, default_value_t = Visibility::Public)]
    pub visibility: Visibility,

    /// Year stamped into the LICENSE files. Falls back to the leading year of
    /// `$AUTOBUILDER_DATE` when omitted; never read from a wall clock (keeps
    /// runs reproducible). Required if neither is present.
    #[arg(long)]
    pub license_year: Option<String>,

    /// `REPOS.md` category (pipeline | runtime | memory | session | artist | ...).
    #[arg(long, default_value = "pipeline")]
    pub category: String,

    /// Regenerate README/LICENSE even if they already exist.
    #[arg(long)]
    pub force: bool,

    /// Print the plan; make zero filesystem writes and zero network calls.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)] // the publish-receipt/v1 shape is fixed by the PRD; these flags are the attested facts
struct ReceiptDoc {
    schema: &'static str,
    slug: String,
    repo_url: String,
    branch: &'static str,
    head_sha: String,
    readme_generated: bool,
    license_generated: bool,
    repos_md_updated: bool,
    repo_preexisting: bool,
    dry_run: bool,
    verdict: &'static str,
    captured_at: String,
    receipt_digest: String,
}

/// A single MUST-level acceptance criterion lifted from the intent-card.
struct MustAc {
    id: String,
    description: String,
}

#[allow(clippy::needless_pass_by_value)] // owned `Args` matches the clap-dispatched subcommand contract
#[allow(clippy::too_many_lines)] // single linear Stage-6 pipeline; splitting would obscure the step order
pub(crate) fn run(args: Args) -> Result<()> {
    let project = args
        .project
        .canonicalize()
        .with_context(|| format!("project path not found: {}", args.project.display()))?;

    // --- Step 1: Resolve ---------------------------------------------------
    let card_path = project.join("agent/intent-card.json");
    let card = intake::validate_file(&card_path).with_context(|| {
        format!("intent-card validation failed for {}", card_path.display())
    })?;
    let card_obj = card
        .as_object()
        .ok_or_else(|| anyhow!("intent-card is not a JSON object"))?;

    let slug = match args.slug.clone() {
        Some(s) => s,
        None => card_obj
            .get("intent_slug")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow!("no --slug given and intent-card has no `intent_slug`")
            })?
            .to_owned(),
    };

    let root_motivation = card_obj
        .get("root_motivation")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let must_acs = collect_must_acs(card_obj);

    let license_year = resolve_license_year(args.license_year.as_deref())?;

    let owner = resolve_owner(args.owner.as_deref());
    let repo_url = format!("https://github.com/{owner}/{slug}");

    // --- dry-run: print the plan, make zero writes / zero network ----------
    if args.dry_run {
        println!("publish (dry-run): slug={slug} owner={owner} repo={repo_url} visibility={:?}", args.visibility);
        println!("plan:");
        println!("  1. README.md + LICENSE-MIT + LICENSE-APACHE (year {license_year}, holder \"Joe Yen\")");
        println!("  2. branch normalize: autobuilder/{slug} -> main (delete stale main)");
        println!("  3. commit: \"Prep for standalone distribution: README + dual MIT/Apache-2.0 license\"");
        println!("  4. create remote via wm-publish --slug {slug}{}", match args.visibility {
            Visibility::Private => " (private)",
            Visibility::Public => "",
        });
        println!("  5. push main via wm-push --slug {slug}");
        println!("  6. REPOS.md: ensure one line for {slug} under [{}]", args.category);
        println!("  7. receipt: target/autobuilder/receipts/publish-receipt.json");
        println!("publish (dry-run): no files written, no network calls made.");
        return Ok(());
    }

    // --- Step 2: README + LICENSE -----------------------------------------
    let readme_path = project.join("README.md");
    let mit_path = project.join("LICENSE-MIT");
    let apache_path = project.join("LICENSE-APACHE");

    let readme_generated = args.force || !readme_path.exists();
    if readme_generated {
        fs::write(&readme_path, render_readme(&owner, &slug, &root_motivation, &must_acs))
            .with_context(|| format!("failed to write {}", readme_path.display()))?;
    }
    let license_generated = args.force || !mit_path.exists() || !apache_path.exists();
    if license_generated {
        fs::write(&mit_path, render_license_mit(&license_year))
            .with_context(|| format!("failed to write {}", mit_path.display()))?;
        fs::write(&apache_path, render_license_apache(&license_year))
            .with_context(|| format!("failed to write {}", apache_path.display()))?;
    }

    // --- Step 3: Branch normalize -----------------------------------------
    normalize_branch(&project, &slug)?;

    // --- Step 4: Commit ----------------------------------------------------
    commit_prep(&project)?;
    let head_sha = git_rev_parse(&project, "HEAD")?;

    // --- Step 5: Create + push (via wrappers) ------------------------------
    let repo_preexisting = wm_publish(&project, &slug, &root_motivation, args.visibility)?;
    wm_push(&project, &slug)?;

    // --- Step 6: REPOS.md --------------------------------------------------
    let repos_md_updated = ensure_repos_md_line(&owner, &slug, &root_motivation, &args.category)?;

    // --- Step 7: Receipt ---------------------------------------------------
    let doc = ReceiptDoc {
        schema: "publish-receipt/v1",
        slug: slug.clone(),
        repo_url: repo_url.clone(),
        branch: "main",
        head_sha: head_sha.clone(),
        readme_generated,
        license_generated,
        repos_md_updated,
        repo_preexisting,
        dry_run: false,
        verdict: "published",
        captured_at: receipt::now_rfc3339()?,
        receipt_digest: String::new(),
    };
    let value = serde_json::to_value(&doc)?;
    let receipt_path = project.join("target/autobuilder/receipts/publish-receipt.json");
    receipt::write(&receipt_path, value)?;

    println!(
        "publish: slug={slug} repo={repo_url} branch=main head={head_sha} preexisting={repo_preexisting} verdict=published"
    );
    Ok(())
}

/// Collect MUST-level acceptance criteria from an intent-card object.
fn collect_must_acs(card_obj: &serde_json::Map<String, Value>) -> Vec<MustAc> {
    let mut out = Vec::new();
    if let Some(arr) = card_obj.get("acceptance_criteria").and_then(Value::as_array) {
        for ac in arr {
            let Some(o) = ac.as_object() else { continue };
            if o.get("level").and_then(Value::as_str) != Some("MUST") {
                continue;
            }
            let id = o.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
            let description = o
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            out.push(MustAc { id, description });
        }
    }
    out
}

/// Resolve the license year: explicit `--license-year`, else the leading
/// year of `$AUTOBUILDER_DATE`. Never reads a wall clock.
fn resolve_license_year(explicit: Option<&str>) -> Result<String> {
    if let Some(y) = explicit {
        let y = y.trim();
        if !(y.len() == 4 && y.chars().all(|c| c.is_ascii_digit())) {
            return Err(anyhow!("--license-year must be a 4-digit year, got {y:?}"));
        }
        return Ok(y.to_owned());
    }
    if let Ok(date) = std::env::var("AUTOBUILDER_DATE") {
        if let Some(year) = date.split('-').next() {
            let year = year.trim();
            if year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()) {
                return Ok(year.to_owned());
            }
        }
    }
    Err(anyhow!(
        "license year required: pass --license-year YYYY or set $AUTOBUILDER_DATE (never read from a clock, for reproducibility)"
    ))
}

/// Resolve the publish owner: explicit `--owner`, else `$AUTOBUILDER_GH_OWNER`,
/// else the historical default `j0yen`. Kept org-agnostic so the same binary
/// can publish to a personal account or an org without a rebuild.
fn resolve_owner(explicit: Option<&str>) -> String {
    if let Some(o) = explicit {
        let o = o.trim();
        if !o.is_empty() {
            return o.to_owned();
        }
    }
    if let Ok(env_owner) = std::env::var("AUTOBUILDER_GH_OWNER") {
        let env_owner = env_owner.trim().to_owned();
        if !env_owner.is_empty() {
            return env_owner;
        }
    }
    "j0yen".to_owned()
}

fn render_readme(owner: &str, slug: &str, root_motivation: &str, must_acs: &[MustAc]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {slug}\n\n"));
    s.push_str(root_motivation);
    s.push_str("\n\n## Acceptance criteria\n\n");
    if must_acs.is_empty() {
        s.push_str("_No MUST-level acceptance criteria declared._\n");
    } else {
        for ac in must_acs {
            s.push_str(&format!("- **{}** — {}\n", ac.id, ac.description));
        }
    }
    s.push_str("\n## Install\n\n");
    s.push_str(&format!(
        "```sh\ncargo install --git https://github.com/{owner}/{slug}\n```\n",
    ));
    s.push_str("\n## License\n\n");
    s.push_str(
        "Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.\n",
    );
    s
}

fn render_license_mit(year: &str) -> String {
    format!(
        "MIT License\n\nCopyright (c) {year} Joe Yen\n\n\
Permission is hereby granted, free of charge, to any person obtaining a copy\n\
of this software and associated documentation files (the \"Software\"), to deal\n\
in the Software without restriction, including without limitation the rights\n\
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell\n\
copies of the Software, and to permit persons to whom the Software is\n\
furnished to do so, subject to the following conditions:\n\n\
The above copyright notice and this permission notice shall be included in all\n\
copies or substantial portions of the Software.\n\n\
THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\n\
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\n\
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\n\
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\n\
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\n\
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\n\
SOFTWARE.\n"
    )
}

fn render_license_apache(year: &str) -> String {
    // Standard Apache-2.0 header + the boilerplate appendix with the copyright
    // line stamped. The full license text body is referenced by URL to keep
    // the file compact while the holder/year line remains explicit.
    format!(
        "                                 Apache License\n\
                           Version 2.0, January 2004\n\
                        http://www.apache.org/licenses/\n\n\
Copyright {year} Joe Yen\n\n\
Licensed under the Apache License, Version 2.0 (the \"License\");\n\
you may not use this file except in compliance with the License.\n\
You may obtain a copy of the License at\n\n\
    http://www.apache.org/licenses/LICENSE-2.0\n\n\
Unless required by applicable law or agreed to in writing, software\n\
distributed under the License is distributed on an \"AS IS\" BASIS,\n\
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.\n\
See the License for the specific language governing permissions and\n\
limitations under the License.\n"
    )
}

/// Step 3 — if on `autobuilder/<slug>`, delete any stale local `main` baseline
/// and rename the working branch to `main`. No-op if already on `main`.
fn normalize_branch(project: &Path, slug: &str) -> Result<()> {
    let current = git_rev_parse_branch(project)?;
    if current == "main" {
        return Ok(());
    }
    let working = format!("autobuilder/{slug}");
    if current != working {
        // Not on the expected autobuilder branch and not on main; leave it be
        // but surface the situation rather than silently renaming an arbitrary
        // branch into main.
        return Err(anyhow!(
            "expected to be on `main` or `{working}`, but on `{current}`; refusing to rename"
        ));
    }
    // Delete a stale local `main` baseline if present (force; it is the iter-0
    // scaffold commit superseded by the autobuilder branch).
    if branch_exists(project, "main")? {
        run_git(project, &["branch", "-D", "main"])?;
    }
    run_git(project, &["branch", "-m", "main"])?;
    Ok(())
}

/// Step 4 — single prep commit using the Joe Yen identity via per-command
/// `-c` (never `.git/config`). If there is nothing to commit, no-op.
fn commit_prep(project: &Path) -> Result<()> {
    run_git(project, &["add", "README.md", "LICENSE-MIT", "LICENSE-APACHE"])?;
    // Only commit if the index has staged changes.
    let (code, _out, _err) =
        run_git_capturing(project, &["diff", "--cached", "--quiet"])?;
    if code == 0 {
        // Nothing staged — idempotent re-run; skip the commit.
        return Ok(());
    }
    run_git(
        project,
        &[
            "-c",
            "user.name=Joe Yen",
            "-c",
            "user.email=jyen.tech@gmail.com",
            "commit",
            "-m",
            "Prep for standalone distribution: README + dual MIT/Apache-2.0 license",
        ],
    )?;
    Ok(())
}

/// Step 5a — create the remote via the `wm-publish` wrapper resolved from
/// `$PATH`. Returns `true` if the wrapper reports the repo already exists
/// (treated as success, not an error, for idempotency — PRD AC7).
///
/// SAFETY BOUNDARY: this shells out to `wm-publish` and must never call
/// `gh repo create` directly.
fn wm_publish(
    project: &Path,
    slug: &str,
    description: &str,
    visibility: Visibility,
) -> Result<bool> {
    let desc = if description.is_empty() {
        slug.to_owned()
    } else {
        description.to_owned()
    };
    let mut cmd = Command::new("wm-publish");
    cmd.arg("--slug")
        .arg(slug)
        .arg("--description")
        .arg(&desc)
        .arg("--source")
        .arg(project);
    if visibility == Visibility::Private {
        cmd.arg("--private");
    }
    let output = cmd
        .output()
        .context("failed to spawn wm-publish from $PATH (is the wrapper installed?)")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    let already_exists = combined.contains("already exists") || combined.contains("name already exists");
    if output.status.success() {
        return Ok(already_exists);
    }
    if already_exists {
        // Non-zero but the failure is "repo already exists" — idempotent path.
        return Ok(true);
    }
    Err(anyhow!(
        "wm-publish failed (exit {:?}): {}",
        output.status.code(),
        stderr.trim()
    ))
}

/// Step 5b — push `main` via the `wm-push` wrapper resolved from `$PATH`.
///
/// SAFETY BOUNDARY: this shells out to `wm-push` and must never call
/// `git push` directly.
fn wm_push(project: &Path, slug: &str) -> Result<()> {
    let output = Command::new("wm-push")
        .arg("--slug")
        .arg(slug)
        .arg("--source")
        .arg(project)
        .arg("--branch")
        .arg("main")
        .output()
        .context("failed to spawn wm-push from $PATH (is the wrapper installed?)")?;
    if !output.status.success() {
        return Err(anyhow!(
            "wm-push failed (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// Step 6 — ensure exactly one `REPOS.md` line for `slug` under `category`.
/// Idempotent: appends if missing, leaves untouched if present. Returns
/// `true` if a line was appended.
fn ensure_repos_md_line(owner: &str, slug: &str, description: &str, category: &str) -> Result<bool> {
    let repos_md = resolve_repos_md()?;
    let text = fs::read_to_string(&repos_md)
        .with_context(|| format!("could not read REPOS.md at {}", repos_md.display()))?;

    // Idempotency check: a line already linking the slug's repo.
    let needle = format!("https://github.com/{owner}/{slug})");
    let alt_needle = format!("[{slug}]");
    if text.lines().any(|l| l.contains(&needle) || l.contains(&alt_needle)) {
        return Ok(false);
    }

    let desc = if description.is_empty() {
        slug.to_owned()
    } else {
        description.to_owned()
    };
    let line = format!(
        "| [{slug}](https://github.com/{owner}/{slug}) | `{slug}` | {desc} [category: {category}] |\n"
    );
    let mut new_text = text;
    if !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str(&line);
    fs::write(&repos_md, new_text)
        .with_context(|| format!("could not write REPOS.md at {}", repos_md.display()))?;
    Ok(true)
}

/// Resolve `REPOS.md` via `$WINTERMUTE_HOME`, falling back to
/// `$HOME/wintermute/REPOS.md`. Never hard-codes an absolute path in logic.
fn resolve_repos_md() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("WINTERMUTE_HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home).join("REPOS.md"));
        }
    }
    let home = std::env::var("HOME").context("neither $WINTERMUTE_HOME nor $HOME is set")?;
    Ok(PathBuf::from(home).join("wintermute").join("REPOS.md"))
}

fn git_rev_parse(project: &Path, refname: &str) -> Result<String> {
    Ok(run_git(project, &["rev-parse", refname])?.trim().to_owned())
}

fn git_rev_parse_branch(project: &Path) -> Result<String> {
    Ok(run_git(project, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_owned())
}

fn branch_exists(project: &Path, branch: &str) -> Result<bool> {
    let refname = format!("refs/heads/{branch}");
    let (code, _out, _err) =
        run_git_capturing(project, &["show-ref", "--verify", "--quiet", &refname])?;
    Ok(code == 0)
}

fn run_git(project: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn git {args:?}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_git_capturing(project: &Path, args: &[&str]) -> Result<(i32, String, String)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .with_context(|| format!("failed to spawn git {args:?}"))?;
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((code, stdout, stderr))
}
