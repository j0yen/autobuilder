# PRD: build-scan-bold-frontmatter — scan-prds.sh must parse bold-markdown `build_*` keys

**Author:** /build (Phase 6 reflect), for jsy
**Status:** Draft v0.1
**Date:** 2026-06-02
**build_target:** shell
**build_into:** /home/jsy/.claude/skills/build
**build_priority:** high
**Depends on:** none

## TL;DR

`scripts/scan-prds.sh` extracts `build_target` / `build_into` /
`build_priority` / `build_version_bump` with a `case` that only matches
lines **beginning with the bare key** (`"build_target:"*`). But every
`/dream`-authored PRD writes its frontmatter in **bold-markdown** form —
`**build_target:** shell` — which begins with `**build_target`, not
`build_target`, so it falls through and the value stays `null`. The
`Status:` parser already handles the bold form (`"**Status:**"*` case at
line ~112); the `build_*` parsers do not. The asymmetry means the
scanner silently mis-classifies an entire class of PRDs.

Observed 2026-06-02: a `/build` tick scanned 7 `/dream` fleet PRDs
(vigil-*, warden-*) — all emitted `build_target: null` despite each
declaring `**build_target:** shell|rust-cli|rust-extend`. The
orchestrator only recovered by hand-reading each PRD; an unattended tick
would have routed all 7 to `needs_classification`.

## Why this exists

`/dream` and `/build` share the autobuilder queue but disagree on
frontmatter syntax: `/dream` emits human-readable bold markdown, `/build`'s
scanner expects YAML-bare keys. Status parsing was already reconciled
(both forms accepted); the `build_*` keys were missed in that same fix.
Result: a standing, silent mis-parse that recurs for every future dream
fleet and is invisible unless a human is driving the tick.

## Acceptance

1. `scan-prds.sh` emits the correct `build_target` for a PRD whose
   frontmatter line is `**build_target:** shell` (bold markdown).
2. Same for `**build_into:**`, `**build_priority:**`,
   `**build_version_bump:**` (bold form parsed identically to bare form).
3. Bare-key form (`build_target: rust-cli`) continues to parse unchanged
   (no regression) — verified against an existing bare-key PRD.
4. First-match-wins is preserved: a real bold-frontmatter key near the
   top still beats a later bare-key example inside a fenced ``` block
   (fenced lines stay skipped).
5. A regression fixture under `scripts/test/` (or inline in a
   `scan-prds-selftest.sh`) covers AC1–AC4 and exits 0.

## Implementation sketch

In the line loop, normalize each line before the `case` by stripping a
leading `**` and a `:**` → `:` (only for the `build_*` / known keys), or
add explicit `"**build_target:**"*` arms mirroring the existing
`"**Status:**"*` arm. The strip approach is fewer lines and matches how
`Status` could also be simplified. Keep the fenced-code-block skip and
the trailing-`#`-comment strip intact.

## Notes

This is the parser-gap guardrail from the 2026-06-02 reflect: a repeated
mis-parse (7 PRDs in one tick) that a guardrail prevents. Marked
`build_priority: high` because it silently degrades classification for
every `/dream`-authored PRD until fixed.
