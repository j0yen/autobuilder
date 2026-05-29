# PRD: atlas-core — the corpus, as a graph

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/atlas.md
**build_target:** rust-cli
**build_into:** /home/jsy/wintermute/atlas
**Depends on:** none
**Codename:** *atlas* — the queue is already a graph; this draws the nodes.

## TL;DR

107 PRDs, 24 vision docs, two manifests, and a 117-line REPOS.md describe
a single connected structure — vision owns PRDs, PRDs ship to repos — but
the structure lives only in the reader's head. atlas-core is the
substrate that makes it a queryable object: parse every PRD's frontmatter,
both skill manifests, and REPOS.md into one in-memory graph of typed
nodes (vision, prd, repo), and expose it via `atlas nodes` and
`atlas show <vision>`, every command offering `--format json`. This PRD
builds the model + parsers + read commands. Edges, divergence-lint, and
rendering are follow-on PRDs that extend this crate.

## Why this exists

Phase 1 evidence (2026-05-29, live this session):

- `ls ~/wintermute/autobuilder/PRD-*.md | wc -l` → **107**;
  `ls visions/*.md | wc -l` → **24**. No tool joins the two.
- Every PRD carries the join keys already: this fleet's own PRDs and
  `PRD-docket-core.md` open with `**Vision:** visions/<slug>.md` and a
  `**build_target:**`/`**build_into:**` line. The edge from PRD to vision
  is *written down* — just never read by a machine.
- `~/.claude/skills/build/state/manifest.json` (450 KB) holds per-PRD
  `output_repo_path`, `output_repo_url`, `iter_log`, `last_action`,
  `blockers` — the shipped-state of every PRD, verified live (top keys:
  `prds`, `blockers`, `budget_telemetry`, ...).
- `~/.claude/skills/dream/state/manifest.json` holds `visions.<slug>`
  with `prds_drafted[]`, `status`, `seed` — verified live (25 vision
  entries incl. a `_no_fleet_passes` sentinel).
- `~/wintermute/REPOS.md` (117 lines) is the shipped-repo index.
- The /dream SKILL.md itself states the end-state is "each PRD a node in
  a graph"; the dream manifest's *fulfilled* cross-reference is described
  as a per-invocation hand task. The data is rich and machine-readable;
  what's missing is the join.

A graph needs nodes before it needs edges. atlas-core builds the nodes.

## What this builds

A standalone Rust CLI published as `j0yen/atlas`, installed to
`~/.local/bin/atlas`. Mirrors the local toolkit shape (`recall`,
`ctrace`, `docket`).

**No persistent store.** atlas reads the source files fresh on each run
(they total <5 MB; cold parse must stay under ~200 ms — see AC7). It
never writes to the autobuilder corpus.

**Config / source resolution** (override via env for testing):
- PRDs + visions: `${ATLAS_AUTOBUILDER:-~/wintermute/autobuilder}`
- build manifest: `${ATLAS_BUILD_MANIFEST:-~/.claude/skills/build/state/manifest.json}`
- dream manifest: `${ATLAS_DREAM_MANIFEST:-~/.claude/skills/dream/state/manifest.json}`
- repos index: `${ATLAS_REPOS:-~/wintermute/REPOS.md}`

**Node model** (in-memory; serde-serializable for `--format json`):

| node kind | id            | fields                                                        |
|-----------|---------------|---------------------------------------------------------------|
| `vision`  | slug          | `path`, `status` (from dream manifest), `prds_drafted[]`, `seed` |
| `prd`     | filename      | `title`, `vision` slug, `build_target`, `build_into`, `status` (Draft/in-flight/shipped), `repo_url`, `repo_path` |
| `prd_status` | derived    | `drafted` (no manifest entry / no action), `in_flight` (manifest entry, not shipped), `shipped` (iter_log/last_action says shipped or repo path exists) |

**Parsers** (each isolated + unit-tested against fixtures):
- PRD frontmatter: tolerant line scan for `**Vision:**`, `**build_target:**`,
  `**build_into:**`, `**Status:**`, and the `# PRD: <slug> — <title>` H1.
  Must not choke on PRDs lacking a field (older PRDs omit `build_into`).
- dream manifest: serde into `{visions: map<slug, {path, status, prds_drafted, seed}>}`; ignore the `_no_fleet_passes` sentinel key.
- build manifest: serde into `{prds: [{path, output_repo_path, output_repo_url, last_action, iter_log, blockers}]}`; join to PRD nodes by basename of `path`.
- REPOS.md: parse repo names/URLs from its markdown list.

**Commands:**
- `atlas nodes [--kind vision|prd|repo] [--format text|json]` — list all
  nodes of a kind (default: all), one per line in text mode.
- `atlas show <vision-slug> [--format text|json]` — the vision's PRDs,
  each with derived status and repo URL if shipped; unknown slug → exit 2.
- `atlas --version`, `atlas --help`.

**Deps:** `serde`/`serde_json`, a TOML-free frontmatter scan (hand-rolled,
no new dep), `clap` for args, `anyhow`. No SQLite (no store). MSRV 1.85,
no let-chains.

## Acceptance criteria

1. `cargo build --release` produces `target/release/atlas`; `cargo test`
   green; `cargo clippy` clean to the repo's bar.
2. `atlas nodes --kind vision` lists ≥24 vision slugs (the live count),
   excluding the `_no_fleet_passes` sentinel.
3. `atlas nodes --kind prd` lists ≥107 PRD nodes; each shows its derived
   status; no parser panic on any real PRD in the live corpus.
4. `atlas show atlas` (this very vision) lists atlas-core/edges/orphans/
   render with their statuses; `atlas show nonexistent` exits 2 with a
   clear stderr message.
5. A PRD whose build-manifest entry has a non-empty `output_repo_path`
   that exists on disk derives status `shipped` with its `repo_url`
   populated; a PRD with no manifest entry derives `drafted`.
6. `--format json` on `nodes` and `show` emits valid JSON (round-trips
   through `python3 -m json.tool`); text and json carry the same facts.
7. Cold run of `atlas nodes` over the live corpus completes in <200 ms
   (the no-store budget); document the measured time in the README.
8. All four source paths are env-overridable; the test suite drives the
   parsers off fixture dirs, never the live `~/.claude` or `~/wintermute`.
9. README documents the node model, source resolution, and the
   read-only invariant (atlas never writes the corpus).

## Out of scope (follow-on PRDs)

- Dependency edges between PRDs (atlas-edges).
- Divergence/orphan lint (atlas-orphans).
- Graph rendering / DOT / Mermaid (atlas-render).
- Any write to PRDs, manifests, gossip, or REPOS.md — ever.
