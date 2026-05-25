# PRD: recall braid — relax freshness gate for human-paced turns

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** recall v0.4.2 (braid correlator, session_id-from-JSON fix).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
build_version_bump: patch

---

## TL;DR

The braid 60s freshness gate is calibrated for autonomous-paced
turns. In an interactive session, a 2-minute pause to read a long
assistant message before typing the corrective reply is normal —
trips the gate, drops the event, and the user perceives the system
as silently broken. Live verification 2026-05-25 hit exactly this:
AC1's first attempt failed not because the chain was broken (it
wasn't, v0.4.2 had just shipped) but because the human read+type
gap was ~120s.

Fix: raise the default to 300s (5min) and surface
`$RECALL_BRAID_MAX_AGE` as a documented knob. The hook code already
reads from env; only the default constant and documentation change.

Rationale: a memory written from a 4-minute-old error is still
useful; a memory written from a 4-hour-old error is noise. 300s is
a comfortable upper bound on "read assistant message + decide how to
reply" while still well short of session-context drift.

---

## 1. Why this exists

`recall-user-prompt.sh` line 25 hardcodes a 60s default:
```sh
MAX_AGE_SEC="${RECALL_BRAID_MAX_AGE:-60}"
```

That value made sense for the original braid PRD's mental model: an
agent rapidly iterating, errors and corrections seconds apart. In
practice, the slowest leg is reading the assistant's output, and
that's frequently 30-120s for non-trivial responses. A 60s gate
turns plausible cycles into silent drops.

Symptom verified live: 2026-05-25, session d10dd15a-... — AC1 first
attempt produced state file at 20:48, user's corrective prompt
arrived at ~20:50. State was cleared correctly (read-then-delete
held), but the freshness gate dropped the event. No proposal. Hand-
verified the fix worked by retrying with sub-30s turnover.

## 2. What this builds

One constant change in `hooks/user-prompt-submit.sh`:
```sh
MAX_AGE_SEC="${RECALL_BRAID_MAX_AGE:-300}"
```

Plus a documentation block in the hook header explaining the knob and
its rationale. Plus a one-liner mention in `README.md` (under
"Configuration") so the env var is discoverable.

That's it. No Rust change. No new dependencies.

## 3. Why 300s and not larger

- A correction issued >5min after the error is increasingly likely
  to be about *something else* the user noticed in the interim,
  producing false-positive memories.
- 300s comfortably covers human read+type latency on any plausible
  message size.
- 5min is the prompt-cache TTL on Anthropic's side, a happy
  coincidence for cache-line reasoning.

A power user can still set `RECALL_BRAID_MAX_AGE=900` for slower
sessions; the env var stays.

## 4. Non-goals

- Adaptive freshness based on previous prompt-arrival cadence. Too
  much machinery for an open-loop heuristic. Stays static.
- Changing freshness on the write side (post-tool-use). The ts_unix
  is recorded at fail-time; freshness is evaluated only on read.
- Renaming the env var. `RECALL_BRAID_MAX_AGE` stays as-is for
  config-file compatibility.

## 5. Acceptance tests

1. State file written with ts_unix = now - 250s, followed by a
   corrective UserPromptSubmit: proposal lands (within the new 300s
   default).
2. State file written with ts_unix = now - 320s, followed by a
   corrective prompt: no proposal (still beyond the gate).
3. With `RECALL_BRAID_MAX_AGE=60` env override, ts_unix = now - 90s:
   no proposal (override still honored).
4. Existing AC4 (60s) synthetic test (already passing) needs to be
   updated to reflect the new default — change expected age from
   "120s ignored, 30s accepted" to "320s ignored, 250s accepted",
   with the env override case as a separate test.
5. README.md "Configuration" section names the env var and explains
   the trade-off.

## 6. Risks

- **More false-positive proposals from stale errors.** The window
  grows 5x. Empirically the user reviews proposals before promotion,
  so noise is annoying but not damaging.
- **No new code paths.** This is a literal-value change; the risk
  surface is just the gate's semantic correctness, which is
  unchanged.

## 7. Phasing

Single-iteration v0.4.4 patch. Estimated: ~15 minutes including
README edit. Pairs naturally with the bash-response-richness PRD —
could ship together as v0.4.3 if both land same session, but
separate PRDs keep the audit trail honest.
