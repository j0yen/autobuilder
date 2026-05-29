# PRD: atlas-render — make the web visible

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/atlas.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/atlas
**Depends on:** PRD-atlas-core.md, PRD-atlas-edges.md
**Codename:** *atlas* — finally, you can see it.

## TL;DR

atlas-core has the nodes, atlas-edges has the dependencies. atlas-render
is the payoff the whole vision points at: `atlas graph` draws the
vision→PRD→repo web as a Graphviz DOT graph, a Mermaid diagram, or a
terminal tree — so the coherence the dream/build loop is growing toward
stops being a thing held in one head and becomes a picture. Extends the
atlas crate; pure rendering over the existing model, no new data sources.

## Why this exists

Phase 1 evidence (2026-05-29):

- The /dream SKILL.md states the end-state in visual terms: *"an unending
  web of code: tools that compose, skills that gossip, repos that extend
  each other, each PRD a node in a graph that grows toward something
  coherent."* The graph is the *stated goal*; nothing draws it.
- The corpus is now large enough that prose can't convey shape: **107
  PRDs across 24 visions**, with rust-extend chains (this fleet alone is
  a 4-node chain into one repo; vigil Fleet 3 is a 5-node serialized
  chain; almanac, cadence, companion each fan a vision across many PRDs).
  A picture of these chains is the difference between "I think that's
  ordered right" and seeing it.
- `~/wintermute/REPOS.md` already maintains a flat list of ~50 shipped
  repos; a graph keyed by *origin vision* turns that flat list into the
  structure that produced it.
- Mermaid renders inline in the GitHub READMEs the /build skill already
  maintains (per its "update Abouts" step) — a generated `atlas graph
  --format mermaid` could drop straight into REPOS.md or a vision doc.

## What this builds

Extends `j0yen/atlas`. Adds a `render` module + command. Read-only,
deterministic output (stable node ordering so diffs are meaningful).

**`atlas graph [--format dot|mermaid|tree] [--vision <slug>] [--shipped-only]`**

- `dot` — Graphviz: vision nodes as one shape/color, PRD nodes colored by
  derived status (drafted / in-flight / shipped), repo nodes a third
  shape; vision→prd ownership edges and prd→prd dependency edges (the
  latter styled by edge `kind`, frontmatter solid / gossip dashed).
- `mermaid` — equivalent `graph TD` for inline-in-markdown rendering.
- `tree` — terminal: vision as root, its PRDs indented with a status
  glyph and repo URL, dependency arrows noted inline. The
  no-Graphviz-installed default human view.
- `--vision <slug>` scopes to one vision's subgraph; absent → whole web.
- `--shipped-only` prunes to shipped PRDs + their repos (the "what
  actually exists" view vs. the "what's planned" view).

Output is deterministic: nodes sorted by (vision slug, prd filename),
edges sorted by (from, to). Re-running with no corpus change yields
byte-identical output (so a committed `.dot`/`.mmd` diffs cleanly).

**Deps:** none new (string templating, no graphviz crate — atlas emits
DOT/Mermaid *text*; rendering to an image is the user's `dot`/mermaid-cli
downstream). MSRV 1.85, no let-chains.

## Acceptance criteria

1. `cargo build --release` + `cargo test` green; clippy to the repo bar.
2. `atlas graph --format dot` over the live corpus emits valid DOT
   (parses under `dot -Tsvg` if Graphviz is present; otherwise a fixture
   test asserts well-formed `digraph { ... }` structure) containing ≥24
   vision nodes and ≥107 PRD nodes.
3. `atlas graph --format mermaid` emits a `graph TD` block that renders
   (asserted structurally: valid node/edge syntax, no unclosed brackets).
4. `atlas graph --format tree --vision atlas` prints this vision as root
   with atlas-core/edges/orphans/render beneath it, each with a status
   glyph; dependency arrows (core→edges→{orphans,render}) are shown.
5. PRD nodes are colored/marked by derived status; a `--shipped-only`
   run contains only `shipped` PRDs and their repos, and no `drafted`
   node appears.
6. Output is deterministic: two consecutive runs over an unchanged
   fixture corpus are byte-identical (sorted nodes + edges).
7. Dependency edges are styled by `kind` (frontmatter vs. gossip) in
   `dot` and `mermaid` output.
8. README shows a worked example: `atlas graph --format mermaid --vision
   atlas` and the rendered diagram, plus the `dot | dot -Tsvg` pipeline.

## Out of scope

- Rendering to PNG/SVG inside atlas (emit text; let `dot`/mermaid-cli do
  pixels — keeps atlas dependency-light).
- Interactive / TUI graph navigation (a Fleet-2 idea if the static
  exports prove insufficient).
- Writing the generated diagram into REPOS.md or vision docs (atlas is
  read-only; a /build or /dream step could consume the output, but atlas
  does not write the corpus).
