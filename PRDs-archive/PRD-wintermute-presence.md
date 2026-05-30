# PRD: wintermute-presence — she's okay, without watching her

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-cli
**build_version_bump:** n/a (new repo j0yen/wintermute-presence)
**Depends on:** PRD-wintermute-family-intents
**Codename:** *pulse* — a quiet heartbeat, opt-in, never a camera.

## TL;DR

jsy wants to know his mother is okay without surveilling her. This daemon
listens only to the fact *that* she interacted with wintermute — never the
content — and emits two signals: `wm.presence.summon` each time she talks to
it, and `wm.presence.silence` when no interaction has fallen inside her
configured waking-hours window. These feed the digest (reassurance) and the
silence nudge (gentle worry). Default OFF; she enrolls it knowingly.

## 1. Why this exists

- **kin vision Component 4.** The peace-of-mind heartbeat. Distinct from
  distress (which is her reaching out) — presence is the *absence* of news
  being itself news.
- **The interaction signals already exist on the bus.** Phase 1 grep found
  `wm.audio.wake` and `wm.stt.final` are emitted by the companion fleet
  (`wintermute-audio-inference/src/events.rs`, `wm.stt.final` from wm-stt).
  presence only has to count edges on topics that already flow — no new
  capture, no content inspection.
- **Privacy is the design constraint, not an afterthought.** presence reads
  *that* a turn happened and (optionally) its transcript *length*, never the
  transcript text. It publishes counts and timestamps, never words.

## 2. What this builds

A new repo `j0yen/wintermute-presence`, a long-running agorabus daemon.

### 2.1 Interaction tracking

- Subscribes `wm.audio.wake` and `wm.stt.final`.
- On each, records `last_interaction_ts` and increments a daily counter held
  in a small state file (`~/.local/state/wintermute-presence/state.json` or
  the configured path) so a restart doesn't lose the day's count.
- Emits `wm.presence.summon { ts, transcript_len }` per interaction
  (`transcript_len` is a character count from `wm.stt.final`, or 0 for a bare
  wake) — a number, never the text.

### 2.2 Silence detection

- Config: waking-hours window (e.g. 08:00–21:00 local) + a silence threshold
  (e.g. "no interaction for the whole window so far by HH:MM").
- A timer checks the threshold; if crossed with zero interactions in-window,
  emits exactly one `wm.presence.silence { since_ts, window }` per window
  (debounced — not repeated every tick).
- Outside waking hours, silence is expected and never emitted.

### 2.3 Opt-in gating

- The whole daemon no-ops (subscribes nothing, emits nothing) unless presence
  is enabled in `/etc/wintermute/conf.d/` (set by family-enroll). A device
  with no enrollment never phones home about Mom.

### 2.4 CLI

- `wm-presence daemon` — run the loop (systemd `wm-presence.service`).
- `wm-presence status` — print today's count + last interaction + window.

## 3. Acceptance criteria

1. With presence enabled, a published `wm.audio.wake` produces one
   `wm.presence.summon` on the bus (integration test).
2. A `wm.stt.final { transcript: "hello there" }` produces a
   `wm.presence.summon { transcript_len: 11 }` — the count, and the daemon
   logs/emits **no** transcript text anywhere (assert egress carries no
   substring of the transcript).
3. The daily counter survives a daemon restart (state file round-trip test).
4. With zero interactions inside the configured window, exactly one
   `wm.presence.silence` is emitted after the threshold (debounce test:
   subsequent ticks emit none).
5. An interaction inside the window suppresses the silence emission for that
   window (test).
6. Outside waking hours, no `wm.presence.silence` is ever emitted (test with a
   clock at 03:00).
7. With presence **disabled** in config, the daemon subscribes nothing and
   emits nothing (opt-in gate test) — verified by an empty bus after wake
   events.
8. `wm-presence status` prints today's count and last-interaction timestamp.
9. The daemon applies the self-emitted-topic filter and does not consume its
   own `wm.presence.*`.
10. systemd unit installs at the consistent bin path (no cargo-bin drift).
11. `cargo test` green; `cargo clippy` clean; autobuilder receipts produced.
