# PRD — build-archive-finalize: auto-run the clerical finalization that blocks the archive gate

Status: Draft v0.1
build_target: shell
build_priority: high
build_into: /home/jsy/.claude/skills/build

## TL;DR

A `/build` archive-gate pass on 2026-06-03 found that **6 of 9** "done"/
"complete" candidates were NOT blocked on functionality — their `cargo test`
was green and every AC was paired (checks C1 and C5 passed). They failed the
verified-completed gate purely on **clerical post-build chores**:

- `ctrace-scribe-rollup`: repo never pushed (no `origin` remote); not in REPOS.md.
- `wm-router`: never published; no README; not in REPOS.md.
- `rollout`: no README; not in REPOS.md (repo itself was pushed).
- `docket-digest`: README missing the AC-required `digest` subsection.
- `wintermute-reach-digest`: `CHANGELOG.md` absent.
- (`rollout` additionally had 3 clippy `expect_used` errors + untested live ACs — partly clerical, partly real.)

These are the exact steps the skill already documents in Phase 4 (publish /
install / wire) and Phase 5 (Abouts: README, CHANGELOG, REPOS.md) — but a prior
tick marked each PRD `done`/`complete` right after the build went green and
**never executed the finalization steps**. So they sit in limbo and re-fail the
archive gate every time they're re-selected, burning ticks without progress.

This PRD adds a single idempotent helper, `scripts/archive-finalize.sh <slug>`,
that the tick runs as ONE action when a candidate's archive gate fails on
**only** C2/C3/C4 (publish/README/CHANGELOG/REPOS) while C1 and C5 pass. It
performs exactly the missing clerical steps, re-runs `verified-completed.sh`,
and — if now green — hands back to the normal archive action. It NEVER touches
C1/C5 (real functionality/AC gaps stay the build's job) and never fabricates
test evidence.

## Motivation

The done→shipped transition has a silent clerical cliff. Build quality is fine;
the queue clogs on README/CHANGELOG/REPOS/push bookkeeping that no single tick
owns. Making that bookkeeping a first-class, gated, idempotent action converts
~6 stuck PRDs into one-tick auto-finishes and stops the re-selection churn.

## Design

`scripts/archive-finalize.sh <slug> [--dry-run]`:

1. Resolve the PRD path, `build_target`, and `output_repo_path` from
   `manifest.json` (+ `scan-prds.sh` for `build_into`).
2. **Gate the gate.** Run the verified-completed checks. PROCEED ONLY IF
   C1 (tests green) AND C5 (ACs paired/deferred) already pass. If C1 or C5
   fail, exit 10 with `not-clerical: <which>` — this helper refuses to paper
   over a real gap. Do nothing.
3. For each failing clerical check, perform the documented fix:
   - **C2 (new-repo)**: if no `origin`, `wm-publish --slug <slug> --description
     "<PRD one-liner>"`; else `wm-push --slug <slug>`. (rust-extend: `wm-push`
     only — never `wm-publish`.)
   - **C3 (new-repo)**: generate `README.md` from the PRD TL;DR + Acceptance +
     an Install block (reuse the existing Phase-5 README generator if present).
     **(rust-extend)**: `extend-handler.sh changelog-prepend` to create/prepend
     the `## v<ver>` CHANGELOG section from the PRD TL;DR.
   - **C4**: append the one-line REPOS.md entry under the right category if
     absent (idempotent: grep first).
4. Commit clerical changes with the Joe Yen identity, path-scoped (never sweep a
   dirty tree — honor the 2026-06-03 lesson). Push.
5. Re-run `verified-completed.sh`. Echo the new verdict to stderr as
   `[finalize-verdict] ready|still-blocked: <checks>`.

Wire into the skill doc (Phase 4): when a candidate's gate fails on only
C2/C3/C4, the tick runs `archive-finalize.sh <slug>` as its one action; the
NEXT tick re-selects and archives. Add a note to the verified-completed section
pointing at the helper.

## Acceptance

1. `archive-finalize.sh <slug>` on a PRD whose C1 or C5 fails exits non-zero
   (`not-clerical`) and makes zero mutations (verify with `git status` /
   `wchg`).
2. On a rust-extend slug missing only `CHANGELOG.md` (e.g. the
   `wintermute-reach-digest` shape), it creates the `## v<ver>` section from the
   PRD TL;DR, commits path-scoped, pushes, and the re-run gate reports `ready`.
3. On a new-repo slug missing only the REPOS.md entry, it appends exactly one
   idempotent line (second run is a no-op) and re-gate reports the C4 fix.
4. Commits are path-scoped: a deliberately-dirtied unrelated file in the repo is
   NOT included in the finalize commit.
5. `--dry-run` prints the planned steps and the would-be commit pathspec without
   mutating anything.
6. The helper never writes test files, never edits `src/`, and never alters AC
   pairing — proven by a test that points it at a C5-failing PRD and asserts no
   `.rs` file changed.

## Notes

Triggered by the 2026-06-03 archive-gate tick (Phase 6 reflect). Companion to
`verified-completed.sh` (the classifier) and `extend-handler.sh` (the extend
mechanics) — this is the missing "do the chores the classifier is complaining
about" actuator, fenced so it can only ever do clerical work.
