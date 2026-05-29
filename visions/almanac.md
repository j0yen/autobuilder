# Vision: almanac — the rhythm of her day, kept out loud

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** Manual `/dream` (no topic) during the live companion push. The
fleet can now *hear* her (companion), speak *warmly* (hearth), *wait* for
her and speak legibly (earshot), and *link* her to jsy (kin). The whole
loop is **reactive** — it does nothing until she summons it. But the load
an elder actually needs carried is the opposite: the things that happen
*on time, every day*, that she may forget. Pills. Meals. The visiting
nurse at 2. A walk before dark. almanac is wintermute keeping the shape of
her day and speaking the right gentle prompt at the right moment — without
being asked.

## TL;DR

Today wintermute has **no clock-driven proactive speech**. The only
proactive turn in the whole fleet is `recap_opener`
(`wintermute-brain/src/daemon.rs:1352`), which fires once at session start
if a recent thread exists. There is no "at 8am, say the blue-pill prompt."

`wm-cal` (`wintermute-calendar`) is *not* this: it is a **CalDAV** daemon
for **jsy's** appointments — it needs online credentials in SecretService
(`creds.rs:16`), expands RRULEs (`caldav.rs:397`), and exists so "the
caregiver must also see jsy's appointments" (intent-card.json:17). It is
caregiver-facing, network-dependent, and shaped for shared online
calendars. Wrong tool for "your blue pill, every morning, spoken to Mom on
a desk that may have no internet." almanac is **local, recurring,
opt-in, and spoken** — and it *consumes* `wm.cal.event.upcoming` when an
appointment also needs a spoken nudge, rather than reimplementing CalDAV.

The prompts are spoken in **hearth's** persona, paced for **earshot's**
patience, and a missed medication surfaces to **kin**. almanac is the
fifth panel of the companion: the one that carries time so she doesn't
have to.

## End-state

When this vision is fulfilled:

1. **jsy enrolls her routine once.** `wm-almanac add --label "morning pills"
   --at 08:00 --every day --say "Good morning. It's time for your blue
   pill — the one for your heart."` writes a durable local entry. No
   CalDAV, no account, works offline.
2. **At 08:00 local, wintermute speaks it** — unprompted, in hearth's
   voice, at earshot's pace, through the speaker she already hears.
3. **She answers and is heard.** "I took it" / "okay" → done. "In a
   minute" / "later" → snoozed, re-due in N minutes. Silence past
   earshot's patience window → marked *missed*, gently re-asked once.
4. **A missed medication reaches jsy.** `wm.almanac.missed` flows to kin's
   `wm.family.*` link; jsy gets a soft notice — "Mom hasn't acknowledged
   her 8am medication" — not an alarm, a fact he chose to receive.
5. **The day's rhythm is hers, editable.** Entries can be daily, weekly,
   one-off; categorized (med / meal / appointment / activity); each
   independently opt-in. Setting the store empty reproduces today's
   purely-reactive behavior exactly.
6. **It degrades out loud, like the rest of companion.** If the clock is
   wrong, the store unreadable, or wm-tts down, almanac says so / logs a
   `wm.health.*` rather than silently skipping a dose prompt.

## Components (PRD-sized pieces)

Drafted this pass, in dependency order:

1. **PRD-almanac-schedule-store** — new crate `wintermute-almanac`
   (`~/wintermute/wintermute-almanac`). The durable local model + `wm-almanac`
   CLI: add / list / remove / enable-disable recurring entries
   `{id, label, say, recurrence (daily|weekly|once), local_time, tz,
   category, opt_in, snooze_min}` in a TOML/JSON store under
   `$XDG_DATA_HOME/wm-almanac/`. No bus, no speech yet — just the model the
   rest hangs on. Mirrors `wm-cal`'s state-file convention
   (`events.rs:103` `wm-cal/state.json`).
2. **PRD-almanac-tick-daemon** — extends `wintermute-almanac`. A daemon
   (and a systemd-timer oneshot fallback) that, in the entry's IANA
   timezone (chrono-tz 0.9, already a fleet dep), computes the next due
   entry and at due time publishes `wm.almanac.due {id, label, say,
   category}` to agorabus. Mirrors `wm-cal`'s `run_daemon` poll-and-publish
   (`caldav.rs:292`) but for *local recurrence*, not CalDAV polling.
3. **PRD-almanac-speak-bridge** — extends `wintermute-brain`. wmd
   subscribes to `wm.almanac.due` and speaks the prompt by reusing the
   *exact* proactive path `recap_opener` uses: build a `ReplyEvent{text,ts}`
   and `publish(outgoing::REPLY, …)` (`daemon.rs:1352-1377`), so wm-tts
   already plays it, in hearth's persona, at earshot's pace. One new
   handler, one existing publish path — no second proactive mechanism.
4. **PRD-almanac-acknowledge** — extends `wintermute-brain`. After a due
   prompt, correlate the next `wm.stt.final` as an acknowledgment:
   classify done / snooze / missed; on snooze re-arm at `+snooze_min`; on
   silence past earshot's window emit `wm.almanac.ack {id, state}` and one
   gentle re-ask. Closes the loop the prompt opens.
5. **PRD-almanac-missed-to-kin** — extends `wintermute-almanac`. When an
   entry (esp. `category=med`) goes *missed*, emit `wm.almanac.missed`
   unconditionally (so almanac is useful before kin ships), and bridge it
   to kin's `wm.family.message` when that topic exists. Realizes kin.md
   end-state #4 (gentle silence surfaced) for the medication-specific case.

## Order

```
schedule-store (foundation: model + CLI)
      │
      ▼
tick-daemon (publishes wm.almanac.due) ──────┐
      │                                       │
      ▼                                       ▼
speak-bridge (wm.almanac.due → spoken)   missed-to-kin (wm.almanac.missed)
      │                                  (independent of speak; needs store+tick)
      ▼
acknowledge (next wm.stt.final → done/snooze/missed)
      │
      └── feeds missed state back to missed-to-kin
```

- **schedule-store** ships alone, immediately useful as a CLI.
- **tick-daemon** needs the store.
- **speak-bridge** and **missed-to-kin** both need tick-daemon; they touch
  *different* crates (brain vs almanac) so they parallelize.
- **acknowledge** needs speak-bridge (a prompt must be spoken before it can
  be acknowledged) and produces the *missed* signal missed-to-kin consumes.

## Scope boundaries (do not merge)

- **almanac owns the CLOCK-driven proactive turn.** hearth owns the
  *words/persona*; earshot owns *tempo/patience*; almanac owns *when*.
  almanac builds **no** persona string and **no** timing constants — it
  reuses hearth's persona (brain system prompt) and earshot's timing config
  by emitting through brain's existing reply path. If almanac is tempted to
  add a phrase bank or a timeout const, that belongs in hearth/earshot.
- **almanac is NOT wm-cal.** almanac never speaks CalDAV. If an appointment
  on a shared calendar also needs a spoken nudge, a *later* PRD has almanac
  *subscribe* to `wm.cal.event.upcoming` and create a transient due entry —
  it does not duplicate CalDAV fetch/auth.
- **kin owns off-device delivery.** missed-to-kin emits `wm.almanac.missed`
  and (when present) hands to `wm.family.message`; it does not itself reach
  off-device.

## Open questions (for the user / next pass)

- **Caregiver-side editing UX.** `wm-almanac add` is the v1 interface (jsy,
  by hand/voice). Whether mom's routine is editable *remotely* through kin
  or through the onramp/homestead enrollment wizard is a kin/onramp concern,
  not almanac's — left as a bullet.
- **Quiet hours.** Should almanac suppress non-med prompts overnight?
  Probably a per-entry `active_hours`, but defaulting it risks silently
  skipping a real prompt. Deferred — v1 entries fire whenever scheduled.
- **Learned timing.** Could almanac shift a prompt earlier/later from
  observed acknowledgment latency? Same "learned vs static" question
  earshot deferred. Static `local_time` for v1.
- **Snooze ceiling.** How many snoozes before a snooze *is* a miss? v1: a
  per-entry `max_snoozes` (default 2), then treat as missed.
