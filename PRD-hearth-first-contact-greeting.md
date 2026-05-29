# PRD: hearth — the first thing she says

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/hearth.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** PRD-hearth-persona-config
**Codename:** *threshold* — what she says when someone crosses it the first time.

## TL;DR

`wintermute-brain` has a `recap_opener: bool` config flag
(`src/lib.rs:100`) that "proactively publishes a `wm.brain.reply`
greeting before the user's first turn when a recent thread was found" —
but **nothing defines what she actually says**, and there is no distinct
welcome for the very first time the device is ever summoned. This is
`companion.md` Open Question #4 verbatim: *"What does she hear when she
summons it for the first time at her home?"* This PRD defines the
first-contact greeting: a warm, plain-language welcome that, on first
ever summon, tells the user what they can say — distinct from the
ordinary day-opener recap.

## 1. Why this exists

- **The flag exists; the content doesn't.** `src/lib.rs`
  `recap_opener` (default `false`) gates a proactive opener, added by
  `PRD-wmd-session-recap §2.3`. The recap opener references *yesterday's
  thread* — it presumes there *is* a history. On a freshly deployed
  device there is none, so the most important greeting (the first one)
  is exactly the case the existing flag doesn't cover.
- **First contact sets the whole relationship.** For a non-technical
  elder, a greeting like "wintermute daemon ready" (or silence) is
  alienating. `visions/companion.md` OQ#4 calls this out and
  `visions/hearth.md` makes it Component 2.
- **It must teach, not just greet.** The user won't know the wake word,
  won't know she can ask for the time / weather / to call family. The
  first-contact line is the only natural place to say "you can ask me…".
- **It must use the persona's register.** Built on
  `hearth-persona-config`: the greeting is composed from the same
  `self_name` / `user_name` / register so it sounds like the same
  person who answers turns.

## 2. What this builds

### 2.1 A first-contact state, distinct from the recap opener

Add a `greeting` module to `wintermute-brain` with three greeting kinds,
selected at the daemon's first proactive publish after boot:

| Kind | When | Content shape |
|---|---|---|
| `FirstEver` | no `wintermute-profile` facts **and** no prior thread memories in recall | warm welcome + 2–3 example things to say + the wake word |
| `Returning` | prior thread found (the existing `recap_opener` path) | brief warm re-greeting, optionally referencing the last thread — unchanged behavior, now register-composed |
| `Silent` | `recap_opener = false` and not first-ever | no proactive publish (today's default) |

Detection of `FirstEver` reuses the existing recall surfaces: query the
`PROFILE_SUBJECT` ("wintermute-profile") and the `THREAD_SUBJECT_PREFIX`
("wintermute-thread-") subjects already defined in `src/lib.rs`. Zero
hits across both ⇒ first ever.

### 2.2 Greeting composition

The greeting text is composed from `PersonaConfig` (dependency), not
hardcoded prose, so it inherits the register:

> *(FirstEver, warm-elder)* "Hello {user_name}, I'm {self_name}. You can
> talk to me out loud whenever you like — just say '{wake_word}' first.
> Try asking me what time it is, or to tell you about the weather. I'm
> here whenever you need me."

`{wake_word}` is read from the deployment's wake-word setting if exposed
on the bus/config; otherwise a config field `persona.wake_word`
(defaulting to "hey wintermute") supplies it. The example actions are a
short static list for v1 (time, weather) — not a live capability probe
(that's a future PRD).

### 2.3 Config

Add `greeting_mode` to drive this without forcing `recap_opener`
semantics onto first-contact:

```toml
[persona]
# … from hearth-persona-config …
greeting       = "auto"   # auto = FirstEver/Returning detection; off; first-ever-always
wake_word      = "hey wintermute"
```

`auto` is the shipped default's recommendation but the **field defaults
to `off`** to preserve today's conservative no-proactive-speech
behavior; the deployment (companion-boot) opts in.

### 2.4 Publish path

On first proactive opener, publish `wm.brain.reply` exactly as the recap
opener does today (so the dialog FSM → wm-tts path is unchanged). The
greeting is published **once per daemon boot**, not per wake, guarded by
an in-process `greeted: bool`.

## 3. Acceptance criteria

1. **AC1 — tests grow.** `cargo test --release --lib` ≥ current+6
   (first-ever detection on empty recall, returning detection with a
   seeded thread, silent when `greeting=off`, register composition
   substitutes name + wake word, examples present in first-ever text,
   greet-once guard).
2. **AC2 — first-ever fires on empty recall.** With a fake recall client
   returning zero hits for both `wintermute-profile` and
   `wintermute-thread-*`, and `greeting = "auto"`, the daemon's first
   proactive publish is a `wm.brain.reply` whose text contains the wake
   word and at least two example prompts.
3. **AC3 — returning path preserved.** With a seeded prior thread and
   `greeting = "auto"`, the opener is the `Returning` kind (no
   "you can ask me" teaching block) — the existing `recap_opener`
   behavior, now register-composed. A test asserts the two kinds differ.
4. **AC4 — off is silent.** `greeting = "off"` (the default) ⇒ no
   proactive `wm.brain.reply` is published at boot. Default-config test.
5. **AC5 — greet once.** Two boot-opener invocations in one daemon
   lifetime publish the greeting exactly once (guard test).
6. **AC6 — register inheritance.** With persona `self_name = "Ada"`,
   `user_name = "Mum"`, the first-ever greeting contains "Ada" and "Mum"
   and none of "daemon"/"API"/"config".
7. **AC7 — live gate (documented, not auto).** With the full fleet up
   and a wiped recall store, a first summon produces an audible welcome
   through wm-tts naming the wake word. Documented for the deployment
   verification run; not an automated AC.

## 4. Non-goals

- Live capability enumeration in the greeting (static example list for
  v1).
- Changing `recap_opener`'s thread-referencing behavior — `Returning`
  preserves it.
- Multi-turn "say that again" / read-back — that's the
  continuity-of-conversation vision.
