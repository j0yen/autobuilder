# PRD: wintermute-music — voice-driven MPRIS player control

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`
build_target: rust-cli
build_priority: low

---

## TL;DR

A daemon `wm-music` exposing a single small surface for music: play,
pause, next, prev, volume, what's-playing — over MPRIS via D-Bus.
Provider-agnostic: drives whatever supports MPRIS (Spotify desktop,
Rhythmbox, mpv, VLC, Audacious, Tidal via the desktop client). No
in-process audio; this is purely a remote control.

---

## 1. Why this exists

Vision §End-state #9: *"music through MPRIS"*. The "use what's
already running" pattern is the right cut: don't build a player,
control the player she already has.

Concrete evidence from Phase 1:

- `mpris` crate (MIT) wraps `zbus`, gives a clean per-player handle.
- The laptop already runs `pipewire`; MPRIS is independent of audio
  routing (lives on the session D-Bus). No conflict with
  `wm-audio`'s AEC/wake-mic chain.
- Spotify desktop client on Arch ships an MPRIS interface; same
  for Rhythmbox, mpv with `--input-ipc-server` mode, VLC.

---

## 2. What this builds

### 2.1 Binary: `wm-music`

Daemon. Subscribes to D-Bus name-owner-changed for
`org.mpris.MediaPlayer2.*` so it knows what players are available
right now.

### 2.2 Tools (topic `wm.music.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `players` | `{}` | `{players:[{id, name, status}]}` |
| `play` | `{player_id?}` | `{ok}` |
| `pause` | `{player_id?}` | `{ok}` |
| `toggle` | `{player_id?}` | `{ok}` |
| `next` | `{player_id?}` | `{ok}` |
| `prev` | `{player_id?}` | `{ok}` |
| `now_playing` | `{player_id?}` | `{title, artist, album, duration_s, position_s}` |
| `set_volume` | `{level_0_1, player_id?}` | `{ok}` |

`player_id` defaults to the first non-stopped player; brain can
disambiguate ("you have Spotify and VLC running, which?").

### 2.3 Brain integration

Brain registers `music.*` tools. Single most-common ask is
"pause my music" (mid-conversation) — this is the half-second case;
keep latency low (D-Bus is local).

### 2.4 What this does NOT do

- Search Spotify catalogs (out of scope; cloud-API + auth).
- Launch a player from scratch ("play Bach" with nothing running).
  Future Fleet 3 PRD: `wintermute-music-launcher` that knows how to
  start a player + queue something via provider API.

---

## 3. Risks

- **No player running** — every tool returns `{ok:false,
  reason:"no_player"}` cleanly. Brain says "nothing's playing right
  now" without further error.
- **Player without MPRIS** — Browser-based players (YouTube in
  Firefox) MAY expose MPRIS via the browser; not guaranteed.
  Document Firefox's `media.hardwaremediakeys.enabled` setting.
- **D-Bus session bus** — requires the wmd supervisor's session
  D-Bus to be the same as the user's. `wm-platform` Fleet 1
  arranges this; cross-check in install.sh.

---

## 4. Sequencing

Smallest of Fleet 2. Independent of everything else. Can ship in a
single autobuilder cycle.

---

## 5. Acceptance criteria

1. `wm-music players` with Spotify running lists Spotify with
   `status` ∈ {`playing`, `paused`, `stopped`}.
2. `wm-music pause` on a playing Spotify pauses it within 200 ms
   (measured by next `now_playing` reporting paused).
3. `wm-music play` resumes from the same position.
4. `wm-music toggle` flips between playing↔paused.
5. `wm-music next` advances one track; `now_playing` reports the
   new title.
6. `wm-music prev` returns to the previous track.
7. `wm-music now_playing` returns `title`, `artist`, `duration_s`,
   `position_s` for the active Spotify track.
8. `wm-music set_volume {level_0_1:0.3}` halves volume from 0.6 to
   0.3 (verified via player UI or `pw-cli`).
9. With no MPRIS player running, every tool returns `{ok:false,
   reason:"no_player"}` (not an error trace).
10. **[live]** Real round-trip: music is playing; jsy says "pause
    my music", brain calls `pause`, dialog confirms verbally.
    End-to-end <2 s.
