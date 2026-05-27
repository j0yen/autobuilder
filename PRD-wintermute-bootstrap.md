# PRD: wintermute-bootstrap — first-boot caregiver setup

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** nothing (this is the entry gate for the whole vision)
**Sibling PRDs:** `PRD-wintermute-platform.md` (autostarts what this configures)
build_auto: true
build_target: rust-cli
build_priority: high

---

## TL;DR

A laptop for a computer-illiterate user cannot ask her to type her
Wi-Fi password, paste an API key, or pick a microphone from a dropdown
on day 1. Someone else — a caregiver, family member, or installer —
has to do it once, from a phone, before voice control is meaningful.
`wm-bootstrap` is the smallest possible Rust web server that runs on
first boot, announces itself over mDNS as `wintermute.local`, and
walks the helper through a five-minute setup form. On submit it
writes the config and hands control over to `wintermute.target`.
Subsequent boots skip bootstrap entirely.

This is the day-1 unblocker for the whole vision. Without it, Fleet 1
ships but cannot be turned on.

---

## 1. Why this exists

Three observations from the design discussion:

1. **Voice cannot bootstrap voice.** STT needs a working microphone
   choice; TTS needs a voice selection; the brain needs an API key;
   the wake word needs to be picked from the pretrained set.
   *None of those decisions can be made by voice* because nothing
   that listens to voice is running yet.

2. **A non-literate user cannot self-configure.** She isn't typing the
   API key. She isn't choosing a Wi-Fi network from a list of SSIDs.
   The setup belongs to whoever brought her the laptop.

3. **The caregiver isn't necessarily near the laptop.** They might
   sit her at the desk and configure from their phone in the kitchen.
   mDNS + a captive form on `wintermute.local` is the universal-easy
   shape; everyone with a phone knows how to open a URL.

This PRD is **first** in the fleet for a reason: every other PRD in
Fleet 1 assumes `/etc/wintermute/conf.d/00-bootstrap.env` exists with
valid values.

---

## 2. What this builds

### 2.1 Binary: `wm-bootstrap`

A small Rust HTTP server using **axum** + **tokio**. On startup:

- Check for `/etc/wintermute/conf.d/00-bootstrap.env`. If present and
  non-empty, exit 0 immediately (bootstrap was already done).
- If absent: bind to `0.0.0.0:80` (privileged port — install with
  `setcap cap_net_bind_service=+ep`) and serve the setup pages.
- Use **mdns-sd** (Rust crate) to announce `_http._tcp.local.` with
  service name `wintermute` so the caregiver's phone can resolve
  `http://wintermute.local/`.
- On successful submit: write the env file via `txn-edit`-style atomic
  rename, optionally write a NetworkManager connection, render a
  "we're done — say hello to her" page, then `systemctl --user start
  wintermute.target` and exit 0 after 30 seconds (gives the page time
  to load).

### 2.2 The setup form (one page, scrollable)

| Field | Required | Notes |
|---|---|---|
| **Her name** | yes | Used in the greeting and as part of the system prompt |
| **Wi-Fi SSID** | no | Skipped if already on a network; otherwise scan available with `nmcli dev wifi list` |
| **Wi-Fi password** | conditional | Only if SSID chosen |
| **Anthropic API key** | yes | Validated by a live ping (see 2.3) |
| **Microphone** | yes | Dropdown auto-populated from PipeWire (`pactl list short sources`) with a "Test" button that records 2 seconds and plays it back |
| **Speaker** | yes | Same shape; "Test" plays a short Piper sample |
| **TTS voice** | yes | Dropdown of installed Piper voices with audio sample on click; ElevenLabs voices listed if cloud-quality enabled |
| **Wake word** | yes | Radio buttons: "Hey Jarvis" / "Okay Nabu" / "Hey Mycroft" (microWakeWord pretrained set) |
| **Time zone** | yes | Auto-detected via geolocation if browser allows; otherwise dropdown |
| **Emergency contact** | no | Name + phone or email; consumed by `wintermute-emergency` in Fleet 3 |
| **Cloud audio fast-path** | optional | Checkbox: "Use cloud STT when network is OK" (default off) |
| **Cloud audio quality path** | optional | Checkbox: "Use ElevenLabs for natural voice" (default off; reveals voice picker) |

### 2.3 API key validation

Before saving, POST a one-token request to the Claude API to confirm
the key is real and not rate-limited. On failure, render a friendly
error: "That key didn't work. Double-check it and try again." Never
write a key that didn't validate.

### 2.4 Config file shape

`/etc/wintermute/conf.d/00-bootstrap.env`:

```
WM_USER_NAME=Mary
WM_ANTHROPIC_API_KEY=sk-ant-...
WM_MIC_NODE=alsa_input.usb-Logitech_StreamCam-02.analog-stereo
WM_SINK_NODE=alsa_output.pci-0000_00_1f.3.analog-stereo
WM_TTS_VOICE=en_US-lessac-medium
WM_WAKE_WORD=hey_jarvis
WM_TIMEZONE=America/Los_Angeles
WM_EMERGENCY_NAME=
WM_EMERGENCY_CONTACT=
WM_CLOUD_STT_FASTPATH=false
WM_CLOUD_TTS_QUALITY=false
```

All Fleet 1 daemons read this file at startup. No daemon reads it
during runtime; reconfig requires `wm-bootstrap --reconfigure`
followed by `systemctl --user restart wintermute.target`.

### 2.5 Reconfigure flow

`wm-bootstrap --reconfigure` bypasses the "already configured" check
and re-opens the form pre-populated from the existing env file.
On submit, atomically replaces the env file and (if the user opts in
on the page) restarts `wintermute.target`. This is the supported way
to change Wi-Fi, voice, or wake word later without editing config
files by hand.

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| `axum` | ^0.7 | HTTP server | MIT |
| `tokio` | ^1.40 | async runtime | MIT |
| `mdns-sd` | ^0.11 | mDNS announcement | Apache-2.0/MIT |
| `nmcli` (system tool) | any | Wi-Fi list + connect | GPL |
| `pactl` / `wpctl` (system tool) | any | PipeWire device enumeration | LGPL |
| `piper` (system binary) | any | TTS voice samples | MIT |
| `reqwest` | ^0.12 | API-key validation ping | MIT/Apache-2.0 |
| `serde` + `serde_json` | ^1 | form parsing | MIT/Apache-2.0 |

---

## 4. Acceptance criteria

1. After a fresh install with no `00-bootstrap.env`, running
   `wm-bootstrap` makes `http://wintermute.local/` reachable from a
   second device on the same LAN within 10 seconds.
2. A caregiver can complete the setup form from a phone browser in
   under 5 minutes (timed walkthrough on this laptop).
3. Submitting an invalid Anthropic API key surfaces a clear error
   and does not write the env file.
4. Submitting valid input writes `00-bootstrap.env` atomically and
   calls `systemctl --user start wintermute.target` exactly once.
5. Re-running `wm-bootstrap` without `--reconfigure` exits 0 in
   under 100 ms (idempotent skip).
6. `wm-bootstrap --reconfigure` opens the form pre-populated from
   the current env file.
7. No sensitive value (API key, Wi-Fi password) is logged.

## 5. Out of scope (Fleet 2 / 3)

- Voice-driven reconfigure ("change my wake word") — Fleet 2.
- Multi-user setup (more than one resident) — Fleet 3.
- Remote setup from off-LAN — never; this is local-network only by
  design.
- TLS on `wintermute.local` — out of scope; LAN-only, plaintext is
  acceptable for a 5-minute one-time setup.

## 6. Risks

- **Privileged port 80** — needs setcap. Alternative: bind to 8080
  and document `http://wintermute.local:8080/` in the install
  instructions. Slightly worse UX but avoids the cap step.
- **mDNS sometimes broken** on phone-side networks (especially iOS
  on captive Wi-Fi). Document the IP-address fallback in the setup
  card that ships with the laptop.
- **API key in env file** — this is a personal-laptop tradeoff; if
  the user wants stronger isolation, a follow-up PRD can move it
  into `gnome-keyring` or `pass`.

## 7. Open questions

- Should `wm-bootstrap` also flash an LED pattern or play a chime
  while waiting for the caregiver to open the URL? Probably yes
  ("the laptop says boop-boop-boop, please open `wintermute.local`
  on your phone") — but that's a polish iteration after first ship.
- Should we support a QR code shown on screen if a screen is
  attached? Yes for the "I'm at the desk with a screen but no
  network admin tools" case. Add in iter-2.
