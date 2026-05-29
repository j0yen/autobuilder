# Vision: hearth

> The voice loop works. The mic hears, the bus carries, the speaker
> speaks. But the *character* on the other end is a `const` string in
> a source file, and when it stumbles it says the same four blunt words
> twice. Hearth is the warmth at the center of the home — the
> companion's voice made into data, calibrated for the person who will
> actually live with it: a non-technical elder, jsy's mother.

Created: 2026-05-29
Seed: reflection + companion.md's own deferred Open Questions.
Pace: drafts ship to /build per the 2026-05-27 instruction.

## TL;DR

`companion.md` shipped the conversation loop (audio → STT → dialog-FSM →
brain → TTS) and `companion-degrade` (*say-so*) shipped the plumbing
that breaks silence on a fault. Neither addressed the companion's
**personality** — and both explicitly deferred it:

- `companion.md` Open Question #4, verbatim: *"What does she hear when
  she summons it for the first time at her home? … a too-technical
  greeting will be alienating. Companion has a personality question
  lurking under it."*
- `PRD-wintermute-dialog-turn-fsm.md` Non-goal #2: *"Mood / personality
  model. The phrases are blunt for v0.1."*

The evidence that it's still blunt, found live in Phase 1 this session:

- **The persona is a `const` in source.**
  `wintermute-brain/src/daemon.rs:47` —
  `DEFAULT_PERSONA: &str = "You are wintermute, a voice-first companion
  daemon. …"`. Changing how she speaks means editing Rust and
  recompiling. The config struct (`src/lib.rs`) already carries
  `user_name`, `timezone`, `recap_opener` — but **not** the persona.
- **The first-contact greeting has a flag but no content.**
  `wintermute-brain/src/lib.rs:100` — `recap_opener: bool` ("proactively
  publishes a greeting before the user's first turn"), default `false`.
  Nothing defines *what she says* on first summon, and there is no
  distinct first-ever-boot welcome.
- **The dialog degrade bank says the same thing twice.**
  `wintermute-dialog/src/degrade.rs:44-45` — both `SttUncertain` and
  `TranscribeTimeout` return the identical `"Sorry, I didn't catch
  that."`. The module doc calls itself a placeholder; AC6 of the FSM PRD
  asserts the literal string. This is the FSM's own bank, distinct from
  `companion-degrade`'s fault bank in wm-brain — and no PRD touches it.

Hearth is the **voice**; `companion-degrade` is the **plumbing**. They
share the wm-tts path but own different concerns: say-so turns a *fault*
into spoken acknowledgment (keyed by component error kind, rate-limited,
`wm.health.*`); hearth makes *every* spoken line — normal replies, the
welcome, the stumbles — sound like one consistent, kind person rather
than a robot reading a lookup table.

## End-state

When hearth is done:

1. The companion's personality is **configuration, not code**. jsy can
   retune her register (her name, how warm, how brief, what she calls
   the user) by editing `brain.toml` — no recompile, no redeploy. The
   shipped default is calibrated for a non-technical elder: short
   sentences, no jargon, names the user, never mentions "daemon" /
   "API" / "config".
2. The **first thing she says** when summoned for the first time is
   deliberate and warm — a welcome that tells the user, in plain speech,
   what they can say to her. Distinct from an ordinary reply and from
   the day's first recap-opener.
3. When a turn collapses, she **doesn't repeat herself**: the dialog
   degrade bank gives mode-distinct, gently-varied phrasing that shares
   the persona's register, so two failures in a row don't sound like a
   broken record.

## Components

Each is PRD-sized, rust-extend, single-target — the same shape as the
shipped companion fleet (`pipewire-output`, `dialog-turn-fsm`,
`companion-degrade`).

- **PRD-hearth-persona-config** (rust-extend → wintermute-brain) — lift
  `DEFAULT_PERSONA` out of the source `const` into a `[persona]` table
  in `brain.toml`; ship a calibrated elder-friendly default; keep the
  prompt-cache prefix stable. *Foundation.*
- **PRD-hearth-first-contact-greeting** (rust-extend → wintermute-brain)
  — define the content behind the `recap_opener` flag and add a distinct
  first-ever-boot welcome that teaches the user what to say. *Depends on
  persona-config.*
- **PRD-hearth-dialog-degrade-warmth** (rust-extend → wintermute-dialog)
  — replace the duplicated blunt phrases in `wm-dialog`'s `degrade.rs`
  with mode-distinct, rotating, register-matched phrasing. *Independent
  of the brain PRDs.*

## Order

```
persona-config ──► first-contact-greeting        (both → wintermute-brain)

dialog-degrade-warmth                              (→ wintermute-dialog, independent)
```

- persona-config is the foundation: greeting copy references the
  configured name + register.
- dialog-degrade-warmth can ship anytime; it touches a different repo.
- **Coordination note:** persona-config and first-contact-greeting both
  extend `wintermute-brain`'s system-prompt composition, as do the
  in-flight `brain-prompt-cache` and `brain-backend-ladder` PRDs. These
  four touch the same `compose_persona` / `BrainConfig` surface — /build
  should serialize them or expect rebases.

## Open questions

1. **Learned persona facts.** Should how the user likes to be addressed
   (formal vs first-name, louder by default, slower) be *learned* into
   the `wintermute-profile` recall subject (`PROFILE_SUBJECT` exists in
   lib.rs) rather than only configured? Edges into
   continuity-of-conversation territory — left as a future component,
   not a v1 PRD, until persona-config proves the data shape.
2. **One register, two repos.** The persona register lives in
   `brain.toml`; the dialog degrade phrases live in wm-dialog. They can
   drift. A future consistency check (do all spoken surfaces share a
   voice?) is real but premature — deferred until both banks are
   config-sourced.
3. **Voice ↔ words.** wm-tts picks the Piper voice; hearth picks the
   words. The *pairing* (a warm voice model that matches the warm copy)
   is a wm-tts concern (`reload_voice`), noted here so the deployment
   moment considers both together.
4. **Name calibration.** Does she call herself "wintermute" to mother,
   or something softer? The persona default keeps "wintermute" but makes
   the self-name a config field so the deployment can soften it without
   a recompile.
