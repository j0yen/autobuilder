# PRD — skill-doctor

Status: Draft v0.1
build_target: rust-cli
build_into: ~/wintermute/skill-doctor/
Vision: visions/drift.md
build_priority: medium

## TL;DR

A Rust CLI that walks `~/.claude/skills/*/SKILL.md`, extracts
shell invocations from fenced bash blocks and backtick inline
code, cross-references each `--flag` and subcommand against the
`tool-manifest` JSON, and parks drift proposals at
`~/.claude/skill-doctor/proposals/<ULID>.md` for the user to
review. Same review-gated pattern as `recall observe` and
`freshness/recall-doctor-claims`; no auto-edit.

## Why this exists

Drift between skill text and installed tooling has been observed
repeatedly (see `visions/drift.md` §"Why this exists" for the four
live instances). Companion PRD `tool-manifest` produces the
ground-truth manifest of what flags and subcommands each binary
actually supports. This PRD ships the consumer that uses that
ground truth to find drift.

Why a proposal queue instead of auto-edit:
- Edits to skill text need human review — false positives are
  inevitable when parsing shell out of Markdown prose.
- Mirrors the existing `recall observe` pattern, which the user
  is already familiar with reviewing.
- Decoupling detection from application lets the same checker
  feed multiple downstream consumers (a `/drift-proposals` skill,
  self-review Phase A integration, etc., all deferred to Fleet 2).

## What this builds

A Rust CLI crate at `~/wintermute/skill-doctor/`:

```
~/wintermute/skill-doctor/
├── Cargo.toml
├── src/
│   ├── main.rs         # CLI entrypoint
│   ├── lib.rs          # public API
│   ├── extract.rs      # SKILL.md -> invocation list
│   ├── check.rs        # invocation × manifest -> drift result
│   └── proposal.rs     # proposal file writer
└── tests/
    └── fixtures/       # tiny SKILL.md samples with known drift
```

### Invocation extraction

Walk every `*.md` file under `~/.claude/skills/*/` (case-folded
SKILL.md or skill.md):

1. Extract fenced code blocks (` ```bash`, ` ```sh`, ` ```shell`,
   plus unfenced when context suggests shell). Within each block,
   split by newline and identify lines that look like commands:
   start with a known binary name (or `~/.local/bin/<name>`).
2. Extract backtick-inline code in prose. Same heuristic: starts
   with a known binary name.
3. Parse each candidate as `[~/.local/bin/]<binary>
   [<subcommand>] [<flags-and-args>...]`. Heuristic — argparse
   ordering varies, but in skill prose the canonical shape is
   binary -> sub -> flags.
4. Yield an `Invocation { skill_path, line, binary, subcommand,
   flags }` record per match.

### Drift checking

For each `Invocation`:

1. If `binary` not in the manifest -> drift type `BinaryMissing`.
2. If `subcommand` not in the manifest's subcommands for that
   binary -> drift type `SubcommandUnknown`.
3. For each `flag` in `flags`: if not in the manifest's flag set
   for the appropriate scope (subcommand if present, else top-
   level) -> drift type `FlagUnknown`.
4. If the binary is `version_only: true`, skip flag-validation
   (record `SkippedVersionOnly`); still check binary presence.

### Proposal output

Each unique drift produces one file at
`~/.claude/skill-doctor/proposals/<ULID>.md`:

```markdown
---
id: 01HXYZ...
kind: FlagUnknown
created: 2026-05-28T01:35:00Z
status: pending
---

# Drift in self-review SKILL.md:74

Invocation:
```
~/.local/bin/pevent gc --older-than 7d --dry-run
```

Manifest evidence (tool-manifest/manifest.json):
- `pevent gc` supports flags: [`-h`, `--help`, `--older-than`]
- `--dry-run` is not in the supported flag set
- (also: `--older-than` expects a float, `7d` would be rejected;
  but the parser only catches presence, not type — this is a
  hint, not a finding)

Suggested resolution (one of):
- Remove `--dry-run` (skill uses in-skill filter instead)
- Replace `7d` with `7.0`
- Confirm the skill's intent matches the new surface
```

### CLI surface

- `skill-doctor check` — walks skills, writes any new proposals
  (deduped against existing pending proposals so re-runs don't
  pile up). Prints a one-line summary (`N drift findings, M new
  proposals`).
- `skill-doctor proposals list` — table of pending proposals.
- `skill-doctor proposals show <ULID>` — print the proposal.
- `skill-doctor proposals reject <ULID>` — mark `status:
  rejected` (skill intent overrides manifest, e.g., the flag is
  about to be added).
- `skill-doctor proposals promote <ULID>` — print a recommended
  shell command to apply the fix (e.g., `sed` invocation), but
  do not actually edit (mirrors `recall observe`'s
  promote-as-recommendation pattern).

### What's out of scope

- Auto-edit of skill files (Fleet 2 candidate
  `drift-skill-doctor-fix`).
- Walking config files under `~/.config/**` (Fleet 2 candidate
  `drift-config-files`).
- Parsing multiline backslash-continuations, heredocs, or
  shell-variable-expanded commands. Fleet 1 covers single-line
  invocations only.

## Acceptance criteria

1. `cargo build --release` green on a fresh clone; `cargo test
   --release --lib` green.
2. `skill-doctor check` against the live `~/.claude/skills/`
   produces ≥4 drift proposals matching the four live instances
   in `visions/drift.md`: `pevent gc --dry-run`, `pevent gc
   --older-than 7d` (the `7d` value rejection is a hint, not a
   required finding), `bpolicy status --format`, the missing
   binaries in self-review's bootstrap-symlinks list, and the
   `ctrace ls` in dream. (The drift-fix PRD may have landed and
   resolved some; in that case, the four-instance check applies
   against the pre-fix state, verified via git history.)
3. Proposals are written under
   `~/.claude/skill-doctor/proposals/<ULID>.md` with the schema
   shown in §"Proposal output" above.
4. Re-running `skill-doctor check` does not create duplicate
   proposals for the same `(skill_path, line, binary, drift_kind,
   detail)` tuple — pending proposals are deduped by content
   hash.
5. `skill-doctor proposals reject <ULID>` flips `status:
   rejected` and the proposal disappears from `proposals list`.
6. The crate handles a missing tool-manifest gracefully: if
   `~/.claude/tool-manifest/manifest.json` does not exist,
   `skill-doctor check` prints a friendly error directing the
   user to `tool-manifest sync` and exits 2.
7. False-positive rate ≤30% on the live skill set: at least 70%
   of generated proposals reflect *actual* drift (verified by
   manual review of the first batch). Mirrors
   `freshness/recall-doctor-claims` AC5.
8. The proposal queue path is created with mode 0700 (private
   to the user), matching `recall observe`'s convention.
9. The crate is published to `github.com/j0yen/skill-doctor`
   under MIT+Apache-2.0 dual license, with a README citing the
   drift vision and the dependency on tool-manifest.
10. `~/.local/bin/skill-doctor` is installed via
    `bootstrap/install.sh`, and the crate gets a row in
    `~/wintermute/REPOS.md`.
11. AC10 (verified-completed): one user-promoted proposal lands
    as a skill edit — the user reviews skill-doctor's first
    batch, runs `skill-doctor proposals promote <ULID>`, applies
    the recommended edit by hand, and confirms the skill no
    longer contains the drift. This proves the loop closes.
    (Mirrors `freshness/recall-doctor-claims` AC10.)

## Notes for /build

- Standard `/autobuilder` rust-cli flow.
- The extraction heuristic is the meatiest part: budget time on
  it. Test against handwritten fixtures (a tiny SKILL.md with
  known drift in a fenced block, inline backtick, env-prefixed
  call, etc.) before running on the live set.
- Cross-skill diff handling: if two skills cite the same drifting
  invocation, write one proposal per occurrence so the user can
  fix them independently.
- Do NOT auto-sync the manifest in `skill-doctor check` —
  consumers should call `tool-manifest sync` explicitly, so
  `skill-doctor` doesn't shell out to another binary in its hot
  path. (`skill-doctor check` reads the JSON file directly.)

## Dependencies

- **tool-manifest** must ship first; `skill-doctor` reads
  `~/.claude/tool-manifest/manifest.json` as ground truth.
- The drift-fix PRD (`drift-fix-self-review-dream`) does NOT
  block skill-doctor — they're complementary. drift-fix closes
  the four known cases; skill-doctor catches future cases.

## Cross-fleet notes

- Composes naturally with `freshness/recall-doctor-claims`: same
  proposal-queue idiom, different target. A future
  unified-proposals skill (Fleet 2) could merge both queues.
- No collision with chord, cadence, continuity, handshake, or
  wintermute fleets.
