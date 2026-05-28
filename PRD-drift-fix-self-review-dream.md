# PRD — drift-fix-self-review-dream

Status: Draft v0.1
build_target: shell-extend
build_into: ~/.claude/skills/
Vision: visions/drift.md
build_priority: medium
deferred_acs: [5]
deferred_ac_reasons:
  5: "negative-evidence AC gated on a future external event — verifies that the *next* self-review tick after the edits land records zero of the four flagged drift instances. The edits themselves landed iter-2 (2026-05-28T10:06:04Z): dream-skill commit f942d25 on j0yen/dream-skill main, loose self-review SKILL.md edits persisted on disk; ACs 1-4 grep checks all return zero matches; AC6 (probed live before write) recorded in manifest.verification.probe_* fields; AC7 (prose style preserved) verified by re-read. AC5 can only be observed retroactively once self-review next fires and emits a clean journal entry. Deferring per the build-deferred-acs convention: future-tick-observation ACs are paired-when-observed, not at archive time."

## TL;DR

Self-review and dream both contain hardcoded references to CLI
flags and tool surfaces that no longer match the installed
binaries. Same drift instances surface in journal every tick
(6+ consecutive ticks) without resolution. This PRD point-fixes
the four known instances; the durable detection layer (tool-manifest
+ skill-doctor) ships in companion PRDs.

## Why this exists

Four concrete drift instances, all currently live on the laptop:

1. `~/.claude/skills/self-review/SKILL.md:74,170,389` —
   `pevent gc --older-than 7d --dry-run`. The installed `pevent
   gc` binary supports only `--older-than OLDER_THAN`, expects a
   float, and has no `--dry-run`. The string `7d` errors. Every
   self-review tick that reaches Phase D either logs the failure
   or silently skips the prune. (Evidence: live probe
   2026-05-28T01:35; flagged in journal `2026-05-27.md` Notable
   sections of runs 5 and 6.)
2. `~/.claude/skills/self-review/SKILL.md:77` —
   `bpolicy status --format json`. Installed `bpolicy status`
   accepts `[-h]` only; `--format json` errors. The skill expects
   to parse `{loaded, enforcing, policies}` JSON; today it must
   parse text or skip the check.
3. `~/.claude/skills/self-review/SKILL.md:93` — bootstrap-symlink
   check enumerates 13 names; 7 do not exist in
   `~/.local/bin/` (`skill`, `episode`, `apipe`, `recall-ops`,
   `recall-doctor`, `recall-io`, `mirror`). Each tick prints
   "DANGLING" for symlinks that were never created in the first
   place. Confirmed in self-review run 6 ("11 false positives").
4. `~/.claude/skills/dream/SKILL.md:86` — `ctrace ls`. Installed
   `ctrace` exposes subcommands `start|stop|status|query|tail`.
   No `ls` subcommand. Already flagged in
   `visions/freshness.md` evidence log
   ("`ctrace ls` cited in 06:30 freshness gossip is invalid").

These have all been observed for at least one cycle without a fix
landing. The pattern is structural: nobody owns "fix the drifting
flags," so the journal logs it and the tick moves on. This PRD
owns the fix for the four known cases; the durable detection
layer ships in `tool-manifest` + `skill-doctor`.

## What this builds

Single-tick edit pass over two files.

### Edits to `~/.claude/skills/self-review/SKILL.md`

- **Line 74** (Phase A guidance): rewrite the `pevent gc
  --older-than 7d --dry-run` invocation to use the actual installed
  surface. Two acceptable rewrites:
  - Drop `--dry-run` and pass `7` (a float interpreted as days)
    or whatever unit the binary actually uses — verify with a
    re-read of `pevent gc --help`.
  - If a dry-run preview is required, parse `pevent list --json`
    in-skill and filter to `state == exited && finished_at <
    now() - 7d`. Recommended path: this is more robust and
    independent of future `pevent gc` API changes.
- **Line 77** (Phase A guidance): rewrite the
  `bpolicy status --format json` call to use `bpolicy status` text
  output. The expected fields (loaded/enforcing/policies) need a
  text-parse spec or — if the field shape is brittle — the skill
  drops the JSON dependency and surfaces the full text under
  Pending for the user to read.
- **Line 93** (Phase A guidance): rewrite the bootstrap-symlinks
  enumeration. Two acceptable rewrites:
  - Use the canonical 8-tool list from `~/.claude/CLAUDE_SELF.md`
    Defaults section: `sbx pevent wchg procstat txn-edit tcap
    ctrace bpolicy`. (Plus `claude-self` since it's referenced
    elsewhere as a wintermute tool, plus `recall` since it's the
    central memory CLI.) This matches what's actually installed.
  - Read the list from CLAUDE_SELF.md at runtime so the skill
    stays in sync with the canonical surface. Recommended path
    if the parse is cheap; otherwise the hardcoded 10-tool list
    is fine for Fleet 1.
- **Line 170** (Phase D action) and **line 389** (run-end action):
  match whatever rewrite landed at line 74. If the in-skill
  filter approach was chosen, Phase D becomes `pevent list --json
  | jq ... | xargs -n1 pevent gc-one` (or equivalent), not a
  bulk `pevent gc --older-than`.

### Edits to `~/.claude/skills/dream/SKILL.md`

- **Line 86** (Phase 1 research checklist):
  `ctrace ls` -> `ctrace status` (lists active sessions per
  installed binary) OR `ctrace query --recent` (depends on what
  the dream skill actually wants — the surrounding line implies
  "what ran recently," which is `ctrace query`'s job, not
  `status`'s). Recommended: split into two bullets — one for
  active sessions (`ctrace status`) and one for recent activity
  (`ctrace query --recent <N>`).

### Out of scope

- Detection of future drifts — that's `skill-doctor`'s job.
- A canonical source-of-truth for installed tools (a structured
  manifest at `~/.local/bin/.bootstrap-tools.json` or similar) —
  that's `bootstrap-emit-toollist`'s job, deferred to Fleet 2 of
  the drift vision.
- Rewriting the four invocations to read from `tool-manifest`'s
  output — `tool-manifest` doesn't exist yet; this PRD ships
  before it.

## Acceptance criteria

1. After edits land, `grep -n "pevent gc --older-than 7d
   --dry-run" ~/.claude/skills/self-review/SKILL.md` returns no
   matches.
2. After edits land, `grep -n "bpolicy status --format json"
   ~/.claude/skills/self-review/SKILL.md` returns no matches.
3. After edits land, the bootstrap-symlinks check at
   `self-review/SKILL.md:93` references only symlinks that
   currently exist in `~/.local/bin/`. Verified by running the
   check live and seeing zero false-positive DANGLING reports.
4. After edits land, `grep -n "ctrace ls"
   ~/.claude/skills/dream/SKILL.md` returns no matches.
5. The next self-review tick after these edits records zero of
   the four flagged drift instances in its journal entry. (User
   can verify by reading `~/brain/journal/<next-date>.md`.)
6. Edits do not introduce new drift: every replacement invocation
   was probed live before being written (e.g., `pevent list
   --json` was confirmed to exist before being used in a
   rewrite).
7. The skill bodies remain readable English-plus-code; this isn't
   a refactor to a config file. Style matches the surrounding
   prose.

## Notes for /build

- Single-tick edit. No Cargo, no compile, no install.
- Verify each replacement invocation against the live binary
  *before* writing it; do not paste a guessed flag and call the
  PRD done.
- For the `pevent gc` rewrite, the recommended in-skill-filter
  path is more durable but adds a few lines of jq. Either is
  acceptable as long as ACs 1, 5, 6 pass.
- For the bootstrap-symlinks rewrite, the hardcoded 10-tool list
  path is simpler; the runtime-read path is more durable. Use
  the simpler one unless reading CLAUDE_SELF.md is trivially
  short.
- Edits to `dream/SKILL.md` cannot be tested by re-running dream
  (would race with this very session). Verification is grep-only
  plus a sanity read of the surrounding prose.
- Commit identity: Joe Yen (`~/.claude/skills/` is part of the
  dotfiles + wintermute ecosystem; convention is Joe Yen for
  ~/.claude edits).

## Dependencies

None. This PRD is independent of `tool-manifest` and
`skill-doctor`; they consume from a manifest that doesn't exist
yet, while this PRD just point-fixes existing drift.
