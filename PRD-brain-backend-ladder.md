# PRD: brain-backend-ladder — local by default, climb only when needed

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** wm-local-llm (path dep — the local backend client); composes-with PRD-brain-prompt-cache (the Anthropic tiers keep their cache breakpoints)
**Codename:** *ascent* — climb a rung only when the turn demands it.

## TL;DR

Today every brain turn goes to one Anthropic model (`default_model = "sonnet"`).
jsy's decision (2026-05-29): the brain should **default to a local 3B model** and
**climb a ladder only when needed** — `local-3b → local-8b → Sonnet → Opus`. This
PRD turns the brain's single-backend turn path into a **tier ladder**: a default
tier (local-3b, free), manual switches to any tier (generalizing the existing
`swap-model`/`default-model`), and automatic escalation up one rung when a local
tier returns `Escalate` (the `wm-local-llm` failure/low-confidence outcome). A
huge side benefit: with the local tier as default, **the brain works with no
Anthropic API key** — the missing-key case stops disabling the brain and instead
just makes the paid tiers unavailable.

## 1. Why this exists

- **jsy chose local-first with an escalation ladder** (2026-05-29, this session):
  "default to local 3b. wire up switches to use 8b, Sonnet and Opus when needed."
  The thrift vision records it under "Brain backend ladder."
- **The brain already has the exact seam.** `wintermute-brain/src/daemon.rs:88`
  defines an `LlmClient` trait; `AnthropicClient` implements it
  (`daemon.rs:105`), and tests inject a fake. The turn path is
  `compose_persona` → `compose_request(model, …)` (`daemon.rs:118`, via
  `canonical_model`) → `llm.collect_messages(&req)` (`daemon.rs:1058`). A ladder
  dispatcher slots in at the `LlmClient` boundary — no rewrite of the loop.
- **Model selection machinery exists.** `default_model` (persistent, brain.toml),
  `pending_model` (next-turn), and the `swap-model` / `default-model` CLI
  commands (`src/main.rs:54-95`) already mutate the config and resolve short
  names via `canonical_model`. The ladder extends these to span tiers, not just
  Anthropic ids.
- **The key-gate is currently all-or-nothing.** `build_anthropic_client`
  (`daemon.rs:1248`) returns `None` when no API key — and `None` means "the
  daemon runs without a brain." With a local default tier, a missing key should
  only disable Sonnet/Opus, not the whole brain. (Recall: the live laptop ran
  for sessions with `WM_ANTHROPIC_API_KEY` empty and the brain mute — this PRD
  fixes that class of outage.)
- **The local client exists.** `wm-local-llm` (built this session) is the
  OpenAI-compatible client; local-3b and local-8b are the *same* client with a
  different `model` config (qwen2.5:3b vs qwen3:8b on ollama 127.0.0.1:11434).

## 2. What this builds (rust-extend into wintermute-brain)

### 2.1 Tier + ladder model

```rust
pub enum Backend { Local, Anthropic }
pub struct Tier { pub name: String, pub backend: Backend, pub model: String } // model: ollama id or canonical claude id
// Built-in default ladder, lowest→highest, overridable in brain.toml:
//   local-3b (Local, qwen2.5:3b) → local-8b (Local, qwen3:8b)
//                                → sonnet (Anthropic) → opus (Anthropic)
```

`brain.toml` gains: `default_tier` (default `"local-3b"`), a `[backends.local]`
section (`endpoint`, default `http://127.0.0.1:11434/v1`), and an optional
`[[ladder]]` override; back-compat: an existing `default_model = "sonnet"` maps
to the `sonnet` tier on load.

### 2.2 LadderClient (implements the dispatch)

A `LadderClient` that owns the local client (`wm-local-llm`) + the optional
`AnthropicClient` and the resolved ladder. For a turn it:
1. Starts at the active tier (pending override > default_tier).
2. Dispatches: `Local` tier → `wm-local-llm::generate(prompt, sink)`;
   `Anthropic` tier → the existing `collect_messages` path (carrying the
   prompt-cache breakpoints from PRD-brain-prompt-cache untouched).
3. **Auto-escalation:** a `Local` tier returning `LocalOutcome::Escalate{reason}`
   advances to the next tier up and retries. Bounded — never climbs past the top
   tier; the climb is logged with the reason.
4. Terminal failure (top tier unreachable / no key for an Anthropic tier) routes
   to the existing degrade path, never a panic.

### 2.3 Switches

- `swap-model <tier>` (next turn only) and `default-model <tier>` (persistent)
  accept tier names (`local-3b`, `local-8b`, `sonnet`, `opus`) **and** the legacy
  short model ids (`sonnet`/`opus` resolve to their tiers) — same CLI, same
  brain.toml mutation path, extended resolver. Unknown name → error (as today).

### 2.4 Key-gate relaxation

`build_anthropic_client` returning `None` (no API key) no longer disables the
brain when the default tier is `Local`. Anthropic tiers become *unavailable*
(an attempt to use/escalate to them without a key yields a clear degrade
outcome — "I can't reach the bigger brain right now" — not a crash).

## 3. Acceptance criteria

1. A built-in tier ladder `local-3b → local-8b → sonnet → opus` is defined;
   `default_tier` loads from brain.toml and defaults to `local-3b`; a legacy
   `default_model = "sonnet"` config still resolves (to the sonnet tier).
2. With default tier `local-3b` and an injected fake local backend that returns
   `Answer`, a turn is served by the local backend and the Anthropic client is
   **never called** — asserted via injected fakes (reuse the existing
   `LlmClient` fake-injection test pattern).
3. **Auto-escalation:** a local tier whose backend returns
   `LocalOutcome::Escalate` causes a retry on the next tier up; a test with a
   fake local backend (Escalate) + a fake higher tier (Answer) asserts the reply
   came from the higher tier, the climb is bounded (stops at the top tier), and
   the escalation reason is logged.
4. `swap-model local-8b` sets the next-turn tier (consumed after one turn, like
   `pending_model` today); `default-model opus` sets the persistent tier;
   `swap-model sonnet` (legacy short name) still works. Unknown tier → error.
5. With **no Anthropic API key**, local tiers still serve turns (a test asserts a
   turn completes via the local backend with `build_anthropic_client` → `None`);
   attempting to use/escalate to an Anthropic tier without a key yields a typed
   degrade outcome, not a panic and not a hang.
6. The local tier is served by the `wm-local-llm` crate (path dependency); the
   tier's model id (`qwen2.5:3b` vs `qwen3:8b`) and the endpoint are config, not
   hardcoded — a test constructs both local tiers from config.
7. Streaming: local `Answer` deltas are forwarded to the reply/TTS path
   incrementally (proven with a fake sink receiving >1 delta), so speech can
   begin before the local generation finishes.
8. (SHOULD) Composition: when a turn lands on an Anthropic tier, the
   `MessageRequest` still carries the `cache_control` breakpoints introduced by
   PRD-brain-prompt-cache — a test asserts the ladder path does not strip them.
   (Gated on brain-prompt-cache having landed; if not yet merged, assert the
   ladder passes the request through unmodified.)
9. `cargo test` green; `cargo clippy` introduces no new warnings beyond the
   wintermute-brain baseline; MSRV 1.85; no let-chains; child-lock and existing
   `pending_model` AC tests still pass.

## 4. Non-goals

- The router that decides skill/cache/escalate before the brain (that is
  `wm-router`); the ladder is purely about *which model answers* once a turn
  reaches the brain.
- Running/supervising ollama (external; `wm-local-llm` only speaks to it).
- Vendoring weights.
- Confidence scoring of local output beyond what `wm-local-llm` already returns
  (its `Escalate` is the signal; smarter local-quality scoring is a future PRD).
