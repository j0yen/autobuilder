# PRD: wm-skills — the answers that never need a model

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/thrift.md
**build_target:** rust-lib
**build_into:** /home/jsy/wintermute/wm-skills
**Depends on:** wm-router (consumes its `SkillId` taxonomy)
**Codename:** *almanac* — the clock, the calendar, the kitchen timer.

## TL;DR

A large share of an elderly companion's turns are utilitarian and structured:
the time, the day, a kitchen timer, a medication reminder, reaching family, the
weather. None need an LLM. `wm-skills` is a library of deterministic Rust
handlers behind a `Skill` trait plus a registry keyed by the `SkillId`s
`wm-router` dispatches. Each skill takes a parsed intent and returns a spoken
response (and, where relevant, a side effect — an armed timer, a persisted
reminder, a `wm.family.message` envelope) with **zero API cost**.

## 1. Why this exists

- **These intents are the bulk of the deflectable volume.** The thrift vision's
  end-state step 2 enumerates them: time, date, day-of-week, timers, reminders,
  meds, "call Joe", weather. They are high-frequency and fully deterministic.
- **Time-sensitive answers must NOT be cached.** The semantic cache
  (`PRD-wm-semcache`) explicitly excludes time-sensitive intents; they must be
  served fresh by a skill instead. wm-skills is where "what time is it" gets a
  correct answer every time — the cache would serve a stale one.
- **The family contract already exists — don't fork it.**
  `PRD-wintermute-family-intents` defines `wm.family.message` / `wm.family.ack`
  / `wm.family.reply` envelopes (`wintermute-dialog/src/family.rs` per that PRD).
  The family skill here is a thin wrapper that emits that existing contract — it
  does not invent a parallel one (vision OQ2).

## 2. What this builds

A library crate at `~/wintermute/wm-skills/`.

### 2.1 The Skill trait + registry

```rust
pub trait Skill {
    fn id(&self) -> SkillId;
    fn handle(&self, intent: &Intent, ctx: &SkillCtx) -> SkillResult;
}
pub struct SkillResult { pub speak: String, pub effect: Option<SideEffect>, pub cache_safe: bool }
```

`cache_safe` is the flag wm-semcache reads — time/weather skills return `false`.

### 2.2 The launch skill set

- **time / date / day-of-week** — from the system clock + the brain's configured
  timezone (`brain.toml` already has a `timezone` field, currently null —
  resolve it or default to system tz). `cache_safe = false`.
- **timer** — "set a timer for N minutes": arm a one-shot, announce on fire via
  the bus/TTS path. Side effect: a registered timer.
- **reminder / medication** — "remind me to take my pills at 8": persist a
  scheduled reminder (recall save_fact or a local store), fire at time.
  `cache_safe = false`. (vision OQ3: side-effecting skills write to recall so the
  brain retains continuity.)
- **family** — "tell/call Joe …": emit the existing `wm.family.message`
  envelope; speak the confirmation. Delegates to the family-intents contract.
- **weather** — current conditions from a free/local source (config: provider +
  location); `cache_safe = false` (weather changes).

### 2.3 Boundaries

No skill calls the Anthropic API. A skill that cannot confidently handle its
intent returns an error the caller maps to `Escalate` (never a guessed answer).

## 3. Acceptance criteria

1. `Skill` trait + a `Registry` that resolves a `SkillId` to its handler;
   registering two skills with the same id is a construction-time error.
2. time / date / day-of-week skills produce correct localized strings for a
   fixed injected clock + timezone (test with a frozen clock — no wall-clock
   flakiness), and report `cache_safe = false`.
3. timer skill arms a timer and signals fire through an injected sink; a test
   with a simulated clock asserts the fire fed the TTS/bus path exactly once.
4. reminder/medication skill persists a reminder and reports `cache_safe = false`;
   a test asserts the persisted record round-trips and that the side effect is
   recorded for brain continuity (per vision OQ3).
5. family skill emits an envelope **matching the existing `wm.family.message`
   contract** from family-intents (same field names/topic) — a test asserts the
   serialized envelope is contract-compatible; it does not define a new topic.
6. weather skill is fully injectable (the HTTP source is a trait the test stubs);
   no live network call in the test suite; reports `cache_safe = false`.
7. Every skill is total: an unhandleable intent returns a typed error (mapped to
   `Escalate` upstream), never a fabricated answer — proven by a test.
8. `cargo test` green; `cargo clippy -D warnings` clean (new crate, high bar);
   MSRV 1.85, no let-chains.
