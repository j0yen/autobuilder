# Vision: wintermute — voice-first AI laptop companion

**Authored by:** /dream (Claude Opus 4.7), with jsy
**Created:** 2026-05-24
**Updated:** 2026-05-27 (Fleet 2 drafted, Fleet 1.5 added, bus-smoke convention)
**Status:** active
**Fleet 1 drafted:** 7 PRDs (foundation), 5/7 shipped as of 2026-05-27
  (bootstrap archived; platform/tts/stt/dialog shipped per CLAUDE_SELF
  changelog but archive pending; audio + brain still queued)
**Fleet 2 drafted:** 6 PRDs (action layer) — 2026-05-27 `/dream extend`
**Fleet 3:** captured as bullets; future `/dream extend wintermute`

---

## TL;DR

A Linux laptop for someone who is completely computer-illiterate.
Power button on → laptop greets her by name within ~15 seconds → she
talks, it listens, it answers, it does things for her. No typing, no
reading required. The brain is Claude (Sonnet 4.6 default); the voice
stack is local-first with optional cloud fast-paths; the action
surface eventually covers browser, desktop apps, mail, calendar,
music, and the open web. Prototyped on this Arch Linux laptop under
the `wintermute` name; she will name her own laptop later.

## End-state

When Fleet 1 ships:

1. **Cold boot to "Hi, I'm here" greeting in ≤15 s** after a one-time
   caregiver setup at `wintermute.local`.
2. **Continuous conversation, no wake-word fatigue.** Microphone is
   always live; AEC prevents her TTS from retriggering wake; she can
   barge in mid-sentence.
3. **Sub-2-second response latency** for short queries (wake →
   first TTS audio).
4. **Conversation context persists** across the day and across
   reboots via `recall`. She can resume a thread from this morning
   when she comes back from a nap.
5. **Verbal-confirmation gating** for destructive actions ("you want
   me to delete the email from your sister — say 'yes delete'").
6. **Graceful offline behavior** when the network drops — a spoken
   apology, not a hang.

When Fleet 2 ships (action layer):

7. **Browse the web by description.** "Find me a recipe for chicken
   soup with celery."
8. **Read what's on the screen** if a sighted helper points at it.
9. **Mail / calendar / music** through MPRIS, IMAP/SMTP, CalDAV.

When Fleet 3 ships (personalization & safety):

10. **Only responds to her voice** (speaker profile), not the TV.
11. **Emergency contact** if she asks for help.
12. **Quiet hours**, undo, multi-user, comforting voice clone.

## Architecture

```
┌───────────────────────────────────────────────────────────┐
│  CONVERSATION   wmd: Claude API loop, prompt caching,     │
│                 recall-backed memory, tool router         │
├───────────────────────────────────────────────────────────┤
│  DIALOG         wm-dialog: turn-taker, barge-in arbiter,  │
│                 verbal-confirmation protocol              │
├───────────────────────────────────────────────────────────┤
│  ACTION (F2)    browser / desktop / mail / cal / music    │
├───────────────────────────────────────────────────────────┤
│  PERCEPTION     wm-stt (whisper.cpp + cloud fast-path)    │
│                 wm-tts (Piper + cloud quality option)     │
│                 screen narrate (F2)                       │
├───────────────────────────────────────────────────────────┤
│  AUDIO          wm-audio: mic → AEC → NS → wake           │
│                 (microWakeWord) → VAD (Silero) → events   │
├───────────────────────────────────────────────────────────┤
│  PLATFORM       wm-bootstrap (one-time caregiver setup)   │
│                 → autologin → systemd `wintermute.target` │
│                 → wmd supervisor → audio pipeline up      │
│                 within ~15 s of power-on                  │
└───────────────────────────────────────────────────────────┘
```

## Foundation choices (cited per PRD)

| Layer | Library | License | Why |
|---|---|---|---|
| Wake | **microWakeWord** (Apache-2.0) | low CPU; same engine HA Voice PE ships; pretrained "Hey Jarvis" / "Okay Nabu" / "Hey Mycroft" |
| VAD | **Silero VAD** (MIT) | industry standard turn-end, ONNX, ~1 MB |
| STT local | **whisper.cpp** + `whisper-rs` (MIT) | default `distil-small.en` on CPU; opt-up at runtime |
| STT cloud | **Whisper API** | optional fast-path when network OK |
| TTS local | **Piper** (MIT) | CPU-only, ~10× real-time, broad voice library |
| TTS cloud | **ElevenLabs** | optional quality path |
| AEC | **PipeWire `module-echo-cancel`** | AEC3 preferred; webrtc classic fallback |
| NS | **NoiseTorch-ng** (GPL-3) | virtual mic source |
| Browser (F2) | **Playwright** (Apache-2.0) | accessibility-snapshot, token-efficient |
| Desktop a11y (F2) | **AT-SPI2** via `atspi-rs` | linux equivalent of macOS AX |
| Desktop input (F2) | **xdotool** (laptop is X11) | reuse from `baton` |
| Brain LLM | **Claude API** | Sonnet 4.6 default, Opus 4.7 opt-in |

Reference architecture: **Home Assistant Voice Preview Edition** —
microWakeWord + Whisper + Piper at 300–700 ms end-to-end on a Pi5.
Proves the local stack is viable at the latency budget needed for
natural conversation.

## Reusable foundation already on this laptop

- `peon-ping` (sound output; TTS designed in its own PRD-003 but
  unbuilt — wm-tts collaborates rather than duplicates)
- `recall` v0.4 (agentic memory; daemon mode in flight via
  `PRD-recall-daemon.md` — wm-brain depends on it)
- `pevent` (supervised background processes — for wmd / wm-*)
- `agorabus` (UDS pub/sub — bus for wake / speech / dialog events)
- `tcap`, `baton` (X11 keystroke injection — useful in Fleet 2)
- `autobuilder` + `/build` skill (PRD → Rust pipeline, ships PRDs
  to standalone `j0yen/<slug>` repos under `~/wintermute/`)

## Fleet 1 — Foundation (drafted 2026-05-24)

All seven PRDs carry `build_auto: true` (user override of the default
`/dream` rule) so `/build` can begin ticking immediately.

| # | PRD | Target | Binary | Notes |
|---|---|---|---|---|
| 1 | `PRD-wintermute-bootstrap.md` | rust-cli | `wm-bootstrap` | caregiver-facing one-time web setup at `wintermute.local` |
| 2 | `PRD-wintermute-platform.md` | mixed | `wmd-init`, `wm` | autologin + systemd target + supervisor |
| 3 | `PRD-wintermute-audio.md` | mixed | `wm-audio` | mic → AEC → NS → wake → VAD → events (merged with wake) |
| 4 | `PRD-wintermute-stt.md` | rust-cli | `wm-stt` | whisper.cpp + optional cloud fast-path |
| 5 | `PRD-wintermute-tts.md` | rust-cli | `wm-tts` | Piper + collaboration with peon-ping PRD-003 |
| 6 | `PRD-wintermute-dialog.md` | rust-cli | `wm-dialog` | turn-taker, barge-in arbiter, verbal-confirm protocol |
| 7 | `PRD-wintermute-brain.md` | rust-cli | `wmd` | Claude API loop + recall memory + tool router |

**Sequencing:**
- #1 bootstrap and #2 platform are the entry gates (no deps).
- #3 audio gates #4-#7 (publishes the speech/wake events they consume).
- #5 tts can land in parallel with #3 (only needs sink; useful to test #2's greeting).
- #6 dialog and #7 brain can develop in parallel; #7 depends on
  `recall-daemon` (`PRD-recall-daemon.md`) shipping for the sub-10 ms
  memory path.

**Risks called out by the planning pass:**
- AEC3 build-flag on Arch's `pipewire` package — fallback to webrtc
  classic if AEC3 missing (cancellation quality reduced but functional).
- microWakeWord pretrained wake words only in v1 — custom training
  is too finicky for a non-literate user's setup. The wake word is
  configurable via bootstrap from the pretrained set.
- Sonnet vs Opus default for the brain — Sonnet is the chatty-day
  default; Opus is opt-in for deep questions to manage cost+latency.

## Fleet 2 — Action layer (drafted 2026-05-27)

Six PRDs drafted; sequencing browser+desktop first (biggest unlocks),
screen-narrate composes with desktop, mail/calendar/music independent
and parallel. All `build_target: rust-cli`. No new external substrate
required beyond what Fleet 1 already arranges.

| # | PRD | Binary | Notes |
|---|---|---|---|
| 1 | `PRD-wintermute-browser.md` | `wm-browser` | chromiumoxide (CDP); no Rust Playwright binding exists |
| 2 | `PRD-wintermute-desktop.md` | `wm-desktop` | atspi-rs + xdotool via baton |
| 3 | `PRD-wintermute-screen-narrate.md` | `wm-screen-narrate` | scrot/grim → Claude vision messages API |
| 4 | `PRD-wintermute-mail.md` | `wm-mail` | async-imap + lettre + freedesktop SecretService |
| 5 | `PRD-wintermute-calendar.md` | `wm-cal` | minicaldav + ical (CalDAV only; OAuth out of scope) |
| 6 | `PRD-wintermute-music.md` | `wm-music` | mpris-rs over zbus; control only, not playback |

**Sequencing:** browser and desktop are the two large foundations
(both ~1 autobuilder cycle each). screen-narrate composes with
desktop but works standalone. mail/calendar/music are independent
and small; music is the cheapest ship.

**Bumped to Fleet 3 (not drafted this pass per dream rule 6 — no
direct user articulation, no End-state pin):**
- `wintermute-news` — RSS + summarize-and-read
- `wintermute-glow` — visual ambient state indicator

## Fleet 1.5 — Maturation & validation (observed pattern 2026-05-27)

Added by `/dream` pass 13 in response to a 4-PRD identical-shape
bottleneck. Fleet 1's publish flurry today shipped 5 PRDs to GitHub,
but 4 of the 5 are now stuck in `in_progress` purgatory because their
acceptance criteria include ground-truth-required ACs (real mic, live
systemctl, AT-SPI bus, 8h soak) that `/build`'s verified-completed
check #5 cannot mechanically satisfy. The gap is structural, not
effortful — combined 68 build-ticks already invested across the four
PRDs without advancing the gate.

| PRD | Stuck on |
|---|---|
| `wintermute-platform` | ACs 1-2 (systemctl/cold-reboot), AC8 (init.backoff event) |
| `wintermute-audio` | Hardware-dependent ACs (PipeWire mic capture) |
| `wintermute-stt` | Hardware-timing ACs + whisper.cpp build dep |
| `wintermute-tts` | Hardware-timing ACs 1/3/5/7 (`#[ignore]`-gated) |

This is not a "ship faster" problem; it is a "make the gap legible"
problem. The fleet ships when the gap is named in the archive commit
rather than hidden in iter-log notes.

| # | PRD | Target | Notes |
|---|---|---|---|
| 1 | `PRD-build-deferred-acs.md` | self-mod | `deferred_acs:` frontmatter + gate honoring + archive trailer + backfill of 4 stuck PRDs. **Drafted 2026-05-27 (pass 13).** |
| 2 | `PRD-wintermute-hardware-smoke-convention.md` | mixed | Codify the empirical `WM_<SLUG>_HARDWARE_SMOKE` env-witness pattern that shipped wintermute-tts. Backfill `tests/hardware_acs.rs` into platform/stt/audio. Convention doc + 3 scaffolded test files; no skill or version changes. Complement, not replacement for #1: hardware-smoke handles in-Rust hardware-dep ACs (dominant wintermute case); deferred-acs handles ACs that exit Rust entirely. **Drafted 2026-05-27 (pass 15).** |
| 3 | `PRD-wintermute-fleet-bus-smoke-convention.md` | mixed | Codify the in-process-agorabus smoke pattern that wm-audio already practices (`wake_bus_smoke.rs` etc.). Backfill canonical `tests/bus_smoke.rs` into tts/stt/dialog/brain — the four daemons just caught shipping with missing `Client::announce()` calls (see sibling `PRD-wintermute-fleet-agorabus-announce-fix.md`). Convention doc + 4 scaffolded test files; no skill, version, or library changes. Pays forward to Fleet 2: every new wm-* daemon must include `bus_smoke.rs` before archive. **Drafted 2026-05-27 (pass 16).** |

**Future Fleet 1.5 (bullets — not drafted this pass):**

- `wintermute-verify` — interactive `wm-verify` CLI that walks the
  declared `deferred_acs:` of a PRD, prompts jsy to attest each one
  against real hardware (and records timestamp + result + notes), and
  outputs a Verified-completed: trailer block ready to paste into the
  archive commit. Turns "deferred-by-design" into "verified-by-
  attestation." Motivated by the same 4 stuck PRDs once their
  declared deferrals exist.
- `build-maturation-log` — a per-PRD journal at
  `~/wintermute/autobuilder/maturation/<slug>.md` capturing
  attestation episodes (run by `wm-verify` or by hand). Lets a future
  reader trace which ACs were verified, when, by whom, against what
  hardware. Motivated once `wm-verify` exists and attestation events
  start happening.

**Sequencing:** `build-deferred-acs` lands first (single tick, unblocks
the 4 stuck PRDs immediately). `wm-verify` lands second once the
declared-deferred ACs are visible in PRD frontmatter. `build-
maturation-log` lands third once `wm-verify` is producing attestation
records that benefit from a structured journal.

## Fleet 3 — Personalization, safety, offline (future)

- `wintermute-voice-profile` — speaker adaptation (only responds to
  her, not to TV / visitors)
- `wintermute-voice-clone` — comforting voice clone (note XTTS v2 /
  F5-TTS license constraints — likely personal-use-only build)
- `wintermute-emergency` — "I'm not feeling well" → contact caregiver
- `wintermute-quiet-hours` — sleep schedule, no proactive sound
- `wintermute-multi-user` — distinguish her from family
- `wintermute-undo` — verbal undo for last reversible action
- `wintermute-offline-persona` — richer behavior when API is down
  (cached news, music, time-telling, simple chat from a small local
  LLM — likely its own sub-vision)

## Open questions

- Should `wmd-init` (the supervisor in #2) reuse `pevent` or be its
  own minimal supervisor? Leaning reuse; `/build` can decide during
  #2 implementation.
- Naming for the eventual production deployment: user said "she will
  select a name for her laptop later" — production rename is a small
  later patch across configs.
- Hardware target — currently CPU-only Arch (mirrors this laptop).
  When a deployment laptop is picked, may want to add a GPU
  variant (Parakeet TDT for streaming STT, larger TTS).
- The "wake word for non-literate user" UX: should we eventually
  support always-on (no wake word) with diarization to filter only
  her voice? Fleet 3 question.

## Provenance

- **Seeded by:** user `/dream` invocation 2026-05-24, "I need you to
  help build an ai-laptop like yourself…"
- **Research:** Explore agent surveyed `~/wintermute/`, `~/.claude/`,
  `~/brain/journal/`, archived PRDs (no prior thinking found — clean
  slate). WebSearch for 2026 library landscape on STT/TTS/wake/AEC/
  desktop-automation/voice-assistant reference architectures.
- **Plan-agent critique** flagged: split orchestrator into
  dialog+brain, merge wake/VAD into audio, cut Moonshine, switch
  openWakeWord → microWakeWord, add the bootstrap PRD as the day-1
  unblocker, Sonnet not Opus as default.
- **User decisions:** codename `wintermute`, cloud-allowed audio
  fast-path, CPU-only baseline, all 7 PRDs `build_auto: true`.
