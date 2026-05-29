# PRD: wm-router — the gate that decides what the API never sees

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.2
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-router
**Depends on:** recall (consumes its `embed` socket RPC)
**Codename:** *gatekeeper* — when unsure, it opens the gate to the brain.

> **v0.2 (2026-05-29):** jsy's locked switching strategy made the brain itself
> local-first with its own tier ladder (PRD-brain-backend-ladder). So the router
> no longer needs a `LocalLlm` route — *everything that reaches the brain already
> starts local*. The route taxonomy simplifies to `Skill / CacheLookup / Brain`,
> and the router gains a **safety stakes tag** on `Brain` so the ladder can apply
> its cost-blind safety override. See Changelog.

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
    Skill(SkillId),         // deterministic handler (wm-skills)   — zero API
    CacheLookup,            // try the semantic cache (wm-semcache) — zero API
    Brain { stakes: Stakes },// hand to wmd's local-first tier ladder
}
pub enum Stakes {
    Ordinary,               // ladder starts local-first (default 3b)
    HighStakes(StakesClass),// ladder safety-override: start at a trusted cloud tier
}
pub enum StakesClass { Medication, Medical, Emergency, Distress, Money }
pub struct Decision { pub route: Route, pub confidence: f32, pub why: String }
```

`Brain` replaces v0.1's `LocalLlm`/`Escalate` split — the brain ladder owns the
local-vs-cloud decision now. The router's remaining jobs: deflect to skill/cache
when confident, and **tag stakes** so the ladder knows when to skip local.

### 2.2 The classifier stages

1. **Safety stage (FIRST, cost-blind):** match the utterance against the
   high-stakes classes (`Medication`/`Medical`/`Emergency`/`Distress`/`Money`)
   via rules + embedding prototypes tuned for HIGH RECALL (a missed
   medication/emergency turn is the worst failure). A hit → `Brain { HighStakes(..) }`
   immediately, bypassing skill/cache deflection.
2. **Rules stage:** normalized-text patterns for high-volume, unambiguous skill
   intents ("what time is it", "set a timer"). A hit → `Skill(id)` at conf 1.0.
3. **Embedding stage:** embed via recall's `embed` RPC, cosine-compare against
   labeled prototypes. ≥ skill floor → `Skill(id)`; cache-candidate → `CacheLookup`;
   else → `Brain { Ordinary }`.
4. **Confidence floor:** below the configurable threshold, default to
   `Brain { Ordinary }` (never a guessed skill/cache hit). The brain's local-first
   ladder + `wm-verify` gate then handles quality — so the router can be cheap and
   the *brain* owns the cost/quality climb.

### 2.3 Embedder client + offline degradation

A thin client for recall's `embed` op. The router must be dim-agnostic (read the
returned vector length; don't hardcode 384 vs 256). If recall is unreachable, the
rules + safety stages still work; the embedding stage degrades to
`Brain { Ordinary }` (fail safe — hand to the brain, never to a wrong skill).

## 3. Acceptance criteria

1. `Router::classify(&str) -> Decision` returns a `Route` + confidence + `why`
   string; pure rules-stage classification requires no network/socket.
2. A committed labeled fixture set (≥40 utterances spanning skill intents,
   cache-repeat phrasings, high-stakes, open-ended companionship, and ambiguous
   cases) drives a test that measures and asserts thresholds on: **(a)**
   deflection rate (% routed to `Skill`/`CacheLookup`) and **(b)** false-deflection
   rate (% of *open-ended* fixtures wrongly deflected away from `Brain`).
3. **False-deflection of open-ended turns < 1%** on the fixture set at the
   default confidence floor (the vision's quality guarantee — wrong-routing a
   companionship turn away from the brain is the cardinal sin).
4. **High-stakes recall:** every high-stakes fixture (medication / medical /
   emergency / distress / money) is classified `Brain { HighStakes(..) }` with
   the correct class, and NONE are deflected to skill/cache — the safety stage
   runs first and is tuned for recall (a missed high-stakes turn is the worst
   failure). Assert 100% high-stakes recall on the fixture set.
5. With recall's `embed` socket unreachable, `classify` still returns (rules +
   safety stages work; embedding-dependent cases return `Brain { Ordinary }`)
   and never panics — proven by a test pointing the embedder at a dead socket.
6. The embedder client reads vector length from the response and works against
   both the 384-dim (fastembed) and 256-dim (hash) embedders — a test feeds both
   dims through cosine comparison without a hardcoded dimension.
7. `cargo test` green; `cargo clippy -D warnings` clean for this new crate (new
   crates start from zero — hold the high bar here, unlike the recall baseline);
   MSRV 1.85, no let-chains.

## 4. Changelog

- **v0.2 (2026-05-29)** — switching strategy locked. Route taxonomy simplified
  from `Skill/CacheLookup/LocalLlm/Escalate` → `Skill/CacheLookup/Brain{stakes}`
  (the brain ladder now owns local-vs-cloud, so no `LocalLlm` route). Added a
  safety stage + `Stakes`/`StakesClass` tagging and the high-stakes-recall AC4;
  dropped v0.1's "LocalLlm gated off" AC (route no longer exists).
- **v0.1 (2026-05-29)** — initial: `Skill/CacheLookup/LocalLlm/Escalate`,
  two-stage rules+embedding classifier, conservative escalate-when-unsure.
