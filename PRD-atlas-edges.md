# PRD: atlas-edges — the dependencies between nodes

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/atlas.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/atlas
**Depends on:** PRD-atlas-core.md (the node model)
**Codename:** *atlas* — a node knows what it waits on.

## TL;DR

atlas-core renders nodes; the queue's real intelligence is in the
*edges* — "foo → bar → baz (baz depends on bar's API)". Those edges are
written down in two places: gossip's free-form `Order:` lines and PRD
frontmatter (`Depends on:`, `build_into`). atlas-edges parses both into
typed dependency edges and exposes `atlas deps <prd>` (what it waits on,
what waits on it) and `atlas blocked` (PRDs whose dependencies haven't
shipped). It extends the atlas crate; it adds no new node kinds.

## Why this exists

Phase 1 evidence (2026-05-29):

- `wc -l notes/gossip.md` → **3922 lines**, dense with dependency prose.
  The tail read this session shows the shape: vigil Fleet 3's note spells
  out `client-reconnect → drain-notice → state-persist → reload`, and
  almanac's spells `tick-daemon first ... missed-to-kin adds the bridge`.
  Every fleet's gossip note carries an `Order:` block. It is the
  dependency graph, in prose, unqueried.
- PRD frontmatter carries structured edges too: `PRD-docket-core.md`
  declares `**Depends on:** none`; this fleet's PRDs declare
  `**Depends on:** PRD-atlas-core.md`. `**build_into:**` is itself an
  edge from a rust-extend PRD to the repo it extends.
- The two sources disagree in formality (prose vs. frontmatter), so a
  parser must rank them — frontmatter authoritative, gossip secondary
  (vision OQ: "Edge source-of-truth ambiguity").
- Today's `/build` manifest shows **9 user-gated blockers**; "what is
  blocked and by what" is currently answered by reading gossip by hand.

A node model without edges is a list, not a graph. This adds the edges.

## What this builds

Extends `j0yen/atlas` (no new binary). Adds an `edges` module + two
commands.

**Edge model** (serde-serializable):

| field    | notes                                                          |
|----------|----------------------------------------------------------------|
| `from`   | prd filename (the dependent)                                   |
| `to`     | prd filename (the dependency)                                  |
| `kind`   | `frontmatter` (from `Depends on:`) or `gossip` (from `Order:`) |
| `source` | file + line the edge was parsed from (provenance)              |

**Parsers:**
- **frontmatter edges:** for each PRD with `**Depends on:** <list>`,
  resolve each named `PRD-*.md` to a node; `none` → no edge. Authoritative.
- **gossip edges:** scan `notes/gossip.md` for `Order:` blocks and
  `depends on` / `→` / `->` arrows; map the named PRD slugs to nodes.
  Best-effort, lower precedence. An edge present in frontmatter is *not*
  duplicated from gossip; a gossip-only edge is kept and tagged
  `kind: gossip` so the user can see it's the softer signal.
- An edge whose endpoint can't be resolved to a known PRD node is
  dropped and counted (reported under `--format json` as `unresolved`),
  never panics.

**Commands:**
- `atlas deps <prd> [--format text|json]` — prints `depends on:` (out-edges)
  and `required by:` (in-edges), each line annotated with edge `kind`.
  Unknown PRD → exit 2.
- `atlas blocked [--format text|json]` — PRDs that have ≥1 dependency
  whose derived status (from atlas-core) is not `shipped`. This is the
  "what can't build yet, and why" view.

**Deps:** none new beyond atlas-core's. MSRV 1.85, no let-chains.

## Acceptance criteria

1. `cargo build --release` + `cargo test` green; clippy to the repo bar.
2. `atlas deps PRD-atlas-edges.md` shows `depends on: PRD-atlas-core.md
   (frontmatter)` and lists atlas-orphans / atlas-render under
   `required by:` (they depend on edges).
3. A frontmatter `**Depends on:** none` PRD yields zero out-edges and
   does not appear as blocked solely on that basis.
4. When the same edge is asserted by both frontmatter and gossip, exactly
   one edge is emitted, tagged `frontmatter` (precedence rule).
5. A gossip `Order:` line naming a PRD that exists yields a
   `kind: gossip` edge; one naming a non-existent PRD is counted in
   `unresolved` and emitted by no command (no panic, no phantom node).
6. `atlas blocked` lists a PRD iff at least one of its dependencies is
   not `shipped`; a PRD all of whose deps are shipped is absent.
7. `--format json` on `deps` and `blocked` is valid JSON; edges carry
   `kind` and `source` provenance.
8. Fixture-driven tests cover: frontmatter-only, gossip-only,
   both-agree, both-disagree-endpoint, unresolved-endpoint. No test
   reads the live gossip or manifests.
9. README documents the frontmatter-over-gossip precedence rule and the
   edge model (vision OQ closure).

## Out of scope

- Cycle detection / topological sort (a Fleet-2 nicety; v1 just reports
  edges — if a cycle exists, `blocked` still terminates because it does
  not recurse, it checks one hop of dependency status).
- Divergence lint (atlas-orphans) and rendering (atlas-render).
