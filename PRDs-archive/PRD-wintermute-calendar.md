# PRD: wintermute-calendar — voice-driven CalDAV

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`,
  `PRD-wintermute-bootstrap.md` (account credentials)
build_target: rust-cli
build_priority: medium
deferred_acs: [1, 2, 3, 4, 5, 6, 7, 10]
deferred_ac_reasons:
  1: "Requires live CalDAV credentials and a running iCloud/Fastmail/Nextcloud server; cannot be simulated without a real account."
  2: "Requires a primed live CalDAV account with a real event on today's date; no offline substitute."
  3: "Requires a live CalDAV server with a weekly-recurring event to verify RRULE expansion end-to-end."
  4: "Requires live CalDAV write access and a verbal-confirm round-trip with the brain/dialog stack."
  5: "Requires a live CalDAV account with a dentist event in the 30-day lookahead to validate query filtering."
  6: "Requires live CalDAV delete capability and cross-verification via the web UI."
  7: "Requires a live CalDAV poll cycle and a real upcoming event to trigger the wm.cal.event.upcoming publish."
  10: "End-to-end voice round-trip requires jsy's live account, brain, dialog, and microphone — not automatable in CI."
mock_unjustified_for: [1, 2, 3, 4, 5, 6, 7, 10]
mock_justifications:
  1: "A mock CalDAV server (e.g. Baikal) would require a live network binding; minicaldav's HTTP transport cannot be fully mocked without an HTTP server fixture, which is out of scope for a v1 ship gate."
  2: "Event data is determined by the real calendar state on the day of testing; a static mock ICS cannot replicate the 'event on today's date' semantic reliably."
  3: "RRULE expansion is unit-tested (expand_weekly_rrule_count) in the offline suite; the missing piece is the live sync cycle, which requires real CalDAV."
  4: "The verbal-confirm flow runs through wm-dialog and the brain bus; mocking that stack would be a test of the mock, not of wm-cal's CalDAV write path."
  5: "Query filtering is tested offline via parse_ics_events_basic; the missing piece is live network retrieval of a matching event in a real calendar."
  6: "Delete verification requires the event to disappear from the live CalDAV server; a mock that always succeeds cannot validate the protocol delete path."
  7: "The reminder publish timing depends on a live poll cycle triggered by real CalDAV change timestamps; a mock clock does not exercise the real integration."
  10: "Full end-to-end voice round-trip is a system-level acceptance test that requires the complete wintermute stack running on jsy's live hardware."

---

## TL;DR

A daemon `wm-cal` that gives the brain a CalDAV-backed calendar
surface: list events, add, find, delete. Uses `minicaldav` for
CalDAV speak (Apache-2.0, dependency-light) plus the freedesktop
keyring for credentials. Add/delete go through verbal confirmation.
Bootstrap extends with a `/cal` setup page.

---

## 1. Why this exists

Vision §End-state #9 lists calendar alongside mail and music. For a
voice-first user, "what do I have today?" and "remind me to call my
sister Sunday at 2" are core daily affordances — and they belong on
a real calendar (iCloud, Google, Fastmail, Nextcloud) so the
caregiver can also see them, not in a local-only sqlite.

Concrete evidence from Phase 1:

- CalDAV is the only common-denominator open calendar protocol.
  All of iCloud / Google / Fastmail / Nextcloud / FastMail support
  it; Gmail-via-Google-account uses OAuth which is a Fleet-3 task.
- `minicaldav` crate (Apache-2.0) is small, sync-only, hits the
  90% of CalDAV. Async-wrap in `tokio::task::spawn_blocking`.
- ICS parsing via `ical` crate (MIT) — also small.

---

## 2. What this builds

### 2.1 Binary: `wm-cal`

Daemon. Holds a CalDAV principal + selected calendar; periodic
poll (5 min) for changed events; publishes `wm.cal.event.upcoming`
for the next 30 minutes.

### 2.2 Tools (topic `wm.cal.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `today` | `{}` | `{events:[{id, summary, start, end, location?}]}` |
| `range` | `{start, end}` | `{events}` |
| `add` | `{summary, start, end?, location?, description?}` | `{ok, id}` — destructive |
| `find` | `{query, lookahead_days?=30}` | `{events}` |
| `delete` | `{id}` | `{ok}` — destructive |
| `calendars` | `{}` | `{calendars}` |
| `set_calendar` | `{name_or_url}` | `{ok}` |

### 2.3 Time parsing

Brain provides ISO 8601 timestamps; jsy speaks natural language
("Sunday at 2"). Brain handles the language→ISO conversion using
its own context (current time, weekday) plus a small helper tool
`when {phrase}` that returns the parsed ISO — this gives the brain
a clean handoff.

### 2.4 Reminders

`wm-cal` publishes `wm.cal.event.upcoming` 5 minutes before each
event. Dialog/brain decide whether to speak ("in 5 minutes you have
'call dentist'"). Polite delay vs barge during conversation handled
by `wm-dialog` Fleet 1 already.

### 2.5 Credentials

Same pattern as `wm-mail`: `wm-bootstrap` `/cal` page collects
server URL, user, password (app-specific), writes to SecretService.

---

## 3. Risks

- **iCloud principal discovery** — sometimes the discovery URL is
  not the same as the server URL the user types. minicaldav can
  follow propfinds; document expected URLs per provider.
- **Recurring events** — minicaldav surfaces RRULE in raw form.
  Expand recurrences in-process up to lookahead window using `ical`
  crate's expander. Cap at 200 expanded events.
- **Timezone drift** — store and emit in IANA TZ format, never
  bare local. Brain says local naturally ("Sunday at 2 in the
  afternoon").
- **OAuth-only providers** — Google personal calendars require
  OAuth, not basic auth. Out of scope v1; document as a known
  limitation.

---

## 4. Sequencing

Independent of `wm-mail`. Can ship before or after. No new external
substrate. Reminders feed through dialog (Fleet 1, shipped).

---

## 5. Acceptance criteria

1. `wm-bootstrap` `/cal` page accepts an iCloud or Fastmail or
   Nextcloud account, daemon connects, lists at least 1 calendar
   on `wm-cal calendars`.
2. `wm-cal today` against a primed account with one event today
   returns that event with `summary`, `start`, `end` populated and
   timestamps in IANA TZ.
3. `wm-cal range {start,end}` over a 7-day window returns all
   events including expanded recurrences (verified against a
   weekly-recurring test event).
4. `wm-cal add` with `summary` + ISO `start` requires verbal
   confirm; on "yes add", the event appears in the next `today`
   listing AND in the iCloud/Nextcloud web UI within 30 s.
5. `wm-cal find {query:"dentist"}` over a 30-day lookahead returns
   at least 1 matching event.
6. `wm-cal delete` requires verbal confirm; on "yes delete", the
   event is gone from CalDAV (verified via web UI).
7. Reminder fire: an event 5 min out produces a single
   `wm.cal.event.upcoming` publish; if the event is 4 min out,
   no further duplicates.
8. Brain `when {phrase:"Sunday at 2"}` returns a valid ISO in local
   TZ resolving to the next Sunday at 14:00.
9. Credentials never logged or sent over agorabus in plaintext.
10. **[live]** Real round-trip: jsy says "add lunch with my sister
    Saturday at noon", brain calls `add` with confirm; "what do I
    have Saturday?" lists it. End-to-end <15 s per leg.
