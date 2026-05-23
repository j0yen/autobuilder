# PRD: Ambient Compositions

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / generative sound
**Date:** 2026-05-22
**Audience:** jsy (primary listener), Katherine, Maria
**Form:** long-running generative ambient piece, parameterized by laptop telemetry; daily render archived as .wav
**Cadence:** continuous during work hours; daily snapshot

---

## TL;DR

A generative ambient piece runs while you work. Its parameters — density, key, harmonic content, occasional events — come from the same signal stream that drives the Tide Chart (ctrace, wchg, git, builds). Each day produces a unique 6–10 hour piece, archived as a .wav. Over years: an audible diary of work.

---

## 1. Why this exists

1. Software work is visual and silent. Days bleed together because there's no auditory texture marking the rhythm.
2. Ambient music is honest about being slow, atmospheric, and not-the-point. The right register for "soundtrack to thinking."
3. Generative music driven by *your* signals isn't background — it's a co-author. The piece is in part composed by what you do.
4. An archive of "what yesterday sounded like" is a strange and useful artifact. Re-encountering the texture of a day after the day is over.

## 2. Who this is for

- **Primary:** you, during work, through speakers or headphones.
- **Secondary:** K and M. They can listen to a day's archive (or sit in your workspace while it plays).
- The daily .wav stands alone but is *richer* with context.

## 3. Form

- Sonic medium: drones (3–5 layers), sparse pitched events, occasional percussive grains.
- Pitch material: a small set of modes (one per day, rotating); root note slides slowly across the day.
- Sound sources: a curated library of 30–60s samples you provide — piano, strings, field recordings, found sound. The agent layers and modulates; it doesn't synthesize from scratch.
- Initial mapping:
  - file save → soft chime, key-aligned
  - build pass → low harmonic settle
  - build fail → grain decay
  - new file created → a stretched piano note in a higher register
  - long idle → silence (actually silence)
  - high focus (long sequences in one repo) → slow tonic drift
  - fragmentation (many context switches) → polyrhythmic layering
- Output: continuous audio (live) + daily .wav at `~/wintermute/sound/<date>.wav`.

## 4. Process

```
Sonic Pi or SuperCollider engine (loaded sample library)
   ↑
parameter bus (OSC or stdin) ← Rust orchestrator
                                ↑
                              telemetry collector (same signals as Tide Chart)
                                ↓
                              .wav recorder writes <date>.wav
```

- Engine: Sonic Pi (Ruby DSL, fast) or SuperCollider (denser, more capable).
- Orchestrator: Rust binary; reads telemetry, debounces, maps to OSC, throttles.
- Recorder: `arecord` or SuperCollider buffer write, segmented daily.

## 5. Cadence

- Continuous during work hours (or whenever you launch the player).
- Daily .wav archived at midnight.
- Annual album: pick favorite days, bind into a 12-track virtual album, master, share with K and M.

## 6. Non-goals

1. **Foreground music.** No melodies, no songs. The piece recedes.
2. **Lo-fi beats.** No drums; no genre signature; no "vibe" presets.
3. **Live remix capability.** It composes itself; you don't conduct it.
4. **Distribution.** Personal use + annual album for K and M only.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Manual Sonic Pi experiments — map *one* signal (file save) to *one* sonic event |
| 1 | Telemetry bridge: orchestrator binary + 4-signal mapping |
| 2 | Daily .wav recorder + sample library v1 |
| 3 | Annual album ritual: curate, master, share |

## 8. Risks

- **Generative-ambient default ugliness.** Most algorithmic ambient sounds like a meditation app. *Mitigation:* sample library is curated by you; mapping is iterated against your taste; agent does not pick chords.
- **Distraction.** Sound during work can oppose focus. *Mitigation:* a "silence" mode, easy keyboard kill switch; mute during high-focus periods.
- **Sample licensing.** Anything not your own recordings or CC0/CC-BY is fraught. *Mitigation:* library = field recordings + your piano + Mutable Instruments CC-BY.
- **Hardware ambient.** A noisy laptop fan or open window changes daily listening. *Mitigation:* good headphones during composition; the .wav is canonical.

## 9. Open questions

1. Engine: Sonic Pi for Phase 0; SuperCollider if depth needed?
2. Should the agent ever *break* the piece — a glitch, a wrong note, a clipped sample — when something unusual happens? Or is the discipline always smooth?
3. Annual album: does K or M commission a track (pick a day to sonify)?
4. Should there ever be a quiet spoken-word layer (a half-buried sentence from a recall memory)? Beautiful but potentially gimmicky.
