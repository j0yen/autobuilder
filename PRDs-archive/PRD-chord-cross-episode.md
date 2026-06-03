# PRD-chord-cross-episode

Status: Draft v0.1
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/episodic-observer
build_version_bump: minor
Vision: visions/chord.md

## TL;DR

Extend `episodic-observer` (single-session detector today) to detect
**cross-session episodic patterns**: errors in session A followed by
corrective writes in session B, redundant edits across sessions,
rescues (one session unblocks another). Read agorabus's heartbeat +
structured intent stream (from `PRD-chord-intent-rich`) alongside
per-session JSONL transcripts and emit episodic candidates with a
`cross_session_pattern` tag. Strict windows + same-path filters keep
false positives low.

## Why this exists

`episodic-observer/README.md` is explicit: "end-of-session JSONL
detector that surfaces the loadbearing patterns and emits candidate
memories. Stop-hook integration + recall-write are downstream;
this slice ends at `episode observe --dry-run <jsonl>`." One JSONL
per call. One session per pass. Anything that happens across two
JSONLs is invisible.

But in practice cross-session patterns *happen routinely*:

- **2026-05-24 self-review run-2:** "headless /build (PID 99933)
  running alongside this /self-review (PID 103334) plus interactive
  PID 930 … 3 concurrent claude sessions, all correctly peered in
  agorabus."
- **recall-observer-correlation shipped 2026-05-25** demonstrated
  that error+correction *within* one session is a tractable pattern
  to detect (proposals 01KSEM4BTVBDGEBJA66PAQY07E and
  01KSEMDGVTS9E2QNZMDD8G4HFR). The same shape across two sessions —
  session A fails, session B fixes — is not currently observed by any
  tool on this laptop.
- **2026-05-25 journal v0.4.2 fix:** the recall hook env-var bug was
  diagnosed and fixed in the same session that observed it. If
  diagnosis had happened in session-X and the fix in session-Y, no
  current tool would have linked them.

The chord vision (visions/chord.md §End-state #4) names this as the
fourth Fleet 1 component.

## What this builds

### New episodic-observer subcommand

```sh
episode observe-cross \
  --since 2h \
  --transcripts-dir ~/.claude/projects/-home-jsy \
  --agorabus-events ~/.cache/agorabus/events.ndjson \
  --dry-run
```

Reads:

1. All session JSONLs modified in the window (`mtime >= now - since`).
2. The agorabus event log for the same window (if present; see
   §Dependencies for the log shape). At a minimum, peer heartbeats
   with `working_paths` are needed.
3. Per-session intent records (from `agorabus intent list` snapshot
   at session start, end, and any mid-session reshape).

Emits candidate memories on stdout in the same format
`episode observe` already produces, but with two distinguishing
features:

- A new memory **kind**: `episodic-cross-session` (extending the
  existing `episodic`).
- A new top-level field **`participants`**: `[sid-A, sid-B]` (length
  ≥ 2). Single-session candidates retain `participants: [sid-A]`.

### Detector rules (v1, strict)

The PRD ships three concrete detectors. Each is a narrow rule with
explicit thresholds; broaden later only with evidence.

**Detector 1: cross-session corrective.**

Trigger: session A's transcript shows an error event (non-zero exit,
ToolResult containing "error"/"failed"/specific exit codes) on path
`P` at time `T_A`. Session B's transcript shows a successful write
to `P` (Edit / Write tool result, `ok:true`) at time `T_B` where
`0 < T_B - T_A < 300 seconds` and B's intent's `working_paths`
contained `P` (or P's parent dir) at the time of the fix.

Emits: `{kind: "episodic-cross-session", subject: "self",
participants: [A, B], context: "corrective", path: P, ...}`.

**Detector 2: redundant work.**

Trigger: session A and session B both have a successful Edit/Write
on `P` within `0 < T_B - T_A < 1 hour`. Both diffs touch overlapping
line ranges. *Neither* session had an explicit claim on `P` from
chord-claim (if chord-claim is live; otherwise just the time+path
match suffices).

Emits: `episodic-cross-session`, context `redundant`. Flagged for
review — these often suggest a missing claim.

**Detector 3: rescue.**

Trigger: session A's transcript stalls (no tool calls for ≥ 5 min)
while error events are present in its last tool call. Session B's
intent then names the same skill/PRD slug A had (from
chord-intent-rich) and B's transcript begins activity on related
paths within the next 10 min.

Emits: `episodic-cross-session`, context `rescue`. Lower precision;
documented as such.

### Dependencies

- **agorabus event log.** Today agorabus daemon doesn't persist a
  trail of pub/sub events. This PRD assumes one of:
  - Option A (preferred): a separate observer process subscribes
    to `claim.*` and the new `intent.*` topics and writes
    `~/.cache/agorabus/events.ndjson`. Implementable in a few lines
    of bash; this PRD ships it under
    `~/.claude/scripts/agorabus-event-log.sh`.
  - Option B (fallback): episode-observe-cross only uses
    `agorabus peers` snapshots and per-session intent at start/end.
    Lower fidelity for fast-moving sessions.
- **chord-intent-rich must ship first** for high-quality
  `working_paths` filtering. Without it, detectors fall back to
  best-effort path inference from transcripts.

### CLI verification (sandbox-friendly)

```sh
# Single-shot, dry-run, all output to stdout.
episode observe-cross --since 1h --dry-run

# Write candidates as recall memory candidates (post-write step is
# downstream; observe-cross only writes proposal files under
# ~/.claude/recall/proposals/cross-session/).
episode observe-cross --since 1h --write-proposals
```

## Acceptance criteria

1. **AC1 — detector 1 fires on synthetic data.** With two synthetic
   JSONLs in a temp dir (A errors on `/tmp/foo` at T,  B writes
   `/tmp/foo` at T+30s) and matching intent records,
   `episode observe-cross --since 1h --transcripts-dir <tmp> --dry-run`
   emits exactly one candidate of kind
   `episodic-cross-session`, context `corrective`,
   `participants: [A, B]`.

2. **AC2 — no false positive across sessions on unrelated paths.**
   Two JSONLs where A errors on `/tmp/foo` and B writes `/tmp/bar`
   (no overlap): observe-cross emits zero cross-session candidates.

3. **AC3 — time window enforced.** Same paths but `T_B - T_A = 400s`
   (>300s): no candidate. At 290s: one candidate.

4. **AC4 — detector 2 fires on overlapping diffs.** Two sessions
   write the same file's same line range 20 min apart, no claim
   present: one `redundant` candidate.

5. **AC5 — detector 2 suppresses when claim present.** Same setup
   as AC4 but with a `claim.acquire` event for session A in the
   agorabus event log spanning the window: zero candidates (the claim
   was an explicit "I am editing this," redundancy is expected).
   *Requires chord-claim shipped; if not, this AC is deferred to
   post-chord-claim.*

6. **AC6 — detector 3 fires on intent-named rescue.** Session A's
   transcript shows 6 min of stall after an error; session B's
   subsequent intent matches A's prior PRD slug; B's first 10 min
   touches paths from A's `working_paths`: one `rescue` candidate.

7. **AC7 — single-session pass-through unchanged.** Running the
   existing `episode observe <jsonl>` (no `-cross`) against a single
   transcript produces identical output to v0.1.0. observe-cross is
   strictly additive.

8. **AC8 — JSON schema valid.** Each cross-session candidate has
   `kind`, `subject`, `participants` (length ≥ 2), `context`
   (`corrective`|`redundant`|`rescue`), `paths[]`, `t_first_unix`,
   `t_last_unix`, `evidence: [transcript_refs]`. Validates against a
   schema shipped under `schema/episode-cross-session.schema.json`.

9. **AC9 — write-proposals flag.** With `--write-proposals`,
   candidates land as files under
   `~/.claude/recall/proposals/cross-session/` (same convention as
   recall-observer-correlation). No file is written under `--dry-run`.

10. **AC10 — version + changelog.** episodic-observer Cargo.toml
    minor bump (0.1 → 0.2). CHANGELOG entry. REPOS.md untouched.

## Risks / trade-offs

- **False positives in detector 3 (rescue).** Lowest precision of the
  three. PRD §Detector rules names this; AC6 covers only the obvious
  case. In practice, the user reviewing proposals before they become
  memories is the safety net (same convention as
  recall-observer-correlation).
- **Detector 2 needs diff overlap.** "Same line range" requires
  parsing Edit tool params. The tool-call schema in `~/.claude/projects/`
  JSONL is stable enough today; if it drifts, detector 2 degrades
  to "same file within window" with lower precision. Document the
  fallback.
- **Coupling to chord-intent-rich.** AC5 is deferred until chord-claim
  ships; the PRD reads tighter when both Fleet 1 prerequisites land
  first. If those don't ship, this PRD still works with reduced
  fidelity — explicitly degraded, not broken.
- **agorabus-event-log.sh is new infrastructure.** A small bash
  script (~30 LOC) that subscribes to a few topics and writes NDJSON.
  Documented in this PRD as a side-deliverable; if the user prefers
  it as a separate PRD, split out.

## Out of scope

- Stop-hook integration / live writing (this PRD ends at
  `--write-proposals`; the hook that runs observe-cross on session
  end is a separate change).
- Pattern detectors beyond the three named (parallel work, chained
  delegation, etc.) — add later with evidence.
- Real-time cross-session detection (event-driven). This is a
  batch detector; live detection is Fleet 2.

## Provenance

- Vision doc: `visions/chord.md` (§End-state #4, §Components #4).
- Depends on: `PRD-chord-intent-rich.md` (for high-quality
  `working_paths` filtering). Soft-depends on `PRD-chord-claim.md`
  (for AC5).
- Existing single-session base:
  `~/wintermute/episodic-observer/README.md`.
- recall-observer-correlation (shipped, archive
  `PRDs-archive/PRD-recall-observer-correlation.md`) provides the
  proposal-file convention and the within-session analog of
  detector 1.
- /dream session 2026-05-25, seed: reflection.
