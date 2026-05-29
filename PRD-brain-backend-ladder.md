# PRD: brain-backend-ladder — local by default, climb only when needed

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.2
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** wm-local-llm (local backend client, ✅ built); wm-verify (the soft-failure gate that drives escalation); wm-router (supplies the starting tier + safety stakes tag); composes-with PRD-brain-prompt-cache (the cloud tiers keep their cache breakpoints)
**Codename:** *ascent* — climb a rung only when the turn demands it.

> **v0.2 (2026-05-29):** revised after jsy's locked switching strategy (vision
> "Switching strategy"). Ladder gains a **Haiku** rung; escalation is now driven
> by the **wm-verify** soft-failure gate (not just `wm-local-llm`'s hard
> failures); adds **filler-while-escalating**, a **cost/quota governor**,
> **conversational stickiness**, and honoring **wm-router's safety pre-route**
> (high-stakes turns start cloud, skipping local). See Changelog.

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
//     → haiku (Anthropic, claude-haiku-4-5)   ← cheap-cloud floor
//     → sonnet (Anthropic) → opus (Anthropic)
```

`brain.toml` gains: `default_tier` (default `"local-3b"`), a `[backends.local]`
section (`endpoint`, default `http://127.0.0.1:11434/v1`), and an optional
`[[ladder]]` override; back-compat: an existing `default_model = "sonnet"` maps
to the `sonnet` tier on load.

### 2.2 LadderClient (implements the dispatch)

A `LadderClient` that owns the local client (`wm-local-llm`) + the optional
`AnthropicClient` + the `wm-verify` gate + the resolved ladder. For a turn it:

1. **Starting tier:**
   - If `wm-router` tagged the turn **high-stakes** (medication, medical, falls,
     acute distress, money) → start at the configured trusted tier (Sonnet/Opus),
     **skipping local entirely** (safety override; cost-blind).
   - Else → start **local-first** at `pending_override > default_tier`
     (default `local-3b`) per jsy's locked posture.
2. **Dispatch:** `Local` tier → `wm-local-llm::generate(prompt, sink)`;
   `Anthropic` tier → the existing `collect_messages` path (carrying the
   prompt-cache breakpoints from PRD-brain-prompt-cache untouched).
3. **Escalation is driven by TWO signals (climb one rung on either):**
   - *Hard failure* — `LocalOutcome::Escalate{reason}` from `wm-local-llm`
     (timeout/unreachable/empty/truncated).
   - *Soft failure* — a `Local` tier `Answer` that `wm-verify` rejects
     (refusal/looping/wrong-language/disclaimer/etc.) before it is spoken.
   Bounded — never climbs past the top tier; each climb is logged with its
   reason (hard vs. soft + the specific reason).
4. **Filler while escalating (voice latency governor):** if a climb would exceed
   the first-audio budget, emit a short backchannel via the TTS path (reuse the
   `companion-degrade` phrase bank) while the higher tier generates, so a climb
   never lands as a dead pause.
5. **Cost/quota governor:** track cloud spend; as a configured cap nears, raise
   the stakes threshold required to escalate into cloud tiers and reserve Opus
   for top-stakes only.
6. **Conversational stickiness:** maintain a per-session "tier floor" — within an
   ongoing warm/emotional thread don't drop below the tier that's been handling
   it; the floor decays on topic change.
7. **Terminal failure** (top tier unreachable / no key for an Anthropic tier)
   routes to the existing degrade path, never a panic.

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

1. A built-in tier ladder `local-3b → local-8b → haiku → sonnet → opus` is
   defined; `default_tier` loads from brain.toml and defaults to `local-3b`; a
   legacy `default_model = "sonnet"` config still resolves (to the sonnet tier).
2. With default tier `local-3b` and an injected fake local backend that returns
   an answer `wm-verify` ACCEPTS, the turn is served by the local backend and the
   Anthropic client is **never called** — asserted via injected fakes (reuse the
   existing `LlmClient` fake-injection test pattern).
3. **Dual-signal escalation:** a local tier climbs one rung on EITHER (a) a
   `LocalOutcome::Escalate` (hard failure) OR (b) a local `Answer` that
   `wm-verify` REJECTS (soft failure). A test with a fake local backend whose
   output `wm-verify` rejects + a fake higher tier (accepted answer) asserts the
   reply came from the higher tier; a second test does the same via the hard
   `Escalate` path. Both assert the climb is bounded (stops at the top tier) and
   the reason (hard vs. soft + specific reason) is logged.
3b. **Safety override:** a turn tagged high-stakes by `wm-router` starts at the
   configured trusted cloud tier and the local backend is **never called** —
   asserted with an injected high-stakes tag + fakes (cost-blind; this overrides
   the local-first default).
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
9. **Filler while escalating:** when a climb is triggered and the first-audio
   budget would be exceeded, a backchannel phrase is emitted to the TTS/reply
   path before the higher tier's answer — asserted with a fake sink + an injected
   slow higher tier (the sink receives the filler, then the real answer).
10. **Conversational stickiness:** a per-session tier floor holds across turns in
   a thread (a turn after an escalation to Sonnet does not silently drop back to
   local-3b within the same thread) and decays on topic change — asserted with a
   two-turn fixture.
11. `cargo test` green; `cargo clippy` introduces no new warnings beyond the
   wintermute-brain baseline; MSRV 1.85; no let-chains; child-lock and existing
   `pending_model` AC tests still pass.

## 5. Changelog

- **v0.2 (2026-05-29)** — jsy locked the switching strategy (vision "Switching
  strategy"): local-first posture, filler-while-escalating, and a **Haiku**
  cheap-cloud rung (`3b→8b→haiku→sonnet→opus`). Escalation is now **dual-signal**
  (hard `wm-local-llm` failures + soft `wm-verify` rejects), gaining deps on
  `wm-verify` and `wm-router`. Added safety-override pre-route (AC3b), filler
  (AC9), cost/quota governor + stickiness (AC10). v0.1's simple
  Escalate-only ladder is a strict subset of this.
- **v0.1 (2026-05-29)** — initial: linear `3b→8b→sonnet→opus`, auto-escalate on
  `wm-local-llm` `Escalate` only, manual switches, no-API-key gate relaxation.

## 4. Non-goals

- The router that decides skill/cache/escalate before the brain (that is
  `wm-router`); the ladder is purely about *which model answers* once a turn
  reaches the brain.
- Running/supervising ollama (external; `wm-local-llm` only speaks to it).
- Vendoring weights.
- Confidence scoring of local output beyond what `wm-local-llm` already returns
  (its `Escalate` is the signal; smarter local-quality scoring is a future PRD).
