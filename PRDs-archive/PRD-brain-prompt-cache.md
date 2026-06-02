# PRD: brain-prompt-cache — make every brain turn pay for its prefix once

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-brain
build_version_bump: minor
**Depends on:** (none — independent, ship first)
**Codename:** *ledger* — stop re-billing what hasn't changed.

## TL;DR

`wintermute-brain` re-sends and re-bills its entire stable prefix — persona,
tool definitions, recall context, conversation history — at full input-token
rate on *every* turn, because the request it builds carries **no
`cache_control` breakpoints**. Its own acceptance card (AC3) targets a ≥60%
prompt-cache read ratio that the implementation never attempts. This PRD adds
`cache_control` ephemeral breakpoints to the Anthropic request and restructures
how context is composed so the **stable persona prefix is cacheable** and the
**volatile recall context no longer busts the cache** every turn. No call
volume changes; this is a pure per-call cost reduction with zero quality
tradeoff.

## 1. Why this exists

- **The request struct has no cache field.** `wintermute-brain/src/anthropic.rs`
  defines `MessageRequest { model, max_tokens, system: Option<String>, messages:
  Vec<Message> }` (verified 2026-05-29). There is no `cache_control` anywhere in
  `src/*.rs` — the only occurrence of the string in the repo is in
  `agent/intent-card.json` (the spec), not the code.
- **AC3 already demands this.** `agent/intent-card.json:30` specifies a test
  asserting `sum(cache_read) / sum(input_tokens) ≥ 0.6` over a recorded 50-turn
  fixture. The target exists; the mechanism doesn't.
- **The current composition actively defeats caching.** `src/daemon.rs`
  `compose_persona(base, child_lock, recall_context)` splices the per-turn
  recall hits *into the system prompt*. Anthropic prompt caching keys on a
  stable prefix; because recall hits differ every turn, the system string
  differs every turn, so even adding a breakpoint to `system` as-is would cache
  almost nothing. The volatile content must move out of the cached prefix.
- **The companion loop is the ideal caching workload.** A long-lived
  conversation with a large, fixed persona + tool defs re-sent every turn is the
  textbook case where ephemeral (5-min TTL) caching pays off: ~90% discount on
  cached input reads against a ~25% write surcharge, amortized across a
  multi-turn conversation.

## 2. What this builds

### 2.1 Request model: carry cache breakpoints

Extend `MessageRequest` so the system field and message content can serialize
as Anthropic **content-block arrays** with optional `cache_control`:

```rust
// system as a typed block array (not a bare String) so the last stable
// block can carry a breakpoint; serializes to the API's array form.
pub enum SystemBlock { Text { text: String, cache_control: Option<CacheControl> } }
pub struct CacheControl { pub r#type: CacheType } // CacheType::Ephemeral => {"type":"ephemeral"}
```

Backward-compatible serialization: when no breakpoint is set, the existing
plain-string `system` form is still emitted (the existing
`streaming_request_serializes_with_stream_true` test and the "system omitted
when None" invariant must keep passing).

### 2.2 Composition: stable prefix vs. volatile tail

Split what `compose_persona` does today:

- **Cacheable prefix** = base persona + child-lock clause + tool/destructive-op
  preamble (the parts that do NOT change turn-to-turn). Emit as system block(s)
  with a `cache_control: ephemeral` breakpoint on the **last** stable block.
- **Volatile tail** = the per-turn recall context. Move it OUT of `system` into
  the message stream (e.g. a leading framed user-role context block, or a
  system block placed *after* the breakpoint so it is not part of the cached
  prefix). It must never sit before the breakpoint.
- Conversation history breakpoint (optional, second breakpoint): place a
  breakpoint after the most recent stable history boundary so growing history
  is also partially cached. Anthropic allows up to 4 breakpoints; use ≤2.

### 2.3 Usage accounting

The SSE `message_start` already carries `usage`. Surface
`cache_read_input_tokens` and `cache_creation_input_tokens` from the usage
struct (add the fields if absent) and log them per turn so the cache-read ratio
is observable in the journal, not just in tests.

## 3. Acceptance criteria

1. `MessageRequest` can serialize a `system` content-block array with a
   `cache_control: {"type":"ephemeral"}` breakpoint on the final stable block,
   and a unit test asserts the exact JSON shape matches Anthropic's documented
   form.
2. When no breakpoint is configured, serialization is byte-identical to today's
   plain-string `system` form (the existing serialization tests still pass;
   `system` is omitted, not null, when empty).
3. The cacheable persona prefix is composed **without** any per-turn recall
   context; a test feeds two turns with *different* recall hits and asserts the
   serialized bytes **before the breakpoint are identical** across both turns.
4. Volatile recall context still reaches the model (companionship continuity is
   preserved) but is positioned after the cached prefix; a test asserts the
   recall text is present in the request and located after the breakpoint.
5. **AC3 carried forward:** the existing `cache_hit_ratio_above_60pct` test
   (recorded 50-turn fixture against a fake Anthropic client that echoes
   `cache_read_input_tokens` per the `cache_control` breakpoints sent) passes
   with `sum(cache_read)/sum(input_tokens) ≥ 0.6`.
6. Per-turn `cache_read_input_tokens` / `cache_creation_input_tokens` are parsed
   from `message_start.usage` and emitted in a structured log line.
7. `child_lock` semantics are unchanged: the child-lock clause remains inside
   the cacheable prefix and an existing child-lock test still passes.
8. `cargo test` green; `cargo clippy` introduces no new warnings beyond the
   documented baseline (recall: clean HEAD already carries ~172 clippy warnings
   and a fastembed-transitive `cargo deny` finding — the bar is *no new*
   warnings + tests green, MSRV 1.85, no let-chains).
