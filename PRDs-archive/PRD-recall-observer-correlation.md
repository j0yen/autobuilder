# PRD: recall observer correlation (codename: *braid*)

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** `recall` v0.4 (`observer.rs`, `hooks/post-tool-use.sh`).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
build_version_bump: patch
**Sibling to:** `PRD-recall-daemon.md` (the daemon eats this correlator as
a natural component once it exists).

---

## TL;DR

The v0.4 `recall observe` heuristic catalog needs `user_prompt_after` to
propose anything — but a single `PostToolUseFailure` event can't carry the
next user prompt, because that prompt hasn't been written yet. As a result
the wire-up shipped in v0.4 is structurally correct and functionally
inert: zero proposals get parked, ever. `braid` adds the missing piece —
a session-scoped state file that pairs the most recent error with the
next `UserPromptSubmit` and feeds the joined event to `recall observe`
synchronously, in the prompt-submit hook's milliseconds-budget window.

The whole change is a state file under `~/.cache/recall-braid/` plus two
hook scripts (replacing `recall-post-tool-use.sh` and adding a
`recall-user-prompt.sh`). No recall binary change required. The observer
already accepts the right input shape; we just need to assemble it.

---

## 1. Why this exists (the structural gap)

v0.4 wired a PostToolUseFailure hook that translates the harness payload
to the observer's `Event` schema:

```jsonc
{ "tool_name": "...", "tool_input": {...}, "tool_response": {...},
  "status": "error", "user_prompt_after": null }
```

The observer's heuristic 1 (PRD §4b.16 — "tool error followed by
corrective user prompt") requires `user_prompt_after` to be a non-empty
string containing language like `"no wait"` / `"undo"` / `"actually"` /
`"don't"`. With it null, the heuristic short-circuits and no proposal
gets parked.

The fundamental issue: `PostToolUseFailure` fires *before* the next user
turn. Even if the failure was followed by silent acceptance, the hook
doesn't know that yet. So a single-event observer with the right
heuristic but no correlation produces a 100%-precision, 0%-recall
classifier.

## 2. What this builds

A two-hop correlator:

1. **`recall-post-tool-use.sh`** (replaces v0.4 version): on error,
   write a JSON record to `~/.cache/recall-braid/<session>/last-error.json`.
   Do *not* call `recall observe`. State-file write is atomic
   (tempfile + rename) and includes a monotonic-clock timestamp so
   stale records can be discarded.

2. **`recall-user-prompt.sh`** (new, wired to `UserPromptSubmit`): read
   `last-error.json` if present and recent (≤ 60s), JOIN it with the
   incoming prompt's text, and pipe the merged event to
   `recall observe`. Then delete the state file (so the same error
   isn't paired with multiple subsequent prompts).

The recall binary gains one new helper, `recall observe --emit-jsonl`,
which has no behavior change — it just confirms the input parse for
hook-script debugging.

### State-file layout

```
~/.cache/recall-braid/
└── <session-id>/
    └── last-error.json    # { ts_unix, tool_name, tool_input, tool_response }
```

Per-session subdir so concurrent sessions never share state.
`<session-id>` is `$CLAUDE_SESSION_ID`; if absent, the hooks no-op.

### Why a state file and not a daemon

Two reasons: (a) `braid` should ship before the daemon does, since the
daemon PRD (sibling) lists this as a prerequisite; (b) the read-then-
delete pattern at UserPromptSubmit is well-suited to filesystem state —
no IPC contract, no socket cleanup, no lifecycle management. The state
file is < 1KB, lives in `~/.cache/`, and gets reaped on session end.

## 3. Heuristics this unlocks

The original §4b.16 heuristic 1 starts working:

- **Tool error + corrective language in next prompt** → propose a
  reflective/self memory: "When I {tool_name} with {input excerpt} I got
  {error excerpt}, and the user told me {corrective excerpt}." Confidence
  0.4. Subject `self`, kind `reflective`.

Two new heuristics become tractable with the same infra:

- **Edit reverted in the next prompt.** If the user's prompt contains
  `"revert"` / `"undo that"` / `"put it back"` and the most recent error
  is None (so it's not error-driven), check whether the previous tool
  was `Edit` or `Write`. If so, propose a reflective memory tying the
  rollback to the original edit's `file_path`.

- **Repeated correction.** If the same `tool_name` errors twice within
  the same session window (state file gets two writes before any
  UserPromptSubmit clears it), propose with higher confidence (0.6).
  This catches "I keep doing the wrong thing" patterns that single-shot
  errors miss.

## 4. Non-goals

- Building the daemon. The state-file approach is enough for ≤ a few
  proposals per session. Per-turn retrieval and sub-millisecond
  correlation are the daemon's concern (sibling PRD).
- LLM-grading the proposal. The observer parks; the user reviews. The
  v0.4 `recall proposals` surface already handles the human gate.
- Auto-applying proposals. Even high-confidence proposals stay parked.

## 5. Acceptance tests

1. Bash error followed by `"no wait, that path was wrong"` produces
   exactly one proposal under `<root>/proposals/<id>.md` with the joined
   excerpt visible in the body.
2. Bash error followed by an *unrelated* prompt (e.g. `"now run the
   tests"`) produces zero proposals. The state file is still cleared.
3. Two errors in a row, then a corrective prompt — produces a single
   proposal that mentions the most recent error (not both).
4. State file older than 60s is treated as expired and ignored; no
   stale pairings.
5. Concurrent sessions don't cross-pollinate: a state file in
   `<sess-A>/` is invisible to `<sess-B>`'s UserPromptSubmit handler.
6. If recall is uninstalled or the JQ binary is missing, both hooks
   silently no-op (verified by `set -e` not triggering).

## 6. Risks

- **The hook chain blocks the prompt-submit critical path.** UserPromptSubmit
  is on the latency-sensitive path. *Mitigation:* `async: true` in the
  hooks block; the observer is already non-blocking from the user's POV
  (proposals are reviewed later). Cap the observer at a 200ms budget;
  drop the event if it exceeds.
- **The state file leaks if `Stop` doesn't fire** (e.g. crash, force-kill).
  *Mitigation:* a 60s freshness check on the read side; a `recall doctor`
  enhancement to report stale braid state and offer `--cleanup`.

## 7. Phasing

Single-iteration v0.4.1. Estimated: half a day. No new binary
subcommands; two shell scripts and one settings.json edit.

---

## 8. Open questions

- Do we want PreCompact integration? After compaction, the model has
  effectively forgotten the recent error context. Could we surface
  recent unmerged proposals as a "things you almost remembered" block?
  Probably not for v0.4.1 — it's a separate UX experiment.
