# PRD: wm-local-llm — a stand-in for the low-stakes turns

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-local-llm
**Depends on:** wm-router (serves its `LocalLlm` route)
**Codename:** *understudy* — covers the small parts so the star rests.

## TL;DR

Between the deterministic skills (zero generation) and Sonnet (frontier
reasoning) sits a tier of turns that need *some* generation but not frontier
quality: greetings, acknowledgments, light rephrasings, low-stakes chit-chat.
`wm-local-llm` is a client for a local model served behind a local
**OpenAI-compatible HTTP endpoint** — the protocol ollama, llama-server, and
llamafile all speak. It wraps the *protocol*, not a specific binary, because jsy
is actively choosing a runtime in a parallel window; the endpoint URL and model
id are config. It serves the router's `LocalLlm` route with **zero API cost**,
streams tokens to the TTS path, and — critically — **falls back to `Escalate`
(Sonnet) on timeout, empty output, or low confidence**, degrading to the better
model rather than to silence.

## 1. Why this exists

- **The middle tier is real but was deferred.** The thrift vision originally
  scoped out a local LLM; jsy reopened it on 2026-05-29 ("draft #5, I'm testing
  a local llm in another window"). With a runtime now in hand, the `LocalLlm`
  route the router already defines has a backend to point at.
- **Protocol over binary keeps it runtime-agnostic.** ollama, llama-server, and
  llamafile all expose `POST /v1/chat/completions`. Targeting that surface means
  jsy can swap the model/runtime behind it without touching this crate — the
  same decoupling `wintermute-stt` got by hiding whisper behind a feature flag.
- **Degradation, not silence, is the rule.** `visions/companion.md` step 7 and
  `PRD-wintermute-companion-degrade` establish that wintermute says what's wrong
  rather than going still. A local model that stalls or returns garbage must
  escalate to Sonnet, never drop the turn.
- **No weights vendored.** Per the wintermute-home + autobuilder conventions, a
  PRD doesn't bake in multi-GB model files; the model lives wherever jsy's
  runtime keeps it. This crate only holds the client.

## 2. What this builds

A library crate at `~/wintermute/wm-local-llm/`.

### 2.1 The client

```rust
pub struct LocalLlm { endpoint: Url, model: String, timeout: Duration }
pub enum LocalOutcome {
    Answer { text: String },     // stream completed within budget
    Escalate { reason: String }, // timeout / empty / low-confidence / unreachable
}
impl LocalLlm { pub async fn generate(&self, prompt: &Prompt) -> LocalOutcome; }
```

- Speaks `POST /v1/chat/completions` (OpenAI-compatible), streaming.
- Streams partial tokens to an injected sink (the TTS path) so speech can begin
  before generation finishes.
- Enforces a latency budget; on breach, cancels and returns `Escalate`.

### 2.2 Fallback semantics

`generate` returns `Escalate { reason }` — never an error the caller must
interpret as silence — on any of: connection failure, HTTP error, empty/
whitespace-only completion, generation timeout, or a configurable max-token cap
hit without a clean stop. The caller (dialog FSM) maps `Escalate` to a normal
Sonnet turn.

### 2.3 Config

`endpoint` URL, `model` id, `timeout`, `max_tokens`, and an optional system
preamble are config (a struct deserialized from `brain.toml`/env). No defaults
that assume a specific runtime is installed; if unconfigured, the client reports
unconfigured and the router simply never routes `LocalLlm` (it is gated off by
default per vision OQ5 anyway).

## 3. Acceptance criteria

1. `generate` issues an OpenAI-compatible `POST /v1/chat/completions` streaming
   request; a test against a stub HTTP server asserts the request shape (model,
   messages, stream=true) and that streamed deltas reach the injected sink in
   order.
2. A successful streamed completion returns `LocalOutcome::Answer` with the
   concatenated text.
3. **Timeout → Escalate:** a stub endpoint that stalls past the budget yields
   `Escalate { reason }` (not a hang, not an error-as-silence) within the budget
   window + a small margin — proven with an injected short timeout.
4. **Unreachable → Escalate:** pointing the client at a dead endpoint yields
   `Escalate`, never a panic.
5. **Empty/garbage → Escalate:** a stub returning an empty or whitespace-only
   completion yields `Escalate`.
6. Unconfigured client (no endpoint) reports unconfigured and is safe to
   construct; a test asserts the router, given an unconfigured client, never
   emits `LocalLlm` (ties to wm-router AC6 — gated off by default).
7. Streaming sink receives tokens incrementally (a test asserts >1 sink write
   for a multi-chunk stubbed stream) so TTS can start before completion.
8. No model weights or runtime binaries are vendored or downloaded by the build;
   the test suite makes no live network call (HTTP is stubbed).
9. `cargo test` green; `cargo clippy -D warnings` clean (new crate, high bar);
   MSRV 1.85, no let-chains.
