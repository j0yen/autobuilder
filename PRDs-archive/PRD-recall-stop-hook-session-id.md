# PRD: recall — fix session_id source in Stop hook (same shape as v0.4.2)

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** recall v0.4.2 (braid hooks now read session_id from JSON).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
build_version_bump: patch

---

## TL;DR

The v0.4.2 fix (braid hooks read `.session_id` from JSON, not
`$CLAUDE_SESSION_ID` from env) corrected `post-tool-use.sh` and
`user-prompt-submit.sh`. But `recall-stop.sh` (the Stop hook that
promotes session scratch entries to long-term memory) has the same
bug: line 14 reads `sid="${CLAUDE_SESSION_ID:-}"` and exits silently
when empty — which is *always*, because the harness doesn't export
that env var. Result: the scratch→memory promotion pipeline has been
silently broken since it shipped.

Fix: same shape as v0.4.2 — read `.session_id` from JSON stdin
first, env as fallback. One file, ~5 lines.

---

## 1. Why this exists

The braid v0.4.2 diagnosis revealed that the harness passes
session id in the input JSON's `.session_id` field, NOT as
`$CLAUDE_SESSION_ID` env var. The braid hooks now read JSON-first;
`recall-stop.sh` does not.

Direct evidence: `recall-stop.sh` invokes
```sh
"$RECALL_BIN" promote --session "$sid" --format text 2>&1 | sed ...
```
where `$sid` is `"${CLAUDE_SESSION_ID:-}"`. With env empty, `$sid` is
empty, and `recall promote --session ""` either no-ops or errors. The
hook has `|| true` on the pipe so any failure is silent.

Net: every session-end that should have promoted scratch entries to
long-term memory has silently dropped them on the floor. Magnitude
unknown without retroactive analysis; could be zero (if no one used
`recall scratch write` in real sessions) or large (if the daily
self-review or other skills wrote scratch and expected promotion).

## 2. What this builds

Modify `hooks/session-end.sh` (the source of the
`recall-stop.sh` symlink in `~/.claude/scripts/`):

```sh
# v0.4.5: read session_id from JSON payload (harness doesn't export it
# as $CLAUDE_SESSION_ID — same fix as braid v0.4.2).
JQ="${JQ:-/usr/sbin/jq}"
[ -x "$JQ" ] || exit 0

raw="$(cat -)"
sid="$("$JQ" -r '.session_id // empty' <<<"$raw" 2>/dev/null)"
sid="${sid:-${CLAUDE_SESSION_ID:-}}"
[ -n "$sid" ] || exit 0
```

Replaces the existing `sid="${CLAUDE_SESSION_ID:-}"` line and the
implicit no-stdin assumption. Maintains the silent-on-anything
invariant from the original hook.

## 3. Other hooks that might have the same bug

A grep for `CLAUDE_SESSION_ID` across `~/wintermute/recall/hooks/`
should reveal any siblings. As of 2026-05-25:
- `post-tool-use.sh` — fixed in v0.4.2
- `user-prompt-submit.sh` — fixed in v0.4.2
- `session-end.sh` — fix in this PRD (v0.4.5 if standalone,
  or rolled into a single 0.4.x convoy if shipped with the other
  follow-ons)

The session-start hook (`recall-session-start.sh`) does NOT use the
session id — it just emits memories regardless — so it's
unaffected.

## 4. Non-goals

- Backfilling missed promotions from past sessions. Scratch entries
  expire on session end; reconstructing them post-hoc is impossible
  and not worth the engineering.
- Auditing how much scratch data was actually lost. The hook was
  silent; logs don't exist. Document the gap, move on.
- Migrating the scratch storage to a session-id-derived path so the
  Stop hook can recover dropped state. Out of scope; the bug is in
  the env-vs-JSON read, not in the scratch model.

## 5. Acceptance tests

1. `recall scratch write` followed by simulated session-end (pipe
   `{"session_id":"<id>"}` to `session-end.sh`) promotes the scratch
   entry to long-term memory; `recall list --subject <id>` shows
   the promoted record. Negative control: same setup but no
   session_id in the JSON payload → no promotion.
2. With `$CLAUDE_SESSION_ID` set as env (forward-compat) and JSON
   omitting `.session_id`: promotion still works (env fallback).
3. Missing JQ binary → exit 0, no error, no promotion (silent
   no-op preserved).
4. Existing v0.4.2 hooks unaffected (regression check — diff their
   files in this PR's commit, expect them untouched).

## 6. Risks

- **Promotion side-effects on real sessions.** Once fixed, scratch
  entries WILL promote on real session-end. If self-review or build
  has been writing scratch under the assumption nothing escaped,
  there could be a surge of new long-term memories the first time
  the fix lands. *Mitigation:* run `recall scratch list` before
  shipping; if non-empty, confirm with user before merge.
- **None other** — the fix is a literal-shape copy of the v0.4.2
  pattern.

## 7. Phasing

Single-iteration patch (v0.4.5 if the bash-response and freshness
PRDs ship first as v0.4.3 + v0.4.4; or v0.4.3 if it ships solo).
Estimated: ~15 minutes. One hook file, three new unit-shape tests
(synthetic-invocation against the script), one CHANGELOG entry.
