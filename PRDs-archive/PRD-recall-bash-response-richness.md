# PRD: recall braid — capture richer Bash error context

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** recall v0.4.2 (braid correlator, env-vs-JSON fix shipped).
build_auto: true
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
build_version_bump: patch

---

## TL;DR

Braid proposals derived from Bash failures currently render as
`Tool error: {}` in the body. The Claude Code harness's
`PostToolUseFailure` payload sets `tool_response: {}` for Bash —
neither stderr nor the failing command surfaces. The proposal still
fires (heuristic gates on `status:error` + corrective language), but
loses the most informative half of the context, which is exactly what
makes a reflective memory useful later.

Fix: in `recall observe`, include `tool_input.command` and any
`tool_input.description` in the proposal body when `tool_response` is
empty. Optional stretch: have `recall-post-tool-use.sh` capture stderr
from a parallel `pevent`-style tail of the failing turn — but that's
strictly out of scope here; the in-binary fix lands first.

---

## 1. Why this exists

Verified live 2026-05-25 (session `d10dd15a-...`): three braid
proposals landed across AC1/AC2/AC3 testing of
`recall-observer-correlation`. All three rendered as:

```
Bash call failed and the user corrected with: "<phrase>"

Tool error: {}
```

The `{}` is the literal JSON serialization of the harness's empty
`tool_response`. The proposal records the corrective excerpt and the
tool name, but nothing about *what specifically failed* — which is the
single most useful signal for a reflective memory ("I keep doing X").

## 2. What this builds

Modify `src/observer.rs::classify()` to construct a richer body when
`tool_response` is null/empty/`{}`:

```rust
let detail = if ev.tool_response.is_null()
    || matches!(&ev.tool_response,
                serde_json::Value::Object(m) if m.is_empty())
    || matches!(&ev.tool_response,
                serde_json::Value::String(s) if s.is_empty())
{
    // Fallback: use tool_input.command (Bash) or .file_path (Edit/Read).
    extract_input_signal(&ev.tool_input)
} else {
    first_n_chars(&ev.tool_response.to_string(), 400)
};
```

Where `extract_input_signal` returns, in priority order:
- `tool_input.command` for Bash (first 400 chars)
- `tool_input.file_path` for Edit/Read/Write
- `tool_input.description` if neither is present
- empty string otherwise

The proposal body becomes:

```
Bash call failed and the user corrected with: "<phrase>"

Tool: Bash
Input: <command excerpt or file_path>
```

When `tool_response` IS populated, behavior is unchanged (current text).

## 3. Heuristics this affects

Heuristic 1 (tool-error + corrective) — body becomes richer; gating
logic unchanged. Confidence stays 0.4. No new heuristics added.

## 4. Non-goals

- Capturing live stderr/stdout out-of-band. That requires `ctrace` or
  shell-level interception; a separate proposal.
- Re-running the failing command to reproduce its error. Side-effects
  are too risky; a memory should record what happened, not retry it.
- Embedding the full failing command verbatim when it contains secrets
  or paths over 400 chars. Truncation stays at 400 chars.

## 5. Acceptance tests

1. A Bash failure with empty `tool_response` produces a proposal whose
   body includes the failing command's first 400 chars under an
   `Input:` line.
2. A Bash failure with a non-empty `tool_response` (e.g. an error
   string the harness *did* surface) keeps the current body shape;
   `Input:` line is omitted to avoid duplication.
3. An Edit failure with empty `tool_response` records
   `tool_input.file_path` under `Input:`.
4. A tool whose `tool_input` has none of {command, file_path,
   description} produces a body with `Input:` line absent (graceful
   degradation, no panic).
5. Existing observer unit tests (`error_plus_correction_yields_proposal`,
   `ok_call_yields_nothing`, `run_writes_proposal_file`) still pass
   unchanged.
6. End-to-end: after fix lands and binary is reinstalled, the next
   real Bash failure + corrective prompt produces a proposal that
   names the failing command. Verify with `recall proposals | tail`.

## 6. Risks

- **Leaking secrets in proposals.** A failing `curl -H "Authorization:
  Bearer SECRET"` would surface the token. *Mitigation:* truncate at
  400 chars (current `first_n_chars` budget); also propose a separate
  PRD for a redaction filter (regex set: passwords, tokens, keys).
  v0.4.3 ships the truncation only; redaction is a follow-on.
- **Body grows unbounded for long commands.** Already capped at 400
  chars; no change.

## 7. Phasing

Single-iteration v0.4.3 patch. Estimated: ~1 hour. Pure Rust change in
`src/observer.rs` plus three new unit tests. No hook scripts touched.
