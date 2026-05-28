# Vision: drift

> Skills assert. Tools change. The assertions outlive the change.
> Drift is the discipline of catching where what we wrote about our
> tools no longer matches the tools.

Created: 2026-05-28
Seed: reflection — 6+ consecutive self-reviews flagged the same
  three tool-skill drift instances and parked them in the journal
  without resolution. Cadence is wasted: same finding, same tick,
  same no-op outcome.
Pace: opt-in (default — `build_auto: false`)

## TL;DR

Three concrete drift instances repeat across self-review ticks
without fix:

1. `pevent gc --older-than 7d --dry-run` — `--dry-run` not on the
   installed binary; `--older-than` rejects `7d` (expects float).
   Cited in `~/.claude/skills/self-review/SKILL.md:74,170,389`.
2. `bpolicy status --format json` — `--format` unrecognized; the
   installed `bpolicy status` emits text only. Cited at
   `~/.claude/skills/self-review/SKILL.md:77`.
3. The bootstrap-symlink check at `self-review/SKILL.md:93` lists
   13 tools; live `ls ~/.local/bin/` shows 7 of those 13 don't
   exist (`skill`, `episode`, `apipe`, `recall-ops`, `recall-doctor`,
   `recall-io`, `mirror`). Yields 7 false-positive DANGLING flags
   every tick.

A fourth instance surfaced during Phase 1 of this dream:
4. `~/.claude/skills/dream/SKILL.md:86` cites `ctrace ls`. The
   freshness vision's evidence log already noted `ctrace`'s actual
   subcommands are `start|stop|status|query|tail` — no `ls`.

This vision adds the tooling to catch this class of drift before
it accumulates, and point-fixes the four known instances.

## End-state

When drift is fully built:

- `tool-manifest sync` probes every binary in `~/.local/bin/`
  (and the wintermute fleet under `wm-*`) via `<tool> --help`
  (and `<tool> <sub> --help` for visible subcommands), captures
  `{version, flags, subcommands, subflags}` into a JSON manifest.
- `skill-doctor check` walks `~/.claude/skills/*/SKILL.md`,
  extracts shell invocations from fenced bash blocks and inline
  code, cross-references each `--flag` and subcommand against the
  manifest, and parks drift proposals at
  `~/.claude/skill-doctor/proposals/<ULID>.md` for the user to
  review (same pattern as `recall observe`).
- The four known drifts in self-review and dream are fixed in
  place.
- A future Fleet 2 wires `skill-doctor check` into the self-review
  Phase A so each tick surfaces *fresh* drift, not the same four.

This is the sibling of `freshness/recall-doctor-claims`: that one
catches drift in *memory bodies*, this catches drift in *skill
text*. Same proposal-queue pattern, different data source. Neither
auto-edits; both park reviewable evidence.

## Components

**Fleet 1 — three PRDs:**

1. **drift-fix-self-review-dream** (`shell-extend` —
   `~/.claude/skills/self-review/SKILL.md` and
   `~/.claude/skills/dream/SKILL.md`) — point-fix. Replaces the
   broken `pevent gc --older-than 7d --dry-run` invocations with
   the actual installed surface (`pevent gc` accepts a float; the
   skill should either compute the float days or drop --dry-run
   pre-run guard). Replaces `bpolicy status --format json` with
   `bpolicy status` text parsing or drops the JSON dependency.
   Rewrites the 13-tool symlink list to the actual installed set,
   ideally read from a canonical source (CLAUDE_SELF.md Defaults
   has 8 canonical local tools; self-review's list intersects
   weakly with it). Fixes `ctrace ls` -> `ctrace status` in
   dream/SKILL.md. Single tick. Ships first to close the
   immediate noise.

2. **tool-manifest** (`rust-cli`, new repo at
   `~/wintermute/tool-manifest/`) — foundational. Walks the
   binaries in `~/.local/bin/` (configurable), probes each via
   `<tool> --help` and (for each detected subcommand) `<tool>
   <sub> --help`, captures `{name, path, version, flags,
   subcommands: [{name, flags}]}` into a JSON manifest at
   `~/.claude/tool-manifest/manifest.json`. CLI: `tool-manifest
   sync`, `show <tool>`, `query <tool> <flag>` (exit 0 if
   supported, 1 if not). Probing is best-effort: tools that don't
   support `--help` get a `version_only` flag. Manifest is the
   source of truth other drift tools read from.

3. **skill-doctor** (`rust-cli`, new repo at
   `~/wintermute/skill-doctor/`) — consumer. Walks
   `~/.claude/skills/*/SKILL.md` (and `~/.claude/skills/*/skill.md`
   case-fold), extracts shell invocations from fenced bash blocks
   plus backtick inline code, parses each as `<binary> [<sub>]
   <args...>`, cross-references against `tool-manifest`, flags:
   (a) flags not in the manifest's flag set, (b) subcommands not
   in the manifest's subcommand set, (c) binaries absent entirely.
   Parks proposals at `~/.claude/skill-doctor/proposals/<ULID>.md`
   with the source SKILL.md path, line range, offending
   invocation, and the manifest evidence. CLI: `skill-doctor
   check`, `proposals list`, `proposals show <ULID>`. Depends on
   tool-manifest (Fleet 1's middle ship).

## Order

```
drift-fix-self-review-dream   (independent, single-tick; ship first)
tool-manifest                  (no deps; ship parallel)
skill-doctor                   (depends on tool-manifest)
```

drift-fix-self-review-dream is small and tractable independently —
ship first to close the immediate noise that's been wasting
self-review ticks. tool-manifest and skill-doctor follow to
prevent the next class of drift from accumulating.

## Fleet 2 (not drafted)

Draft after Fleet 1 ships AND skill-doctor produces at least one
user-promoted edit to a skill:

- **drift-self-review-integration** — wire `skill-doctor check`
  into self-review Phase A so each tick surfaces fresh drift and
  the user sees it in Pending alongside other anomalies.
- **drift-cli-help-snapshot** — capture each tool's full `--help`
  output (not just the parsed flags) into the manifest so
  changelog diffs across `tool-manifest sync` invocations can
  highlight newly added or removed flags.
- **drift-changelog-witness** — cross-reference manifest version
  bumps against repo `CHANGELOG.md` entries; surface tools whose
  installed version exceeds the latest changelog line (the tool
  shipped without a release note).
- **drift-config-files** — extend `skill-doctor` to also walk
  `~/.config/**` config files referencing CLI flags (e.g.,
  systemd `ExecStart=` lines, hook scripts under
  `~/.claude/scripts/`).
- **drift-bootstrap-truth** — extend `~/wintermute/bootstrap/
  install.sh` to write `~/.local/bin/.bootstrap-tools.json` so the
  symlink check in self-review can read structured truth instead
  of a hardcoded list (current self-review fix in Fleet 1 is
  point-fix only; this is the durable source-of-truth move).

## Open questions

- **Extraction precision**: parsing shell from Markdown is fuzzy.
  Tolerable if proposals are review-gated. Fleet 1 covers the
  obvious cases (`<binary> --flag value`); rare invocation shapes
  (heredocs, multiline backslash-continuations, env-prefixed
  commands) are out of scope until Fleet 2.
- **Subcommand depth**: `<tool> <sub> --help` covers one level.
  Recursive probing (`<tool> <sub> <subsub> --help`) is deferred
  until skill-doctor finds it actually matters in practice.
- **Bin discovery scope**: `tool-manifest sync` probes the
  configured prefix only — `~/.local/bin/` by default, with
  `--include` for adding `/usr/local/bin/` etc. Path-walking
  every binary in $PATH is too much for a manifest; the bins we
  care about for skill drift are the locally-built wintermute
  fleet.
- **Tools without `--help`**: some installed tools don't accept
  `--help` (printers like `procstat self` are positional). The
  manifest records `version_only: true` and `skill-doctor` skips
  flag-validation for those, only flags missing-binary.

## Evidence log

- 2026-05-28T01:35 (Phase 1): `pevent gc --older-than 7d --dry-run`
  cited at `self-review/SKILL.md:74,170,389`. Live probe shows
  installed `pevent gc` accepts `[-h] [--older-than OLDER_THAN]`,
  `--dry-run` is not a recognized flag, and `--older-than 7d`
  errors with "invalid float value: '7d'".
- 2026-05-28T01:35 (Phase 1): `bpolicy status --format json` cited
  at `self-review/SKILL.md:77`. Live probe shows installed
  `bpolicy status` accepts `[-h]` only; `--format json` is
  unrecognized.
- 2026-05-28T01:35 (Phase 1): bootstrap-symlinks list at
  `self-review/SKILL.md:93` checks `{agorabus, recall, skill,
  spool, episode, apipe, transcript, recall-lint, recall-ops,
  recall-doctor, recall-io, mirror, claude-self}`. `ls
  ~/.local/bin/` shows 7 of these (skill, episode, apipe,
  recall-ops, recall-doctor, recall-io, mirror) do not exist.
- 2026-05-28T01:35 (Phase 1): `ctrace ls` cited at
  `dream/SKILL.md:86`. Already flagged in `freshness.md` evidence
  log as invalid; installed `ctrace` has subcommands
  `start|stop|status|query|tail`.
