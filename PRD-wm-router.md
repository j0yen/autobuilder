# PRD: wm-router — the gate that decides what the API never sees

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-router
**Depends on:** recall (consumes its `embed` socket RPC)
**Codename:** *gatekeeper* — when unsure, it opens the gate to the brain.

## TL;DR

Every transcribed utterance in the companion loop is escalated to Sonnet today,
including utilitarian and repetitive ones that need no frontier model. `wm-router`
is a library that classifies one utterance and returns a `Route` — `Skill(id)`,
`CacheLookup`, `LocalLlm`, or `Escalate` — with a confidence. It uses cheap
deterministic rules first, then embedding similarity (via recall's existing
`embed` RPC) against labeled intent prototypes. It is **conservative by
construction**: below a configurable confidence floor it returns `Escalate`, so
the worst case is "we paid for a turn we might have deflected" — never "we gave
her a wrong/cold answer to save money." It is the integration spine the other
thrift tiers plug into.

## 1. Why this exists

- **No pre-brain gate exists.** `visions/companion.md` end-state step 3:
  "wm-dialog routes the transcript through wmd (brain) to Claude" — unconditional
  escalation. There is no tier between transcript and API.
- **The deterministic-intent pattern already exists, as a one-off.**
  `PRD-wintermute-family-intents` adds a single hardcoded Family branch to the
  dialog FSM, explicitly "to keep intent recognition deterministic rather than
  gated on the Claude API." wm-router generalizes that one branch into a general
  classifier; family becomes one registered skill, not a bespoke `if`.
- **The embedder is already a shared service.** recall's daemon exposes an
  `embed` op (`recall/src/daemon.rs:27` — `OPS = ["query","embed","touch","ping"]`)
  backed by BGE-small-en-v1.5 (384-dim) with a HashEmbedder offline fallback
  (256-dim). wm-router calls it over the socket — no second model load, same
  vectors the rest of the system uses (cited recall AC: cold ≤1.5s, warm ≤200ms,
  offline HashEmbedder fallback).

## 2. What this builds

A `no-daemon` library crate at `~/wintermute/wm-router/`.

### 2.1 The Route taxonomy

```rust
pub enum Route {
    Skill(SkillId),     // deterministic handler (wm-skills)   — zero API
    CacheLookup,        // try the semantic cache (wm-semcache) — zero API
    LocalLlm,           // low-stakes generative (wm-local-llm) — zero API
    Escalate,           // send to wmd/Sonnet                   — full API
}
pub struct Decision { pub route: Route, pub confidence: f32, pub why: String }
```

### 2.2 The two-stage classifier

1. **Rules stage (cheap, first):** normalized-text patterns for the high-volume,
   unambiguous intents ("what time is it", "set a timer", "what day is it"). A hit
   returns `Skill(id)` at confidence 1.0. No embedding call needed.
2. **Embedding stage (fallback):** embed the utterance via recall's `embed` RPC,
   cosine-compare against a labeled prototype set per intent, take the best match.
   If best similarity ≥ skill floor → `Skill(id)`. Else consult cache-candidacy
   and local-llm-candidacy heuristics. Else `Escalate`.
3. **Confidence floor (the safety knob):** a single configurable threshold below
   which the decision is forced to `Escalate`. Default tuned conservative
   (vision OQ4). `LocalLlm` is **gated off by default** (vision OQ5): the lib
   supports the route but the shipped default config never returns it until jsy
   enables an explicit intent allowlist.

### 2.3 Embedder client + offline degradation

A thin client for recall's `embed` op. The router must be dim-agnostic (read the
returned vector length; don't hardcode 384 vs 256). If recall is unreachable, the
rules stage still works; the embedding stage degrades to `Escalate` (fail safe,
never fail to a wrong skill).

## 3. Acceptance criteria

1. `Router::classify(&str) -> Decision` returns a `Route` + confidence + `why`
   string; pure rules-stage classification requires no network/socket.
2. A committed labeled fixture set (≥40 utterances spanning skill intents,
   cache-repeat phrasings, open-ended companionship, and ambiguous cases) drives
   a test that measures, and asserts thresholds on: **(a)** deflection rate
   (% routed to a non-`Escalate` tier) and **(b)** false-deflection rate
   (% of *open-ended* fixtures wrongly routed away from `Escalate`).
3. **False-deflection of open-ended turns < 1%** on the fixture set at the
   default confidence floor (the vision's quality guarantee — wrong-routing a
   companionship turn is the cardinal sin).
4. With recall's `embed` socket unreachable, `classify` still returns (rules
   stage works; embedding-dependent cases return `Escalate`) and never panics —
   proven by a test pointing the embedder client at a dead socket path.
5. The embedder client reads vector length from the response and works against
   both the 384-dim (fastembed) and 256-dim (hash) embedders — a test feeds both
   dims through cosine comparison without a hardcoded dimension.
6. `LocalLlm` is never returned under the shipped default config (gated off);
   a test asserts this, and a second test shows it *is* returned once an intent
   allowlist enables it.
7. `cargo test` green; `cargo clippy -D warnings` clean for this new crate (new
   crates start from zero — hold the high bar here, unlike the recall baseline);
   MSRV 1.85, no let-chains.
