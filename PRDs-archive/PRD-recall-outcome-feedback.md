# PRD: recall outcome feedback (codename: *weather*)

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** `recall` v0.4 (touch, confidence, supersedes).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Sibling to:** `PRD-recall-observer-correlation.md` (the observer
emits "this memory was wrong" signals; weather routes them to
confidence updates).

---

## TL;DR

Every memory in recall today carries a static `confidence` field
written at creation time. It never changes unless the user manually
`recall update`s it. That makes recall's ranking blind to whether a
memory was *useful* on the turns it surfaced. `weather` adds two
lightweight signals:

1. **Implicit-accept**: a memory surfaced (via SessionStart hook or a
   `query --touch` call) and the user did not contradict it within the
   session window — small confidence bump (+0.02 per session, capped).
2. **Implicit-reject**: a memory surfaced and the user explicitly
   contradicted it (corrective language detected by `braid`, or an
   explicit `recall down <id>`) — confidence decay (-0.10 per signal).

Both signals are written by a single new subcommand `recall feedback`
that takes `--accept <id>...` / `--reject <id>...` and updates the
SQLite meta row plus the markdown frontmatter. The braid correlator
emits these signals when it parks a proposal; the SessionStart hook
emits the implicit-accept signal at end-of-session via the Stop hook.

The result: memories that consistently mislead drift down to where
ranking deprioritizes them, without anyone having to remember to mark
them as wrong. Memories that consistently help drift up. Neither runs
away (capped + decayed).

---

## 1. Why this exists

PRD §5 delight #2: "Outcome feedback that updates confidence based on
accept/reject signals." Today that's "unbuilt." The closest thing v0.4
ships is `recall touch`, which bumps `recall_count` — but a memory can
have a high recall count and still be wrong. Recall count measures
*frequency of surfacing*, not *accuracy when surfaced*.

The cost of missing this signal:

- **Stale memories never lose ranking weight.** A memory from 2026-01
  about how the autobuilder used to work still ranks the same in 2026-06,
  even though the autobuilder has self-evolved 24 receipts in the
  meantime. The user has to `supersede` it manually.
- **Wrong memories from `braid` can't be down-weighted.** When `braid`
  parks a proposal that the user discards, today nothing happens to
  the original memory that *led* to the proposal. The signal is
  thrown away.
- **No mechanism for slow trust building.** When a v0.4 hook writes a
  reflective memory at confidence 0.5, the only way it ever rises to
  0.7 is manual `recall update`. There's no slow accumulation from "this
  has helped 12 times and been wrong 0 times."

---

## 2. What this builds

### 2.1 New subcommand: `recall feedback`

```
recall feedback --accept <id> [<id>...]   # bump confidence (cap 0.95)
recall feedback --reject <id> [<id>...]   # decay confidence (floor 0.05)
recall feedback --abstain <id>            # explicit no-op (clears any pending signal)
```

Default deltas (configurable via `recall.toml`):

```toml
[feedback]
accept_delta  = 0.02   # per signal
reject_delta  = 0.10   # per signal
ceiling       = 0.95   # max confidence
floor         = 0.05   # min confidence
half_life_d   = 90     # natural decay back toward 0.5 (see §2.3)
```

The subcommand:
1. Reads the memory's current confidence.
2. Applies `clamp(confidence + delta, floor, ceiling)`.
3. Writes back to both the markdown frontmatter (canonical state) and
   the SQLite meta row.
4. Increments a new `feedback_count` column for observability.

### 2.2 Implicit-accept on session end

Stop hook (already wired in v0.4) gains a step: walk
`session/<sid>/recalled.json` — a list written by the SessionStart hook
when memories were emitted — and call `recall feedback --accept` on
every id that wasn't explicitly rejected. One bump per session, not per
turn.

### 2.3 Slow decay toward neutrality

A memory's confidence quietly decays toward `0.5` at a configurable
half-life. Without this, accept-bumped memories ratchet up forever and
stop responding to new evidence. `recall doctor --apply-decay` is the
maintenance hook (or `recall feedback --decay-sweep`).

The decay formula: `confidence' = 0.5 + (confidence - 0.5) * 2^(-days/H)`
where `H = half_life_d`. Pure linear interpolation toward 0.5 over
time. Memories at confidence 0.5 stay at 0.5. Memories at 0.95 drift
back to ~0.72 after one half-life and ~0.61 after two.

### 2.4 Ranking integration

No change to `retrieval::score`. Confidence already contributes
`w.confidence * hit.confidence`. The whole point of `weather` is that
confidence becomes a *real* signal instead of a constant.

---

## 3. Non-goals

- **Per-turn feedback.** v1 only writes at session end (Stop hook) and
  on explicit `recall feedback` invocation. Per-turn write is daemon
  territory (sibling PRD: `current`).
- **Auto-supersede on N consecutive rejects.** Tempting; out of scope
  for v1. The user can manually supersede; we just lower the rank.
- **Confidence floor < 0 or ceiling > 1.** The schema clamps to [0, 1];
  config knobs let the user tighten further, never loosen.
- **A "boost" command separate from feedback.** Just `--accept` it.

---

## 4. Risks

- **Confidence drift from noisy signals.** If `braid` produces false-
  positive rejects (corrective language detected when the user wasn't
  actually correcting), accepted memories get penalized. *Mitigation:*
  the `reject_delta` (0.10) is large enough to matter but small enough
  that 3 false rejects ≈ 1 honest reject; the decay sweep reverses
  small drift over weeks; `recall doctor` reports memories whose
  confidence drifted ≥ 0.3 from creation as candidates for review.
- **The Stop hook writes to a session-scoped state file that we now
  need to maintain.** *Mitigation:* small (< 1KB), under `~/.cache/`,
  cleaned by the Stop hook itself. Same pattern as `braid`.
- **Decay during long inactivity.** A user who returns to recall after
  6 months sees all their high-confidence memories near 0.6 instead of
  near 0.9. *Mitigation:* document this; offer `--no-decay-sweep` as a
  flag; tune `half_life_d` upward (180? 365?) if the default is wrong.

---

## 5. Acceptance tests

1. `recall feedback --accept <id>` raises confidence by exactly
   `accept_delta` (default 0.02) and bumps `feedback_count` by 1.
2. Stop hook integration: after a session that surfaced 4 memories
   without rejection, all 4 confidences are bumped exactly once.
3. `recall feedback --reject <id>` lowers confidence by `reject_delta`
   (default 0.10) and floors at `floor` (default 0.05).
4. Decay sweep run twice on the same day is idempotent (no
   double-decay within one day).
5. A memory at confidence 0.95 decays to within ±0.01 of 0.725 after
   one half-life (default 90 days).
6. Ranking: with two otherwise-identical memories at 0.9 and 0.5,
   the 0.9 ranks higher by exactly `w.confidence * 0.4` in score.
7. `recall doctor` JSON includes a `confidence_drift` field listing
   memory ids whose confidence has moved ≥ 0.3 from creation.

---

## 6. Phasing

Rebased 2026-05-25: v0.5.0 reserved for `recall-daemon` (iter-1 scaffold
already in-repo; UDS daemon is the higher-leverage foundation since
`weather`'s observer-correlation source eventually wants a long-lived
listener). `weather` rebases to v0.5.1+, daemon-aware from the start.

Rebased 2026-05-26: v0.5.1 (stop-hook session_id fix), v0.5.2
(daemon start/stop + doctor liveness), and v0.5.3 (bash-response-richness,
WIP at commit 789c788) all shipped or are in_progress. `weather` rebases
its phasing to v0.6.x to avoid colliding with the active 0.5.3 work.

- **6a (v0.6.0):** `recall feedback` subcommand + `feedback_count`
  column + decay sweep (manual invocation). Stand-alone — no daemon
  dependency; usable against the file-on-disk store. Minor bump
  because it introduces a new subcommand and a schema column.
- **6b (v0.6.1):** Stop-hook integration (recalled.json + auto-accept).
- **6c (v0.6.2):** `braid` integration (reject signal on proposal
  discard). If `recalld` is live by this point, route the bump through
  the daemon socket instead of re-opening SQLite per call.
- **6d (deferred):** Per-turn feedback via daemon; auto-supersede.

---

## 7. Open questions

- Should `recall query --touch` also imply `--accept`? Today `--touch`
  bumps `recall_count`; the user has to opt in to confidence movement
  separately. The cleanest answer is "no, keep them orthogonal" — touch
  measures frequency, feedback measures quality. But a single-flag
  ergonomics improvement (`--touch-and-accept`?) might be worth it.
- Decay sweep cadence: cron? `/self-review`? On every `recall doctor`?
  Probably `/self-review`'s daily pass is the right home.
