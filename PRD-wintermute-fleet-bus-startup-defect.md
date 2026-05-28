# PRD: wintermute fleet — post-announce bus-startup defect

**Author:** Claude (for the user)
**Status:** Draft v0.1
**Date:** 2026-05-28
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute  # multi-repo; see §3 per-target
**build_version_bump:** patch
**Depends on:** PRD-wintermute-fleet-agorabus-announce-fix (shipped 2026-05-28T08:05:18Z)
**Codename:** *aftercough* — the bus stopped reporting `announce_required`, but three daemons still fall over within ~1s of start.

---

## TL;DR

The announce-fix shipped overnight (4 iters, all four daemons patched) — the bus daemon no longer rejects subscribe-before-announce. But the four reference test starts (`systemctl --user start wm-{tts,stt,dialog}.service`) reveal a sibling defect: **wm-stt and wm-dialog exit on `error=send_line`** within 6ms of start; **wm-tts exits cleanly with `bus closed; daemon exiting`** immediately after `subscribed prefix="wm.tts."`. wm-audio (the reference implementation) is healthy across multiple restarts. All four daemons already use the correct dual-`Client` pattern (separate `sub_client` + `pub_client`), so the issue is not the announce-fix arc revisiting itself.

This PRD asks /autobuilder to find the actual root cause and ship a patch series across the same four repos (tts, stt, dialog, brain) under the same "verify by `systemctl --user start` survives ≥60s" gate.

---

## 1. Observed symptoms (2026-05-28T15:41 local, fresh start each daemon)

### wm-tts (`wintermute-tts/src/daemon.rs:868`)

```
wm-tts: pre-render complete voice=en_US-lessac-medium phrases=8 hits=0 rendered=0 failures=8
wm-tts: subscribed prefix="wm.tts."
wm-tts: bus closed; daemon exiting
```

`Active: inactive (dead)`, `ExecMainStatus=0`. Daemon exits **gracefully** — it thinks the subscribe stream returned None and walks out. The 8 `failures=8` for pre-render is a separate issue (no piper binary installed — `WM_CLOUD_TTS_QUALITY=false` and no AUR `piper-tts` package); that path is recoverable and not what kills the daemon.

### wm-stt (`wintermute-stt/src/daemon.rs:220,240` for the two Clients)

```
wm-stt start: config resolved cfg=SttConfig { model: "distil-small.en", … cloud_fastpath: false }
wm-stt start: daemon exited with error error=send_line
```

`Active: active (running)` only because systemd's `Restart=on-failure RestartSec=1` is masking it; `NRestarts=1` so far. Exits via the `?` operator on a `send_line` call (`agorabus/src/client.rs:308-312` — wire-level write returning Err).

### wm-dialog (`wintermute-dialog/src/daemon.rs:455,475`)

```
wm-dialog start: daemon exited with error error=send_line
```

Same shape as wm-stt — exit on first `send_line` error. `NRestarts=1`.

### wm-audio (reference, healthy)

```
wm-audio starting session=wm-audio-543036 mic=… wake=hey-jarvis
fanout listening path=/run/user/1000/wintermute/mic.sock
wake detector hot-swapped wake_word=okay-nabu        ← responds to live agorabus publish round-trip
```

`Active: active (running)`, `NRestarts=0`, multiple successful round-trips of `wm.audio.reload` envelopes during the previous session.

---

## 2. What we already ruled out (don't redo this analysis)

1. **It's not announce-missing.** PRD-wintermute-fleet-agorabus-announce-fix shipped 4 iters across all four daemons; bus daemon no longer rejects with `announce_required`. Sibling check: `grep -n announce` shows each repo now has the call.
2. **It's not single-vs-dual `Client`.** All four daemons already create two separate `agorabus::Client` instances (`sub_client` + `pub_client`) — confirmed by `grep -n 'Client::' src/daemon.rs` on all four repos. wm-audio's split pattern is already mirrored everywhere.
3. **It's not the Piper-missing failure in wm-tts.** The `rendered=0 failures=8` line is from pre-render's per-phrase Piper invocation; the daemon proceeds to subscribe afterward and only then exits. Removing the cache config would just hide that line; the bus-closed exit happens after.

---

## 3. Per-repo targets

The fix needs to land in each of these. Treat as four `rust-extend` slices in autobuilder's existing series-style; each repo gets one patch + version-patch-bump + push.

| Repo | First exit point | `build_into` |
|---|---|---|
| wintermute-tts | `daemon.rs:868` "bus closed; daemon exiting" | `/home/jsy/wintermute/wintermute-tts` |
| wintermute-stt | `daemon.rs:266` "bus closed" branch OR upstream `send_line` (`agorabus::Client` wire layer) | `/home/jsy/wintermute/wintermute-stt` |
| wintermute-dialog | `daemon.rs:498` "bus closed" branch OR `send_line` | `/home/jsy/wintermute/wintermute-dialog` |
| wintermute-brain | `daemon.rs:1363` "bus closed" branch (untested live yet — needs WM_ANTHROPIC_API_KEY before runtime gate matters; static fix still in scope) | `/home/jsy/wintermute/wintermute-brain` |

wm-audio is the reference — **do not modify**. Use its pattern.

---

## 4. Investigative starting points

These are hints, not prescriptions. Autobuilder is free to root-cause differently.

1. **Compare wm-audio's subscribe-loop vs wm-stt's.** wm-audio (`src/daemon.rs:127-164`) announces the *subscribe* client separately with a different intent (`"wm-audio control subscribe"`) before calling `.subscribe(prefix)`. The other three also announce twice — but check the `intent` strings, the `pid` and `cwd` payload fields, and any heartbeat callback differences.
2. **Look at `agorabus::Client::send_line` callers.** `client.rs:308-312` is just `write_all + flush`. The error comes back as `Err(context("send_line"))`. The first publish or heartbeat after subscribe is the most likely failing call. Trace which message wm-stt sends next after subscribe completes.
3. **Check for a heartbeat / liveness call** that wm-audio satisfies and the others don't. The bus daemon may close the connection if no heartbeat arrives within N seconds and the daemon's Drop fires.
4. **Could be a `Send` / `subscribe` envelope schema mismatch.** Compare the JSON the wire layer sends in wm-audio's first post-announce message vs wm-stt's. `RUST_LOG=agorabus=trace` on both should show the difference in ≤30 lines.

---

## 5. Acceptance tests

A patch series passes when ALL of these hold for each of the four daemons (audio is the control):

1. **AC1 — local cargo test green.** `cd $build_into && cargo test --release --lib` exits 0 (count varies per crate: tts 83, stt 53, dialog 68, brain 145 at last shipped iter).
2. **AC2 — daemon survives 60s under systemd.** `systemctl --user restart $svc.service && sleep 60 && systemctl --user is-active $svc.service` → `active`, with `NRestarts <= 1` (the initial start may legitimately re-establish once).
3. **AC3 — agorabus peers list shows the daemon.** `agorabus peers | jq -r '.[].session_id' | grep -c "$svc"` ≥ 1 after the 60s window.
4. **AC4 — round-trip works.** Pick a topic the daemon subscribes to and publish a benign message; the daemon's log shows a corresponding "processed" / "received" line within 5s. Specific publishes per daemon:
   - **wm-tts**: `agorabus publish wm.tts.say '{"text":"smoke","voice":"en_US-lessac-medium"}'` → log shows `wm-tts: synth dispatched` or similar
   - **wm-stt**: `agorabus publish wm.audio.speech.start '{"id":"smoke","ts":0}'` → log shows the speech-start being routed into the processor
   - **wm-dialog**: `agorabus publish wm.stt.final '{"text":"smoke","confidence":0.9}'` → log shows turn-state advancing
   - **wm-brain**: deferred — needs `WM_ANTHROPIC_API_KEY` for any useful round-trip; AC4 for brain is "starts and subscribes" only.
5. **AC5 — `cargo deny check bans licenses sources` green** (per CVSS4 workaround memory).
6. **AC6 — wm-audio is unchanged.** `git -C ~/wintermute/wintermute-audio status --short` empty; wm-audio's behavior is the regression baseline.
7. **AC7 — fail-open preserved.** With agorabus daemon not running, each daemon's `try_connect` returns `None` and the binary exits cleanly with status 0 (same shape as the announce-fix PRD's AC4). Test via `systemctl --user stop claude-agorabus.service; systemctl --user start $svc.service; sleep 2; systemctl --user is-failed $svc.service` → `inactive`.

---

## 6. Non-goals

1. **No agorabus protocol change.** wm-audio works; the bus daemon doesn't need fixing. The defect is in the four sibling clients.
2. **No new bus features.** No heartbeat tuning, no new envelope types. If the fix requires one of these, raise it as a follow-on PRD before implementing.
3. **No fix to the Piper pre-render failure in wm-tts.** That's `failures=8` from `pre-render`, a separate path with its own missing-binary story.
4. **No fix to the empty `WM_ANTHROPIC_API_KEY`.** That's a user-config issue, not a defect.

---

## 7. Ordering / dependency notes

- Per-repo patches can land in any order — they're sibling fixes.
- The wm-audio reference is **load-bearing as the unchanged baseline**. Don't touch it.
- Suggested order: tts → stt → dialog → brain. Same as the announce-fix arc — least dependencies first.
- After all four land, archive this PRD only when AC2/AC3/AC4 are paired with live `systemctl is-active` + `agorabus peers` + journal-log evidence in the manifest's `verification` field. The announce-fix PRD is currently parked at `user-gate-blocker` for exactly this reason; this PRD should not repeat that pattern — verify live in the iter where AC2/3/4 land, not after.
