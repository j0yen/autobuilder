# PRD: wintermute-brain — local/cloud/offline reply routing

**Author:** Claude Opus 4.8, for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/companion.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-wintermute-dialog-turn-fsm (the FSM that emits `wm.dialog.turn.user` and consumes `wm.brain.reply`)
**Codename:** *two-minds* — a fast local mind that is always present, and a deep cloud mind for when the words matter.

## TL;DR

`wmd` today is a **cloud-only** brain: every turn calls the Anthropic API, so with `WM_ANTHROPIC_API_KEY` empty (current state) or the network down, the companion is mute. This PRD adds a second backend — a local Ollama model on this laptop (`qwen2.5:3b`) — and a **router** that picks per turn: local 3B for instant commands and as the always-available fallback, cloud Claude (Sonnet) for real conversation when online and keyed. The brain degrades from "deep" to "fast" to "canned phrase" instead of failing silent.

## 1. Why this exists

- **The companion cannot depend on the cloud being reachable.** This is a voice-first deployment for jsy's mother. A brain that goes mute when wifi blips, the API key lapses, or Anthropic rate-limits is not a companion — it's a liability. There must always be a mind on the device.
- **The local mind is good enough for control and presence, and far faster.** Measured on this laptop (i7-10610U, CPU-only, no swap): `qwen2.5:3b` runs at **10.9 tok/s** generation, **~26 tok/s** prompt eval — fast enough to outrun TTS speech rate (~3.5 tok/s) with 2.7× headroom, so it streams into `wm-tts` without stutter. `qwen3:8b` is smarter but only **4.0 tok/s** — at/below speech rate once STT+TTS contend for the same 4 cores, so it is NOT the live conversational brain on this hardware (see [[reference-local-llm-setup]]). The local tier is the 3B.
- **The cloud mind is worth its latency only for conversation.** Sonnet gives the warmth and coherence a companion needs; its latency is dominated by the network round-trip, not this weak CPU, so it does not contend for local resources. Reserve it for conversational turns, not "turn on the light."
- **Routing is a brain concern, not a dialog concern.** `wm-dialog` is the turn-taking FSM; it emits `wm.dialog.turn.user` and waits for `wm.brain.reply`. *Which* mind produced that reply is invisible to the FSM and must stay that way. This PRD changes only the brain.

## 2. What this builds

### 2.1 A backend abstraction

```rust
#[async_trait]
trait ReplyBackend {
    /// Produce a reply to the user's turn given the assembled context.
    async fn reply(&self, ctx: &TurnContext) -> Result<Reply, BackendError>;
    fn tier(&self) -> Tier; // Cloud | Local | Canned
}
```

- **CloudBackend** — the existing `anthropic.rs` path, wrapped behind the trait. No behavior change to the prompt-cache / recall-context assembly.
- **LocalBackend** — new `src/local.rs`. Talks to Ollama's HTTP API at `http://localhost:11434/api/chat` using the **already-present `reqwest` dependency** (no new crate, no cross-repo dep bump). Sends the same system/user messages; reads the streamed response. Honors `OLLAMA_KEEP_ALIVE=-1` so the model stays resident and never pays cold-load latency mid-conversation.
- **CannedBackend** — the degrade phrase bank (shared concept with the companion-degrade PRD): a small fixed set of spoken-safe phrases ("I'm having trouble thinking right now — can you say that again in a moment?"). Always succeeds, never calls anything.

### 2.2 The router

`src/router.rs` — given a `TurnContext`, choose a backend by policy:

| # | Condition (checked in order) | Route |
|---|------------------------------|-------|
| 1 | Per-turn override set (`wmd swap-model`) | that tier |
| 2 | Utterance classified **command/control** (see 2.3) | **Local 3B** (low latency) |
| 3 | Online **and** API key present **and** classified **conversational** | **Cloud** (Sonnet) |
| 4 | Offline **or** no API key | **Local 3B** |
| 5 | Cloud attempt errors or exceeds `cloud_timeout_ms` | **Local 3B** fallback (logged) |
| 6 | Local attempt also fails (Ollama down) | **Canned** phrase |

"Online" = a cached reachability probe to the Anthropic API host (TCP connect with short timeout, re-checked at most every `reachability_ttl_s`, never on the hot path per turn). Key presence = the configured env var is non-empty.

### 2.3 Command vs conversation classification

Cheap, deterministic, no model call on the hot path:
- **Command/control** if the utterance matches the intent-card patterns (imperative verbs + known entities: "turn on/off", "set a timer", "what time/date", "stop", "louder/quieter", "mute yourself") OR is shorter than `command_max_words` (default 6) and lacks a question particle that implies open conversation.
- **Conversational** otherwise (questions, statements, anything long or open-ended).
- The classifier is a pure function over the text; it returns `Tier` preference + confidence. Ambiguous → treat as conversational (prefer the better mind when online).

> Rationale: commands want sub-second local latency and benefit nothing from Sonnet; conversation wants Sonnet's quality and tolerates the round-trip. This is the capability/latency split established in the research for this machine.

### 2.4 Configuration (`brain.toml`)

New `[routing]` section, atomic-written like existing model mutations:

```toml
[routing]
local_model        = "qwen2.5:3b"
cloud_model        = "sonnet"          # existing default-model still applies to cloud tier
ollama_endpoint    = "http://localhost:11434"
ollama_keep_alive  = "-1"
cloud_timeout_ms   = 6000
reachability_ttl_s = 30
command_max_words  = 6
prefer             = "auto"            # auto | local-only | cloud-only  (force a tier for the whole deployment)
```

New CLI surface (mirrors existing `wmd default-model` / `swap-model`):
- `wmd route status` — dump effective routing config + last reachability result + last tier used, as JSON.
- `wmd route prefer <auto|local-only|cloud-only>` — persist the deployment-wide preference (e.g. set `local-only` while the API key is unset).

### 2.5 Observability

- On every turn, publish `wm.brain.route` envelope: `{ turn_id, tier, reason, latency_ms, model }`. Lets an observer see which mind answered and why (e.g. `reason="cloud_timeout_fallback"`).
- Log each routing decision at INFO: `brain: route turn=… tier=Local reason=command`.
- Add `wm.brain.route` to wmd's self-emitted allow-list (same loop-suppress pattern as siblings).

### 2.6 What stays unchanged

- `wm.brain.reply` / `wm.brain.reply.destructive` / `wm.brain.error` / `wm.brain.tool.call` envelope contracts are unchanged — wm-dialog sees no difference.
- Recall context assembly, prompt caching, destructive-intent gating: unchanged. **The destructive-confirmation path is cloud-and-local identical** — a local-tier reply carrying a destructive intent must still route through `wm.brain.reply.destructive` for verbal confirmation. Local does not get a shortcut to act.
- Streaming partial replies to TTS is a **non-goal** (§4); v0.1 emits one `wm.brain.reply` per turn from whichever tier.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+12.** Cover: each router policy row (6), classifier command vs conversational (≥3 cases incl. ambiguous), local backend request/response parse (mocked HTTP), cloud-timeout-falls-back-to-local, local-fails-falls-back-to-canned, config round-trip (`[routing]` parse + atomic write).
2. **AC2 — daemon active 60s, NRestarts=0** with `prefer="local-only"` and **no** `WM_ANTHROPIC_API_KEY` set (proves the brain runs cloud-free).
3. **AC3 — offline/keyless turn served locally (mocked bus).** With no API key, harness emits `wm.dialog.turn.user` text="what time is it". Within the local latency budget, wmd publishes `wm.brain.reply` with a plausible answer and a `wm.brain.route` envelope `tier=Local reason=no_key`.
4. **AC4 — command routes local even when online.** With key present and reachability mocked "online", a command utterance ("turn off the kitchen light") yields `wm.brain.route tier=Local reason=command`.
5. **AC5 — conversation routes cloud when online.** Same online state, a conversational utterance ("how are you feeling today?") yields `tier=Cloud reason=conversational` (cloud call mocked).
6. **AC6 — cloud timeout falls back to local.** Online + conversational, but the mocked cloud backend exceeds `cloud_timeout_ms`. wmd publishes a reply from local and `wm.brain.route reason=cloud_timeout_fallback`; total turn latency ≤ `cloud_timeout_ms` + local budget.
7. **AC7 — total-failure degrade.** Cloud unreachable AND Ollama endpoint refused. wmd publishes a canned phrase via `wm.brain.reply` (`tier=Canned`) and does NOT emit `wm.brain.error` as a silent failure.
8. **AC8 — destructive intent still gated on local tier.** A local-tier reply expressing a destructive intent is published as `wm.brain.reply.destructive`, not `wm.brain.reply`.
9. **AC9 — live human gate.** Full fleet running, `WM_ANTHROPIC_API_KEY` set, `prefer="auto"`, Ollama up with `qwen2.5:3b` resident. Speak "hey wintermute, what time is it?" → answered by **local** (journalctl shows `tier=Local`), audible through the speaker in under ~3s. Then speak "hey wintermute, tell me about the weather you like" → answered by **cloud** (`tier=Cloud`). Then disable wifi and repeat the second utterance → answered by **local** (`tier=Local reason=offline`), still audible.
10. **AC10 — `cargo deny check bans licenses sources` clean** (subset per [[self_cargo_deny_cvss4_breakage]]; recall's full-deny baseline debt does not apply here, but keep the subset green).

## 4. Non-goals

1. **Streaming partial replies to TTS.** v0.1 returns one reply per turn. Sentence-by-sentence streaming from the local model into `wm.tts.speak` is the obvious next PRD (it's where the 3B's speed headroom pays off) but is out of scope here.
2. **8B as a live tier.** Measured too slow for conversation under CPU contention on this hardware. Not wired. (Revisit if the box gets a GPU or more cores.)
3. **A model-based intent classifier.** v0.1 uses deterministic pattern + length rules. A learned classifier is a separate concern.
4. **Local recall / memory for the local tier.** The local model gets the same assembled context the cloud path builds; no separate local memory store.
5. **Migrating the Ollama client into the `wm-local-llm` crate.** Kept in-brain via `reqwest` for a single-repo build. Refactoring the client out to `wm-local-llm` is a later cleanup (see open questions).
6. **Multi-model local routing** (3B vs 8B by difficulty). One local model for v0.1.

## 5. Open questions

- **`cloud_timeout_ms` default (6s).** Long enough to let Sonnet answer, short enough that the fallback doesn't feel broken. Tune at deployment.
- **Reachability probe target.** Probe `api.anthropic.com:443` directly, or a lighter heartbeat? Direct TCP connect is simplest; revisit if it's noisy.
- **Should `wm-local-llm` become the real home of the Ollama client?** It's currently a stub (`lib.rs` only). If a second consumer appears (e.g. a local-only summarizer daemon), promote the client there and have the brain depend on it. Not now.
- **Per-turn override semantics.** Does `wmd swap-model opus` (cloud) imply "force cloud this turn even for a command"? Proposed: yes — explicit override (policy row 1) beats classification.
- **Classifier locale.** The command patterns are English; jsy's mother's language may differ. Flag for the deployment-language PRD.

## 6. Files this PRD likely touches

- New: `src/router.rs` (policy table + reachability cache + classifier)
- New: `src/local.rs` (Ollama `/api/chat` client over existing `reqwest`)
- New: `src/canned.rs` (degrade phrase bank; align with companion-degrade PRD)
- Modified: `src/anthropic.rs` (wrap behind `ReplyBackend` trait; no prompt-assembly change)
- Modified: `src/daemon.rs` (call the router instead of the cloud path directly; publish `wm.brain.route`)
- Modified: `src/persist.rs` (`[routing]` config section, atomic write; `wmd route` subcommands)
- Modified: `src/bus.rs` (self-emitted allow-list += `wm.brain.route`)
- Modified: `src/main.rs` (`wmd route status` / `wmd route prefer` CLI)
- Modified: `tests/` (router + classifier + fallback integration tests)
- Modified: `README.md`, `CHANGELOG.md`
