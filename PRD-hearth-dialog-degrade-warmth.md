# PRD: hearth — she doesn't say the same thing twice

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/hearth.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-dialog
**build_version_bump:** minor
**Depends on:** none
**Codename:** *not-a-broken-record* — vary the stumble.

## TL;DR

`wintermute-dialog`'s degrade bank (`src/degrade.rs`) returns the
**identical** `"Sorry, I didn't catch that."` for two different failure
modes (`SttUncertain` and `TranscribeTimeout`), and returns the same
single phrase every time a mode recurs. To a user who mis-speaks twice
in a row, the companion sounds like a broken record reading a card. This
PRD replaces the static `phrase_for` lookup with a small, mode-distinct,
gently-rotating phrase bank that shares the hearth register — so two
stumbles in a row sound like a patient person, not a stuck machine.

## 1. Why this exists

Found live in Phase 1 (2026-05-29), reading `src/degrade.rs`:

- **Two modes, one phrase.** Lines 44–45:
  `DegradeKind::SttUncertain => "Sorry, I didn't catch that."` and
  `DegradeKind::TranscribeTimeout => "Sorry, I didn't catch that."` —
  byte-identical. The recognizer abstaining and the transcribe timer
  elapsing are different events that deserve different responses.
- **The module admits it's a placeholder.** Its own doc comment: *"short,
  blunt utterances … for v0.2 the goal is *something* that tells the
  user the turn ended."* and *"The companion … PRD will replace these
  with mood-aware phrasing."*
- **That forward-reference is mis-aimed.** `PRD-wintermute-companion-degrade`
  (codename *say-so*) builds a phrase bank in **wm-brain**
  (`build_into: wintermute-brain`) keyed by *component error kinds*
  (`brain_unreachable`, `audio_mic_missing`, …) for *operational
  faults*. It does **not** touch this wm-dialog FSM bank, which fires on
  *conversational* collapse (uncertain transcript, think/transcribe
  timeout). No PRD covers this file — confirmed by grep across the
  autobuilder queue. `visions/hearth.md` makes it Component 3.
- **No repetition guard.** `phrase_for` is a `const fn` returning a
  single `&'static str`; there is no variation, so identical audio plays
  on every recurrence.

## 2. What this builds

### 2.1 A rotating, mode-distinct phrase bank

Replace the single-phrase `phrase_for` with a `DegradeBank` that holds,
per `DegradeKind`, a small ordered set of register-matched phrases and
rotates through them so consecutive failures of the same kind differ:

```rust
pub struct DegradeBank { cursors: [usize; N_KINDS] }

impl DegradeBank {
    /// Next phrase for `kind`, advancing that kind's rotation cursor.
    pub fn next_phrase(&mut self, kind: DegradeKind) -> &'static str { … }
}
```

Mode-distinct copy (warm-elder register; ≤ 80 chars each for TTS, the
existing test ceiling):

| Kind | Phrases (rotated) |
|---|---|
| `SttUncertain` | "Sorry, I didn't catch that." / "Hm, I didn't quite hear you." / "Could you say that again?" |
| `TranscribeTimeout` | "I'm still listening — go ahead." / "Sorry, I lost the thread there. Once more?" |
| `BrainError` | "Something went wrong on my end. One moment." / "Sorry — let me try that again." |
| `ThinkTimeout` | "That's taking me a moment. Bear with me." / "Sorry, that took too long." |

`SttUncertain` and `TranscribeTimeout` now lead with **different**
phrases, so back-to-back mixed failures never repeat verbatim.

### 2.2 Keep the FSM call-site simple

The FSM holds one `DegradeBank` in its state (it already owns a
`tokio::Mutex`-guarded state struct, per the README). Where it currently
calls `phrase_for(kind)`, it calls `bank.next_phrase(kind)`. No new bus
topics, no config file required for v1 (the phrases are compiled-in but
*varied*); a future PRD can source them from the shared hearth register
config (vision OQ #2).

### 2.3 Preserve the contracts that still hold

- `from_timeout` mapping (`Capture → None`, `Transcribe → TranscribeTimeout`,
  `Think → ThinkTimeout`) is unchanged — capture timeout stays silent.
- The `< 80` char TTS ceiling is preserved for **every** phrase.
- AC6 of the FSM PRD asserted `phrase_for(SttUncertain)` contains
  "didn't catch". That literal is preserved as `SttUncertain`'s **first**
  rotation entry, so the existing contract holds on a fresh bank; the
  test is updated to read the first phrase from a fresh `DegradeBank`.

## 3. Acceptance criteria

1. **AC1 — tests grow.** `cargo test --release --lib` ≥ current+5
   (rotation advances and wraps per kind, all phrases non-empty and
   `< 80` chars, SttUncertain ≠ TranscribeTimeout first phrases, fresh
   bank preserves the AC6 "didn't catch" literal, capture-silent
   unchanged).
2. **AC2 — modes differ.** On a fresh `DegradeBank`,
   `next_phrase(SttUncertain) != next_phrase(TranscribeTimeout)`.
3. **AC3 — no immediate repeat.** For any kind with ≥ 2 phrases, two
   consecutive `next_phrase(kind)` calls return different strings.
4. **AC4 — rotation wraps.** Calling `next_phrase(kind)` `len+1` times
   returns the first phrase again (cursor modulo length).
5. **AC5 — TTS ceiling.** Every phrase for every kind is non-empty and
   `< 80` chars (the existing `every_kind_has_non_empty_phrase`
   invariant, extended to all entries).
6. **AC6 — legacy contract preserved.** A fresh `DegradeBank`'s first
   `SttUncertain` phrase (lowercased) contains "didn't catch" — the
   FSM-PRD AC6 invariant, ported to the new API.
7. **AC7 — capture stays silent.** `DegradeKind::from_timeout(Capture)`
   is `None` (unchanged regression guard).

## 4. Non-goals

- Config-sourced phrases (compiled-in but varied for v1; shared-register
  sourcing is vision OQ #2).
- Touching wm-brain's `companion-degrade` fault bank — different repo,
  different concern (operational faults vs conversational collapse).
- Randomized (vs round-robin) selection — deterministic rotation is
  testable and sufficient; `Math.random`-style nondeterminism is
  explicitly avoided.
