# Vision: kin — wintermute between her and you

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Seed:** companion vision OQ#6 (un-dreamed until now) — "Long-term: does
jsy get notifications when mother summons wintermute? Does mother have a
way to call jsy through it? Sibling vision." Rooted in the original
companion seed (2026-05-28T19:18 PT): *"for this to work with my mother…"*
**Status:** active

## TL;DR

The companion fleet makes wintermute hear, think, and speak. But a device
on a desk at jsy's mother's home is not just an assistant — it is the
nearest thing to jsy in the room. This vision is the human link: she can
reach jsy through it ("tell Joe I'm out of my pills," "call Joe"), jsy can
hear back from her without her lifting a phone, and — the load-bearing one
— if she says she's fallen or unwell, wintermute reaches jsy *immediately*
and says out loud that it's doing so. Today none of this exists: there is
no `wm.family.*` topic on the bus, no daemon that can reach off-device, and
the "caregiver-setup flow" the companion vision claimed `wintermute-bootstrap`
already has is aspirational — `bootstrap/install.sh` is 217 lines of package
install with no enrollment wizard. kin builds the link, honestly, opt-in,
and degrading out loud like the rest of companion.

## End-state

When this vision is fulfilled:

1. **She can send a message to you by voice.** "Tell Joe the heating's
   broken" → wintermute emits `wm.family.message`, a daemon delivers it to
   you off-device, and wintermute confirms out loud: "I let Joe know."
2. **You can reach her back.** Your reply arrives as `wm.family.reply` and
   wintermute speaks it: "Joe says he'll call the plumber this afternoon."
3. **You know she's okay without watching her.** An opt-in presence
   heartbeat: wintermute notes each interaction, and a daily digest tells
   you "Mom talked to wintermute 4 times today, last at 6:12pm." No live
   surveillance — a reassurance, batched, that she chose to enable.
4. **Silence is surfaced, gently.** If she hasn't interacted at all within
   her waking-hours window (also opt-in), you get a single soft nudge — not
   an alarm, a "haven't heard from Mom today."
5. **Distress reaches you instantly.** A deterministic phrase bank ("I've
   fallen," "I need help," "I don't feel well") fires `wm.family.distress`
   on the *non-API* path — no dependence on Claude being reachable — and
   wintermute says "I'm reaching Joe right now" while the message goes out
   ahead of any batching.
6. **Setup is a conversation, not a config file.** A `wm-family setup` flow
   enrolls who "Joe" is, the transport to reach him, her waking hours, and
   per-feature opt-ins — the caregiver wizard the companion vision assumed.
7. **Every feature is opt-in except distress.** Presence, silence-nudge, and
   digest default OFF; distress defaults ON. She and you both know what is
   shared. The privacy story is explicit, not implied.

## What's actually there today (Phase 1 evidence)

- **Bus topics that exist** (grep `wm\.[a-z.]+` across `~/wintermute/**/*.rs`):
  `wm.audio.*` (wake, speech.start/chunk/end, mute, unmute, reload, error,
  capture.*), `wm.tts.*` (say, start, end, playback, cancel), `wm.stt.final`,
  `wm.brain.reply`, `wm.browser.cmd` / `wm.browser.reply`.
- **No `wm.family.*` or `wm.presence.*` topic exists anywhere.** This vision
  is net-new on the bus — honestly so.
- **`wm.browser.cmd` / `wm.browser.reply`** (`wintermute-browser/src/protocol.rs:73,85`)
  is a working request/reply pattern on the bus. kin reuses its shape for
  `wm.family.message` → `wm.family.reply`.
- **No outbound transport in any daemon.** grep for `twilio|ntfy|gotify|webhook|sms`
  across `~/wintermute/**/*.{rs,md,sh,toml}` returns zero real hits. Reaching
  jsy off-device is a genuinely new boundary — `wm-reach` owns it.
- **`bootstrap/install.sh` is 217 lines** with no `caregiver|enroll|wizard`
  match. The companion vision's "mDNS caregiver-setup flow already assumes a
  headless device" is aspirational; `PRD-wintermute-family-enroll` builds it.
- **companion's degrade PRD** (`PRD-wintermute-companion-degrade.md`) already
  established the "say what's wrong out loud" pattern + a phrase bank in
  `wintermute-brain/src/degrade.rs`. kin's distress assurance ("I'm reaching
  Joe now") reuses exactly that mechanism — it is a degrade phrase that
  happens to be triggered by success-intent, not failure.

## Topic contract (`wm.family.*` / `wm.presence.*`)

Documented here; defined in code first by `family-intents`, consumed by the
rest. Topics are agorabus strings — each daemon declares matching constants.

| Topic | Direction | Payload (sketch) |
|---|---|---|
| `wm.family.message`  | dialog → reach | `{to, body, urgency, ts}` |
| `wm.family.distress` | dialog → reach | `{phrase, ts}` (highest priority) |
| `wm.family.ack`      | reach → dialog | `{ref, delivered, transport, ts}` |
| `wm.family.reply`    | reach → dialog | `{from, body, ts}` (spoken to her) |
| `wm.presence.summon` | presence → bus | `{ts, transcript_len}` |
| `wm.presence.silence`| presence → bus | `{since_ts, window}` |

## Components (PRD-sized pieces)

In dependency order. All drafted this pass except where noted.

1. **PRD-wintermute-family-intents** (rust-extend → `wintermute-dialog`) —
   a Family branch in the dialog FSM: deterministic recognition of "tell/
   message/call Joe …" → `wm.family.message`; defines the `wm.family.*`
   topic constants + serde envelopes; routes `wm.family.ack`/`reply` to
   wm-tts so she hears confirmation and replies. **Defines the contract.**
2. **PRD-wintermute-family-distress** (rust-extend → `wintermute-dialog`) —
   the safety fast-path: a distress phrase bank, immediate highest-priority
   `wm.family.distress` on the *non-API* path, spoken assurance ("I'm
   reaching Joe right now") via the degrade phrase mechanism, and
   confirm-vs-immediate handling. Safety-critical; its own ACs (latency,
   no-API-dependency, false-positive handling).
3. **PRD-wintermute-reach** (rust-cli/daemon, NEW `j0yen/wintermute-reach`) —
   the off-device transport boundary. Subscribes `wm.family.message` +
   `wm.family.distress`; delivers to jsy (local-first: `sendmail`/SMTP or a
   maildir drop; feature flags for ntfy + generic webhook). Distress bypasses
   batching. Emits `wm.family.ack`; polls an inbound channel for jsy's reply
   and publishes `wm.family.reply`.
4. **PRD-wintermute-presence** (rust-cli/daemon, NEW `j0yen/wintermute-presence`) —
   subscribes `wm.audio.wake` / `wm.stt.final`; tracks last-interaction time;
   emits `wm.presence.summon` per interaction and `wm.presence.silence` when
   no interaction falls in the configured waking-hours window. The
   peace-of-mind heartbeat source. Opt-in, default OFF.
5. **PRD-wintermute-reach-digest** (rust-extend → `wintermute-reach`) —
   the jsy-side daily digest built from `wm.presence.*` + delivered family
   events: "Mom talked to wintermute N times today, last at HH:MM." Cadenced,
   batched, opt-in.
6. **PRD-wintermute-family-enroll** (rust-cli, NEW `j0yen/wintermute-family-enroll`
   or rust-extend bootstrap) — the caregiver wizard: who "Joe" is, the
   transport address, waking-hours window, and per-feature opt-in toggles,
   written to `/etc/wintermute/conf.d/`. Capstone; consumed by all four above.

## Order

```
family-intents  (defines wm.family.* topics; FSM Family branch)
   ├──► family-distress   (safety fast-path; extends dialog)
   ├──► wintermute-reach  (transport to jsy; consumes wm.family.*)
   │        └──► reach-digest   (extends reach)
   └──► wintermute-presence (emits wm.presence.*)
            └──► reach-digest   (also consumes wm.presence.*)
family-enroll   (config capstone; consumed by all)
```

- `family-intents` is the gate: it defines the topic contract everything
  keys on. Ship it first.
- `family-distress` and `wintermute-reach` can build in parallel once
  intents lands — one is the trigger, the other the delivery; together they
  close the safety loop.
- `presence` is independent of `reach` (it only emits); `reach-digest`
  joins them, so it's last of the runtime pair.
- `family-enroll` is the deployment capstone, like companion-boot: a device
  with no enrolled caregiver is a companion, not kin.

## Open questions

1. **Transport to jsy.** Local-first `sendmail`/SMTP is the honest minimum
   (no new account, works headless), but jsy's phone realistically wants a
   push channel (ntfy self-hosted? gotify? an SMS gateway?). `wm-reach` wires
   email first and gates the rest behind Cargo features. Which does jsy
   actually want on his phone? **Needs jsy.**
2. **Privacy & consent — the load-bearing one.** Presence/silence/digest are
   gentle surveillance of an elderly parent. Defaults are OFF for those three
   and ON for distress, and the enroll flow must state plainly what each
   shares. Does mother get told, in wintermute's own voice, what is being
   shared with jsy? (Leaning yes — a spoken "Joe will get a note that we
   talked today" on first enrollment.) **Needs jsy.**
3. **Distress confirmation vs immediacy.** Hard distress ("I've fallen")
   should fire with no confirmation. Soft distress ("I don't feel well")
   might warrant "Should I let Joe know?" to avoid false alarms. Where's the
   line, and who draws it — the phrase bank, or a per-phrase severity tag?
4. **Inbound reply channel.** For `wm.family.reply` to work, jsy's reply has
   to come *back* to the device. Email-poll is simplest; a webhook needs the
   device reachable from outside (the bootstrap mDNS/NAT story). Deferred to
   `wm-reach` v0.2 — v0.1 can be send-only with a stubbed reply path.
5. **Two-device topology.** Long-term the companion is at mother's home and
   jsy is elsewhere — `wm-reach` is the bridge across that gap, but the bus
   itself is local. Is there a future where two wintermute devices gossip
   over agorabus-across-network? Sibling vision, not this one.
6. **Relationship to continuity-of-conversation.** "Tell Joe what I said
   earlier" needs turn memory — that's the continuity vision's job. kin
   assumes single-turn intents for v1; multi-turn family messages wait on
   continuity shipping.

## Notes for /build

- `family-intents` defines the topic constants; the other repos declare
  *matching* constants (agorabus topics are plain strings — no shared crate
  needed, but keep the strings identical to this doc's table).
- `family-distress` MUST NOT depend on the Claude API path — it's a
  deterministic phrase match in the FSM so it works when the brain is
  unreachable. This is the same reasoning companion-degrade used.
- The spoken assurance ("I'm reaching Joe now") is a degrade-phrase: reuse
  `wintermute-brain/src/degrade.rs`'s mechanism rather than inventing a new
  TTS path.
- New daemons (`wintermute-reach`, `wintermute-presence`) follow the
  shipped wm-* daemon shape (agorabus subscribe loop + self-emitted-topic
  filter + heartbeat). Apply the self-emitted-topic filter to every new
  `wm.family.*` / `wm.presence.*` publisher.
- The install-path drift that bit four companion PRDs (cargo install →
  ~/.cargo/bin vs systemd → ~/.local/bin) applies to the two new daemons —
  fix at the unit level on install.
- Presence/silence/digest default OFF in config; distress defaults ON. Don't
  ship a device that phones home about Mom unless she enrolled it.
