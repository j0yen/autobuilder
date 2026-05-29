# Vision: thrift — spend the API only where it earns warmth

**Author:** /dream (Claude Opus 4.8), for jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** user-prompt (2026-05-29 — "think hard about what you can build in /autobuilder instead of using the anthropic API which is quite expensive")

## TL;DR

`wintermute-brain` (wmd) is the fleet's only Anthropic API consumer — STT is
already local (whisper.cpp), TTS is local + a synth cache. Today every brain
turn pays full price twice over: (1) the brain re-bills its entire stable
prefix (persona + tool defs + recall + history) at full input rate on every
turn because it sends **no `cache_control` breakpoints** despite its own AC3
targeting a ≥60% cache-read ratio; and (2) *every* transcribed utterance is
escalated to Sonnet, including "what time is it" and "what day is today" that
need no model at all. thrift builds a **local-first dialog tier** in front of
the brain — try cache → skill → local model → escalate — and fixes the brain so
the escalations that remain are cheap. The warm, open-ended companionship turns —
the whole point of a companion for jsy's mother — deliberately *stay* on
Sonnet. thrift doesn't make wintermute cheaper by making her dumber; it stops
paying frontier prices for a clock.

## End-state

When this vision is fulfilled:

1. **A transcribed utterance hits a router before the brain.** The router
   (rules + recall's embedder) classifies it and routes to one of three
   tiers, escalating to the brain whenever it is not confident.
2. **Time-sensitive & structured intents are served with zero API.** "What
   time is it", "what day is it", "set a timer for the pasta", "remind me to
   take my pills at 8", "call Joe", "what's the weather" — answered by
   deterministic Rust skills.
3. **Repeated questions are served from a semantic cache with zero API.** When
   she asks the same thing she asked an hour ago in slightly different words,
   the cached answer is spoken back — except for intents marked cache-unsafe
   (anything time-sensitive routes to a skill instead, never the cache).
4. **Low-stakes generative turns are served by a local model with zero API.**
   Acknowledgments, simple rephrasings, and low-stakes chit-chat the deployment
   is comfortable getting at small-model quality are answered by a local LLM
   (whatever runtime jsy lands on — wrapped behind a local OpenAI-compatible
   endpoint so the tier is runtime-agnostic). The router escalates to Sonnet
   only when the turn is both generative *and* high-stakes/open-ended.
5. **The turns that *do* reach Sonnet are cheap.** The brain sends
   `cache_control` breakpoints; the stable persona prefix is cached
   (~90% input discount on that span) and the volatile recall context is moved
   out of the cached prefix so it doesn't bust the cache every turn. Measured
   cache-read ratio ≥60% (its own AC3).
6. **Every tier proves its deflection.** Each component ships an acceptance
   criterion that measures its cost impact — cache-read ratio, % of turns
   deflected, false-escalation rate — so the saving is a number, not a hope.

## Components (PRD-sized pieces)

Drafted this pass (5):

1. **PRD-brain-prompt-cache** (rust-extend → `wintermute-brain`) — add
   `cache_control` ephemeral breakpoints; restructure so the stable persona is
   the cacheable prefix and volatile recall context moves to a later
   (non-prefix) block. Satisfies the brain's own AC3 (≥60% cache-read ratio).
   *Independent — ships first, no new crates, pure per-call saving.*

2. **PRD-wm-router** (rust-lib → `~/wintermute/wm-router/`) — the classification
   engine. Input: an utterance. Output: a `Route` (`Skill(id)` / `CacheLookup`
   / `LocalLlm` / `Escalate`) with a confidence. Uses cheap rules first, then embedding
   similarity via recall's `embed` RPC. **Conservative by construction**: below
   a confidence floor it returns `Escalate` — it never trades companionship
   quality for cost. Generalizes the one-off deterministic intent branch that
   `PRD-wintermute-family-intents` adds to the dialog FSM.

3. **PRD-wm-skills** (rust-lib → `~/wintermute/wm-skills/`) — the zero-LLM skill
   registry the router dispatches to: time, date, day-of-week, timers,
   reminders, medication schedule, family-reach ("call Joe"), weather (local /
   free source). A `Skill` trait + a registry keyed by intent id.

4. **PRD-wm-semcache** (rust-lib → `~/wintermute/wm-semcache/`) — embedding-keyed
   response cache reusing recall's `embed` RPC and vector substrate. Near-
   duplicate utterances return a cached answer with zero API. TTL + an
   explicit cache-unsafe intent class (time-sensitive answers are never
   cached; they route to a skill).

5. **PRD-wm-local-llm** (rust-lib → `~/wintermute/wm-local-llm/`) — a client for a
   local model behind a local **OpenAI-compatible HTTP endpoint** (ollama /
   llama-server / llamafile all speak it — jsy is testing a runtime now, so the
   PRD wraps the *protocol*, not a specific binary). Serves the `LocalLlm`
   route: low-stakes generative turns. Streams tokens to the TTS path, enforces
   a latency/timeout budget, and **falls back to `Escalate` on timeout, empty
   output, or low local confidence** — degrading to Sonnet rather than to
   silence. No weights vendored; endpoint URL + model id are config.

### Brain backend ladder (jsy decision, 2026-05-29)

jsy chose **local-first with an escalation ladder** for the brain backend:

```
local-3b (qwen2.5:3b, DEFAULT)  →  local-8b (qwen3:8b)  →  Sonnet  →  Opus
```

- **Default tier is local-3b** — free, fast, serves the floor of turns.
- **Switches move UP the ladder "when needed"** — both *manual* (a swap-for-
  next-turn + set-default surface, generalizing wmd's existing
  `swap-model`/`default-model`) and *automatic* (a tier returning `Escalate`
  — `wm-local-llm`'s failure/low-confidence outcome — bumps to the next tier).
- The two local tiers (3b/8b) are the same `wm-local-llm` client with different
  `model` config; Sonnet/Opus are the existing Anthropic client. This is a new
  component — **PRD-brain-backend-ladder** (rust-extend → wintermute-brain) — to
  be drafted next. It supersedes the earlier "manual switch vs fallback vs
  default" open question: the answer is *all three on one ladder, default local*.

## Order

```
PRD-brain-prompt-cache   (independent; ship first — pure per-call saving)
PRD-brain-backend-ladder (rust-extend wintermute-brain; consumes wm-local-llm;
                          local-3b default + switches up to 8b/Sonnet/Opus)

PRD-wm-router ──┬── PRD-wm-skills      (router dispatches to skills)
                ├── PRD-wm-semcache    (router dispatches to cache)
                └── PRD-wm-local-llm   (router dispatches low-stakes generative turns)
```

- **brain-prompt-cache is fully independent** and the highest-ROI / lowest-risk
  PRD — it changes no call volume, only per-call cost, with no quality
  tradeoff. Ship it first regardless of the rest.
- **wm-router is the integration spine.** wm-skills, wm-semcache, and
  wm-local-llm are the three tiers it dispatches to; all depend on the router's
  `Route` taxonomy but are otherwise independent of each other and build in
  parallel.
- The router/skills/cache are **libraries**, not daemons — they are consumed by
  the `wintermute-dialog` FSM (which already owns intent routing; see
  family-intents). Each lib is testable in isolation with fixtures.

## Open questions

1. **Component 6 — dialog-FSM wiring (not drafted).** The libs realize value
   only when `wintermute-dialog` consults the router between its
   Transcribing → Thinking states and short-circuits to TTS on a skill/cache/
   local-model hit. That wiring is a 6th component, deliberately left as a bullet
   until `PRD-wintermute-dialog-turn-fsm` (in-flight) has shipped — the exact FSM
   insertion point depends on that state machine being real. Don't draft it
   blind. **For /build:** do not wire the router into dialog before the turn-FSM
   PRD is shipped.
2. **family-intents overlap.** `PRD-wintermute-family-intents` adds a Family
   branch *directly* to the dialog FSM. Under thrift, "reach family" is one
   skill in the wm-skills registry. These must not both own the intent — either
   family-intents ships first and wm-skills wraps/delegates to its
   `wm.family.*` contract (preferred — don't fork the topic contract), or
   wm-skills defers the family skill to family-intents. Resolve before wiring.
3. **Where does the brain learn a turn was pre-handled?** When the router serves
   a turn locally, does the brain still see it (for memory/continuity) via a
   `recall.save_fact`, or is it invisible? A companion that forgets she asked
   the time five times may be fine; a companion that forgets she set a reminder
   is not. Lean: skills with side effects (reminders, family messages) write to
   recall; pure lookups (time, weather) don't. Confirm with jsy.
4. **Confidence floor calibration.** The router's escalate-when-unsure floor is
   the single knob that trades cost against quality. It needs a labeled
   fixture set and a stated default (lean: tuned so false-deflection of an
   open-ended turn is < 1%, accepting that some deflectable turns escalate).
5. **Local-LLM stakes boundary (component 5).** Which generative turns are
   "low-stakes enough" for the local model vs. warrant Sonnet? This is a second
   calibration knob distinct from the skill/escalate floor. Lean: start with the
   local model OFF in the router (route only to skills/cache/Sonnet), turn it on
   for an explicit allowlist of intents (greetings, acknowledgments, "tell me a
   joke") once jsy's chosen runtime + model prove acceptable quality in his test
   window. The wm-local-llm lib ships regardless; the router gating it on is the
   reversible switch.
