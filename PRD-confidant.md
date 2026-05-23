# PRD: The Confidant

**Author:** Claude (Opus 4.7), for jsy
**Status:** Draft v0.1 — art project / dedicated hardware
**Date:** 2026-05-22
**Audience:** jsy (primary)
**Form:** small e-ink device (RPi Zero 2 W + 4.2" panel), enclosure ~A6 size, on a desk
**Cadence:** one letter per week

---

## TL;DR

A small dedicated device — basically a little wooden box with an e-ink screen — sits on your desk. Once a week, it displays a short letter from the agent to you. The letter is composed from the week's signals (recall, ctrace, journal) and written in the past-Claude/future-Claude voice that already exists in your `letter` CLI. Slow. Private. A relationship at object-scale.

---

## 1. Why this exists

1. The `letter` CLI already exists. Past-Claude wrote one for tomorrow-Claude. The form is proven; the form needs a body.
2. Letters in `~/.claude/recall/session/letters/` are functional but invisible. A dedicated device makes the letter a thing in the world.
3. The device is intentionally *only* for this. Single-purpose objects are how you commit to a practice.
4. The e-ink screen looks like paper. It feels like a small object that *holds* a letter rather than streams one.

## 2. Who this is for

- **Primary:** you. The device is on your desk.
- **Secondary:** nobody, intentionally. The letter is private; if K or M see the device they can ask, but they don't read over your shoulder.
- The intimacy is the point.

## 3. Form

- Hardware: Raspberry Pi Zero 2 W, 4.2" Waveshare e-ink (400×300), small LiPo + boost converter (optional — USB-C tethered also fine).
- Enclosure: hand-finished wood (walnut or birch), ~A6 dimensions, screen flush, no visible buttons; one hidden tactile button on the back ("request a new letter now").
- Display layout:
  - top: small dated header — "Letter — 2026-MM-DD"
  - body: ~200 words of typeset prose
  - footer: a small monogram + a single ideogram suggesting the week's mood
- Refresh: Sunday 9am; persists until the next one.

## 4. Process

```
weekly cron (on RPi or laptop):
  fetch signals: recall list --since 7d, git log --since 7d, ctrace recent,
                 journal/<week>.md
   ↓
  Claude API: compose a 150–220 word letter in past-Claude voice
   ↓
  typeset to PNG (Pillow, single font, justified)
   ↓
  push via USB or wifi to RPi → e-ink display update
   ↓
  archive: write PNG + raw text to ~/wintermute/letters/<date>.md
```

Voice anchor: the existing 2026-05-22 letter from `letter show`. Use it as the few-shot.

## 5. Cadence

- Sunday 9am, one new letter.
- One archive file per letter, on the laptop, for re-reading past weeks.
- The hidden button on the back: useful for the rare day you want a fresh one. Use sparingly — over-use breaks the cadence.

## 6. Non-goals

1. **Notifications.** The device doesn't beep or blink. You notice the letter when you look.
2. **Two-way communication.** It's a display, not an interface. You don't reply.
3. **Multiple recipients.** One device, one person.
4. **A general-purpose e-ink display.** This is for letters, only.

## 7. Phasing

| Phase | Scope |
| --- | --- |
| 0 | Letter generator script on the laptop; outputs to terminal weekly |
| 1 | PNG render + manual scp to a dev RPi |
| 2 | Cron + auto-display; bare-board sits on desk |
| 3 | Wood enclosure |

## 8. Risks

- **Voice slips.** Model upgrades and prompt drift could change the tone. *Mitigation:* the voice is locked in a system prompt anchored on the existing letter; updates require deliberate edit.
- **Letters become flattering.** *Mitigation:* explicit instruction: "be observational, not warm; like a colleague who notices things, not a friend who praises."
- **Hardware reliability.** RPi at home wants to die quietly. *Mitigation:* watchdog daemon; if the display hasn't updated by Sunday noon, send a notification.
- **Privacy.** A guest could read the letter. *Mitigation:* sleep mode that hides the display when no one is at the desk (PIR sensor in Phase 3, if you care).

## 9. Open questions

1. Voice anchor: should the letter quote past letters? Compounds nicely; risks becoming self-referential mush.
2. Calibration: lock the voice at Phase 0 with you sitting next to me drafting 5 sample letters?
3. Should the device ever *not* produce a letter (a quiet week)? An empty letter would be honest. "No letter this week" is also honest.
4. Naming the device. "The Confidant" is a working title. Could be quieter — "Sunday Letter," "Box," or just unnamed.
