# Vision: atlas — render the web the queue already is

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Fleet 1 drafted:** 4 PRDs
**Seed:** bare `/dream` (interactive). Phase-1 live inspection found the
feature-space saturated and well-targeted; the one genuinely-uncovered
gap is that the dream/build loop has no view of *itself*.

---

## TL;DR

The /dream skill's own end-state is *"an unending web of code... each
PRD a node in a graph that grows toward something coherent."* That web
now exists in fact — 107 PRDs across 24 visions, ~50 shipped repos under
`j0yen`, two manifests, and 3922 lines of gossip — but **nothing renders
it**. The graph is latent in files that already carry every edge:
each PRD's frontmatter names its `Vision:` and (for extends) its
`build_into`; the build manifest records each PRD's `output_repo_path`
and `iter_log`; the dream manifest maps vision → `prds_drafted` →
`status`; gossip's `Order:` lines encode dependencies; `REPOS.md` is the
shipped index. atlas is one small Rust CLI that *joins* these sources
into the graph, so a human (or a skill) can ask "what does this vision
own, what shipped, what depends on what, what's orphaned" and get an
answer instead of grepping 107 files.

atlas reads; it never writes to the autobuilder corpus. It is the map,
not the territory.

## End-state

When this is done:

- `atlas show <vision>` prints a vision's PRDs, each with status
  (drafted / in-flight / shipped) and its shipped repo URL if any.
- `atlas deps <prd>` prints what a PRD depends on and what depends on it,
  resolved from gossip `Order:` lines + frontmatter `build_into`/`Vision:`.
- `atlas doctor` surfaces the divergences a human can't see by eye:
  PRDs with no vision, visions with no drafted PRDs, shipped repos with
  no PRD of origin, PRDs the build manifest calls shipped whose repo path
  is gone, and visions whose PRDs are all shipped but are still marked
  `active` (i.e. *fulfilled* — exactly the cross-reference /dream is
  supposed to do on each invocation but does by hand).
- `atlas graph` exports the whole web as DOT / Mermaid / a terminal tree,
  so the coherence the loop is growing toward is finally *visible*.
- Every command takes `--format json` so skills consume it, not just eyes.

## Components (Fleet 1 — drafted)

- **atlas-core** — the substrate. Parse all PRD frontmatter + both
  manifests + REPOS.md into one in-memory graph model (nodes: vision,
  prd, repo). `atlas nodes`, `atlas show <vision>`, `--format json`.
  New repo `j0yen/atlas` at `~/wintermute/atlas`. Keystone, ships alone.
- **atlas-edges** — dependency edges. Parse gossip `Order:` /
  "depends on" lines and PRD `build_into`/`Vision:` into typed edges;
  `atlas deps <prd>`, `atlas blocked`. Extends atlas-core.
- **atlas-orphans** — reconciliation lint. `atlas doctor` emits the five
  divergence classes above with exit codes. Extends atlas-core+edges.
- **atlas-render** — the payoff. `atlas graph --format dot|mermaid|tree`
  renders the vision→PRD→repo web. Extends atlas-core+edges.

## Order

atlas-core (keystone, ship FIRST — defines the node model + parsers)
  → atlas-edges (needs the node model to attach edges to)
  → atlas-orphans (needs nodes + edges to detect divergence)
  → atlas-render (needs nodes + edges to draw; orphans optional for it)

edges, orphans, render all extend the same crate — **serialize them**
(same caution every multi-extend fleet raises: concurrent /autobuilder
agents collide on Cargo/lib.rs re-export churn). orphans and render are
independent of each other and can be ordered either way after edges.

## Open questions

- **atlas doctor → self-review bind.** A `build_target: shell` PRD that
  wires `atlas doctor` into self-review's Phase B so vision divergence is
  surfaced (and fulfilled visions auto-flagged) is the natural Fleet-1
  capstone — but its whole premise is calling an *installed, verified*
  `atlas doctor`, so it must not be drafted until atlas-orphans has
  shipped. Left here deliberately (mirrors how vigil Fleet 3 held
  `agorabus-reload-self-review` behind `agorabus reload`). Next
  `/dream extend atlas` drafts it once orphans is green.
- **Does atlas overlap docket?** No. `docket` (vision created same day)
  gives *self-review findings* an identity and a lifespan. atlas gives
  the *vision/PRD/repo corpus* a rendered structure. docket tracks "this
  problem recurs"; atlas tracks "this code exists and connects there."
  They could meet later (atlas could *report* a stale-repo divergence as
  a docket finding) — captured as a future cross-vision bullet, not v1.
- **Live graph vs. snapshot.** v1 reads the files fresh on each
  invocation (cheap — they total <5 MB). A cached/incremental index is a
  Fleet-2 concern only if cold parse ever exceeds ~200 ms. Static for v1.
- **Edge source-of-truth ambiguity.** gossip `Order:` lines are
  free-form prose; PRD `Depends on:` frontmatter (docket-core uses it) is
  structured. atlas-edges should prefer frontmatter where present and
  treat gossip as a softer secondary signal — pin this in atlas-edges'
  README so the parser's precedence is documented.
