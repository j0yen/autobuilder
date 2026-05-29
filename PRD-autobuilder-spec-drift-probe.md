# PRD: autobuilder-spec-drift-probe — pre-Stage-3 ground-truth check against PRD-named tools

**Status:** Draft v0.1
**build_target:** self-mod
**build_priority:** high
**build_into:** /home/jsy/.claude/skills/autobuilder
**Research:** research/quality-verification-2026-05-28.md §2c, §4 Test 1
**Created:** 2026-05-28
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Before /autobuilder enters Stage 3, probe every external tool the PRD
backticks (`recall list --since 7d`, `letter-curate aggregate`, etc.).
Run `<tool> --help`; diff PRD-asserted verbs against the tool's actual
subcommand list. Block Stage 3 if the PRD names verbs that don't exist.
Costs <2s; catches the single most expensive observed failure mode.

## Why this exists

Per `~/brain/journal/build-auto.log`, cadence-bind-letters iter-1
discovered post-Stage-3 that the PRD assumed `letter-curate aggregate`
exists when the binary only does `triage` / `show` / `list`. Burned
one full /autobuilder cycle. With /build's 5-way parallel dispatch
(landed 2026-05-28), four sibling branches could chase analogous
drifts concurrently before the mismatch surfaces. Catching at
Stage 2.5 makes that wasted concurrency cost go to zero.

Research report §2c + §4 Test 1 traces evidence and rationale.

## What this builds

### Artifact

A Python script at `~/.claude/skills/autobuilder/scripts/spec-drift-probe.py`
(stdlib only, <300 LOC) and a JSON schema at
`~/.claude/skills/autobuilder/schemas/spec-drift.schema.json`.

CLI:
```
spec-drift-probe.py <PRD-path> [--out <json-path>] [--strict]
  → writes target/autobuilder/spec-drift.json (or --out target)
  → exit 0  no drift detected
  → exit 4  drift detected (one or more PRD-asserted verbs not in tool surface)
  → exit 2  bad invocation
```

### Detection algorithm

1. Parse PRD markdown body. Extract backticked spans matching the
   regex `^[a-z][a-z0-9-]*( +[a-z][a-z0-9-]*)*( +--?[a-z][a-z0-9-]*)*`.
   Filter known false positives (Rust idents like `let`, `fn`, `use`).
2. For each unique binary name:
   - `command -v <binary>` → if missing, mark `verdict: unavailable`
     (intentional future-state; not drift).
   - `<binary> --help 2>&1` → capture stdout. If exit != 0, mark
     `verdict: help_fails`.
   - For each second-token verb mentioned in the PRD for that binary,
     check whether it appears in the help output's subcommand list.
     Heuristic: grep for the verb as a standalone word after at least
     two leading spaces (clap/argparse subcommand-list conventions).
3. Emit JSON receipt:
   ```json
   {
     "schema": "spec-drift.v1",
     "tools": [{
       "binary": "letter-curate",
       "found": true,
       "asserted_verbs": ["aggregate"],
       "actual_verbs": ["triage", "show", "list"],
       "missing_verbs": ["aggregate"],
       "verdict": "drift"
     }],
     "summary": {"tools_checked": 1, "drift_count": 1}
   }
   ```
4. Exit 4 if any tool has `verdict: drift`. `--strict` also blocks on
   `help_fails`; default lets help-broken tools through (the PRD might
   be future-state work).

### Autobuilder integration

Edit `~/.claude/skills/autobuilder/SKILL.md` to add Stage 2.5:

> ### Stage 2.5 — Spec-Drift Probe
> Run `scripts/spec-drift-probe.py <PRD-path>` after Stage 2 scaffold,
> before entering Stage 3 iterate-and-prove. On exit 4, abort with the
> spec-drift.json contents as the diagnostic. The verdict is preserved
> as Receipt #8 in Stage 4 when Stage 3 eventually completes.

Stage 4's receipt table gains an 8th row: `spec-drift` (Stage 2.5 source,
pass condition: `summary.drift_count == 0`).

### Out of scope (v0.1)

- Rust API hallucination checks ("uses `tokio::sync::broadcast`").
  CLI surface only.
- Network-API surface checks. Local tools only.
- Auto-fixing the PRD. Block + diagnostic; human edits.

## Acceptance criteria

- **AC1**: Against a PRD body containing only `\`recall list --since 7d\``,
  the probe finds `recall`, confirms `list` is a real subcommand,
  exits 0, writes `{"summary": {"drift_count": 0}}`.
- **AC2**: Against a synthetic PRD containing `\`letter-curate aggregate\``,
  the probe finds `letter-curate`, sees no `aggregate` subcommand,
  emits `verdict: drift` with `missing_verbs: ["aggregate"]`, exits 4.
- **AC3**: Against a PRD containing `\`future-tool start\`` for a binary
  not on `$PATH`, the probe emits `verdict: unavailable`, exits 0.
- **AC4**: Receipt JSON validates against
  `schemas/spec-drift.schema.json` (the schema file is part of this
  PRD's deliverable).
- **AC5**: Probe wall time <2s against a typical PRD (10–20 backticked
  invocations). Measured via `time spec-drift-probe.py <fixture>`.
- **AC6**: **Backfill calibration.** Running the probe against the 5
  most-recently-shipped PRDs (`PRDs-archive/`, sorted by mtime desc)
  yields 0 `verdict: drift`. Any false positive blocks shipping and
  surfaces a probe bug.
- **AC7**: `~/.claude/skills/autobuilder/SKILL.md` gains the Stage 2.5
  block AND the 8th-receipt row in the Stage 4 table.
- **AC8**: Python script has 0 dependencies outside stdlib; verified
  via `python3 -c "import spec_drift_probe"` from an empty venv.

## Files

```
~/.claude/skills/autobuilder/
├── scripts/spec-drift-probe.py            # new
├── schemas/spec-drift.schema.json         # new
├── SKILL.md                                # +Stage 2.5 + 8th receipt row
└── tests/test_spec_drift_probe.py         # new (4 fixtures: AC1–AC4 inputs)
```

## Non-functional

- No network calls.
- Pure Python stdlib (no pip / no venv setup).
- Idempotent: writes only to the `--out` path.
- Receipt schema versioned (`spec-drift.v1`); breaking schema changes
  bump to v2.
