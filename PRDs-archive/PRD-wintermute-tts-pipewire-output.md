# PRD: wintermute-tts — PipeWire output (audio actually plays)

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-tts
**build_version_bump:** minor
**Depends on:** PRD-wintermute-fleet-bus-startup-defect (shipped), PRD-wintermute-fleet-bus-heartbeat-keepalive (shipped), PRD-wintermute-tts-error-loop-suppress (shipped)
**Codename:** *sayit* — the daemon synthesizes; this PRD makes it audible.

---

## TL;DR

`wm-tts` is now bus-healthy, error-loop-free, and renders Piper WAVs end-to-end (verified 2026-05-28T21:53Z — 8/8 pre-render success, on-demand `wm.tts.speak "hello, wintermute is alive"` produced a valid 88KB mono-22050Hz WAV at `~/.cache/wintermute/tts/en_US-lessac-medium/89fac446568980c3.wav`). But no audio reaches the user's speakers. The source documents this gap in three places (`synth.rs:4`, `cache.rs:7`, `lib.rs:8`, `bus.rs:111`) all pointing at a planned iter-4 / iter-6 PipeWire enqueue that never landed. This PRD ships that enqueue: when a `wm.tts.speak` event renders (or hits cache), the WAV is streamed to the configured `WM_SINK_NODE` and the daemon emits `wm.tts.start` / `wm.tts.end` envelopes around the playback span.

The bare-minimum implementation: `pw-cat --target $WM_SINK_NODE --playback <wav>` subprocess. That gets audio to the speaker today. The pipewire-rs streaming implementation (with barge-in mid-playback cancel) is the obvious follow-on but out of scope here — see §6.

---

## 1. Current state (what's missing, with evidence)

1. **WM_SINK_NODE is read but unused.** `/etc/wintermute/conf.d/00-bootstrap.env` has `WM_SINK_NODE=alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Speaker__sink`. The daemon parses it (see config code) but no code path consumes it.
2. **bus.rs:111 explicitly says so:** the metric for "audio bytes played" `// always reports 0 because no PipeWire output is wired yet`.
3. **synth.rs:4:** `// PipeWire enqueue + barge-in cancel land in iter-4 alongside the …` — never shipped.
4. **cache.rs:7:** `// AC3) — just a PipeWire enqueue of the file.` — never shipped.
5. **lib.rs:8:** `// and a PipeWire-rs streaming consumer land in iter-6.` — never shipped.
6. **Manual `pw-play <wav>` works** — confirmed audible on this laptop via the user's HDA speaker sink. So the OS-level audio path is fine; the gap is purely in wm-tts code.

The pre-render success at `wm-tts: pre-render complete voice=en_US-lessac-medium phrases=8 hits=0 rendered=8 failures=0` and the live `wm.tts.speak` rendering at `path=.cache/wintermute/tts/en_US-lessac-medium/89fac446568980c3.wav` both confirm the synth half is healthy. This PRD closes the playback half.

---

## 2. Functional requirements

### 2.1 Default playback backend: `pw-cat` subprocess (iter-1)

When the daemon needs to play a rendered WAV:

```
pw-cat --target "$WM_SINK_NODE" --playback "$wav_path"
```

If `WM_SINK_NODE` is empty, omit `--target` so PipeWire routes to the default sink. The subprocess is `tokio::process::Command`; the dispatch loop awaits its exit so playback completes before the next event.

Why `pw-cat` and not `pipewire-rs`: zero new Rust deps, identical install footprint (`pw-cat` ships with `pipewire` which is already on the laptop), gets us to "audible output today." The pipewire-rs streaming consumer in lib.rs:8's comment is a separate concern — iter-1 is just `pw-cat`.

### 2.2 Event lifecycle

When `wm-tts` processes a `wm.tts.speak` request:

1. **`wm.tts.start`** — publish before invoking playback: `{"phrase_hash": "<hash>", "voice": "<voice>", "duration_ms_estimate": <from-wav-header>}`. Dialog FSM uses this to enter "speaking" turn state.
2. **`wm.tts.end`** — publish after `pw-cat` exits cleanly: `{"phrase_hash": "<hash>", "outcome": "ok" | "cancelled" | "error", "duration_ms_actual": <wall-time>}`. Dialog uses this to return to "listening" turn state.
3. **`wm.tts.error`** — publish if playback fails (non-zero `pw-cat` exit, missing sink, etc.). Per the error-loop-suppress fix, this topic is in the self-emitted allow-list and won't re-trigger the decode-loop.

### 2.3 Cancellation (basic)

If a `wm.tts.cancel` event arrives mid-playback, send SIGTERM to the running `pw-cat` PID. Wait up to 200 ms for clean exit; SIGKILL if it doesn't. Emit `wm.tts.end` with `outcome: "cancelled"`. This is the spot the `synth.rs:4` comment calls "barge-in cancel" — full barge-in (wake-word-triggered cancel from wm-audio) is out of scope, but the cancel hook is here for it to hang off.

### 2.4 Playback metric

The `played_bytes` counter referenced at `bus.rs:111` should now track actual bytes streamed. Use the WAV's data-chunk size from the file header (cheap, no streaming counter needed). Bump the counter on `wm.tts.end` with `outcome: "ok"`.

---

## 3. Acceptance tests

All ACs verified live, not just locally. Don't park at user-gate-blocker.

1. **AC1 — `cargo test --release --lib` exits 0.** Current count is 88 tests (after the error-loop suppress + heartbeat fold-in). Expect +3 to +5 from this iter (one for the start/end publish lifecycle, one for cancel, one for the metric bump). Final ≥ 91.
2. **AC2 — daemon survives 60s under systemd post-bump.** `systemctl --user restart wm-tts.service && sleep 60 && systemctl --user is-active wm-tts.service` → `active`, NRestarts=0.
3. **AC3 — `wm.tts.speak` produces audible output.** Verifiable via journal: `journalctl --user -u wm-tts.service --since "10 sec ago" | grep "play:"` shows a `play: started path=…wav` and `play: ended outcome=ok dur_ms=…` pair within 5s of the publish. The user is invited to confirm audibility — the gate from the code side is that `pw-cat` exited 0 with non-zero `dur_ms`.
4. **AC4 — `wm.tts.start` and `wm.tts.end` envelopes round-trip.** `agorabus subscribe wm.tts.` in one terminal; `agorabus publish wm.tts.speak '{"text":"smoke","priority":"normal"}'` in another. Expect to see (in order): `wm.tts.start` envelope, [audio plays], `wm.tts.end` envelope with `outcome: "ok"`.
5. **AC5 — `wm.tts.cancel` mid-playback.** Speak a long phrase (10+ seconds). Within 1s, publish `wm.tts.cancel`. Expect: `pw-cat` killed within 200ms, `wm.tts.end` envelope with `outcome: "cancelled"`, no leaked `pw-cat` PID (`pgrep pw-cat | wc -l` = 0).
6. **AC6 — `played_bytes` metric is non-zero after a play.** Whatever the metric surface is (presumably `wm-tts metrics` CLI subcommand or a `wm.tts.metric` envelope), it reports a value > 0 after AC3. The `bus.rs:111` comment "always reports `0`" must be deleted as part of the diff.
7. **AC7 — `cargo deny check bans licenses sources` clean** (CVSS4 workaround).
8. **AC8 — wm-audio unchanged baseline.** `git -C ~/wintermute/wintermute-audio status --short` empty.
9. **AC9 — fail-open: no sink configured.** With `WM_SINK_NODE=""`, daemon still starts and `wm.tts.speak` still emits `wm.tts.start`/`end`; playback goes to default sink. Test by `systemctl --user set-environment WM_SINK_NODE=` and restarting.
10. **AC10 — fail-soft: pw-cat missing.** With `pw-cat` not on `$PATH`, `wm.tts.speak` emits `wm.tts.error` with `kind: "pw_cat_missing"`, then a `wm.tts.end` with `outcome: "error"`. Daemon does NOT crash. Test by overriding `PW_CAT_BIN` to a nonexistent path.

---

## 4. Non-goals

1. **Full `pipewire-rs` streaming.** Subprocess `pw-cat` is the v0.1; the streaming consumer described in `lib.rs:8` is a separate iter.
2. **Wake-word-triggered barge-in.** The cancel hook is here (AC5); wm-audio publishing `wm.tts.cancel` on wake is a different PRD (probably wm-audio side).
3. **Queue / playback ordering.** If two `wm.tts.speak` events arrive while one is playing, behavior is implementation-defined for this PRD. A simple "queue them, play in order" is fine; a "drop the second" is also fine. Document the choice in the CHANGELOG.
4. **Voice pack discovery.** The current `~/.local/share/wintermute/tts/models/` flat layout is sufficient. Pack-from-URL discovery is a future PRD.
5. **Cloud fallback.** `WM_CLOUD_TTS_QUALITY=true` + ElevenLabs is wired in voicepack.rs but not in scope here.

---

## 5. Configuration

Add to `/etc/wintermute/conf.d/00-bootstrap.env` schema (already there, just consumed now):

```
WM_SINK_NODE=alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Speaker__sink
WM_PW_CAT_BIN=/usr/bin/pw-cat       # optional; defaults to "pw-cat" on $PATH
```

No new fields. The daemon reads these on startup and on `wm.tts.reload_voice` (existing event).

---

## 6. Future PRDs (do not implement here)

- **PRD-wintermute-tts-pipewire-streaming:** replace `pw-cat` subprocess with `pipewire-rs` streaming consumer for sub-100ms TTFB and accurate `played_bytes` per-frame. The `lib.rs:8` comment promises this for "iter-6."
- **PRD-wintermute-audio-barge-in:** wm-audio publishes `wm.tts.cancel` whenever wake-detector fires during a known `wm.tts` span (between `start` and `end` envelopes). Pairs with AC5 here.
- **PRD-wintermute-tts-queue-policy:** explicit queue / drop / replace policy when overlapping speak requests arrive.

---

## 7. Investigation hints

These are starting points; autobuilder is free to root-cause differently.

1. **Read `daemon.rs` around the `wm.tts.speak` handler** to find the place where `synth.render()` returns a path. That's where to plug in the `play(path)` call.
2. **Read `synth.rs:100-120`** for the existing subprocess pattern (`tokio::process::Command::new("piper")...spawn()...wait().await`). The `pw-cat` invocation is the same shape.
3. **The phrase cache (`cache.rs`)** already maps `(voice, text) → wav_path`. Use the existing cache lookup before render; only invoke synth on miss. Pre-rendered phrases play instantly.
4. **`bus.rs:111`** is the metric surface to update. Find the field, increment in `wm.tts.end`-with-ok handler, delete the "always reports 0" comment as part of the diff.
5. **`pw-cat --help`** for the exact flag spelling. On this laptop: `--target <node>` and `--playback <file>`.

---

## 8. Definition of done

- All 10 ACs pass live.
- Diff includes deleting the four stale comments (`synth.rs:4`, `cache.rs:7`, `lib.rs:8`, `bus.rs:111`) since this PRD ships what they pointed at.
- CHANGELOG entry under v0.2.0 (minor bump — feature add) summarizes the new playback path and the queue/drop policy choice.
- README.md "Recent" section appended with a bullet for the new playback capability.
- Service restarted; user-side smoke check: `agorabus publish wm.tts.speak '{"text":"wintermute is online","priority":"normal"}'` → audible "wintermute is online" through the speaker.
