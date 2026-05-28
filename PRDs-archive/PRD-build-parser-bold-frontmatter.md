# PRD — build-parser-bold-frontmatter

Status: Draft v0.1
build_target: self-mod
build_priority: medium
build_into: /home/jsy/.claude/skills/build/scripts/scan-prds.sh
build_version_bump: none
Created: 2026-05-28

## TL;DR

`scripts/scan-prds.sh` parses plain YAML-style frontmatter (`build_target:
rust-extend`) but not markdown-bold frontmatter (`**build_target:**
rust-extend`). 13+ live PRDs use the bold form (all `cadence-*` and most
`daily-receipt-*`), so the scanner stores `build_target: null` for them,
which forces the /build Phase 3 classifier to mark them
`needs_classification` even though the PRD body is unambiguous. Fix: a
one-line sed normalization at the top of the line-by-line loop that
collapses `**key:** value` → `key: value` before the case branches. Same
fix already exists for `**Status:**` as case alternates.

## Why this exists

This tick (2026-05-28 ~11:20 UTC) the /build skill picked `skill-doctor`
as the queued PRD because it had a plain-text `build_target: rust-cli`.
But 13 queued PRDs with bold-formatted frontmatter sit in the queue with
no target classification, blocking them from being picked. The classifier
denial of an inline parser-fix attempt this tick is what surfaced the
issue; rather than self-mod inside a /build tick, the right path is a
PRD that authorizes the change so future ticks (or /build run
build-parser-bold-frontmatter) can apply it under explicit user review.

Affected PRDs (queued, null target after parse):
- PRD-cadence-bind-confidant, -daily-receipt, -letters, -reliquary, -zine
- PRD-cadence-pulse, -substrate
- PRD-daily-receipt-archive, -haiku, -printer, -stamps, -summarize, -yearend-letter

All declare `**build_target:** rust-extend` (or similar) in their
frontmatter.

## What this builds

A single edit to `/home/jsy/.claude/skills/build/scripts/scan-prds.sh`
inside the `while IFS= read -r line; do ... done` loop, immediately after
the fence-block skip and before the `case "$line" in` build_* dispatch:

```bash
# Normalize markdown-bold frontmatter `**key:** value` -> `key: value`
# so PRDs that write headers as bold parse the same as plain
# YAML-style frontmatter.
line="$(printf '%s' "$line" | sed -E 's/^\*\*([a-zA-Z_]+):\*\*[[:space:]]*/\1: /')"
```

Plus a small smoke-test script under `scripts/test-bold-frontmatter.sh`
that asserts the scanner returns the expected build_target for at least
two known bold-formatted PRDs.

## Acceptance criteria

1. `scripts/scan-prds.sh` returns `build_target: "rust-extend"` for
   `PRD-cadence-substrate.md` (currently returns null).
2. `scripts/scan-prds.sh` returns `build_target: "rust-extend"` for
   `PRD-cadence-pulse.md` (currently returns null).
3. Existing plain-frontmatter PRDs continue to parse correctly
   (PRD-skill-doctor, PRD-agentns-claude, PRD-tool-manifest — regression
   check on at least three).
4. Fenced code blocks containing example frontmatter (e.g., PRDs that
   illustrate the format inside ``` blocks) are still skipped — the
   normalization runs after the in_fence check, not before.
5. The smoke-test script exits 0 on the live PRD set.

## Notes for /build

- Target type `self-mod` since this modifies build's own infrastructure.
- Classifier will likely require explicit user authorization for the
  scan-prds.sh edit, same as build-deferred-acs. Surface clearly in the
  iter-1 journal line so the user can approve in-band.
- After the fix lands, the next scan tick will repopulate `build_target`
  for all 13 affected PRDs, automatically routing them to their correct
  Phase 3 classification (rust-extend, mixed, etc.) on the following tick.
- No new commits to external repos. Single edit to skill scripts +
  smoke-test addition.

## Dependencies

None. Independent of all other PRDs.

## Cross-fleet notes

- Sibling to `build-deferred-acs` (also `self-mod` target): /build
  infrastructure improvements that compound the queue's correctness.
- Unblocks the cadence-* fleet from being picked by /build's queued
  priority, which has been stuck since 2026-05-25.
