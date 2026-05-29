# PRD: almanac-missed-to-kin — a missed dose reaches Joe, gently

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-almanac
Vision: visions/almanac.md

## TL;DR

A missed medication that only wintermute knows about helps no one. This
PRD makes a *missed* almanac entry — especially `category=med` — surface
to jsy through kin's family link: emit `wm.almanac.missed` unconditionally
(so almanac is useful even before kin ships), and bridge it to kin's
`wm.family.message` when that topic exists. A soft notice, not an alarm —
the medication-specific case of kin.md's "silence is surfaced, gently."

## Why this exists

- **almanac-acknowledge produces a `missed` signal with no consumer.** It
  emits `wm.almanac.ack {state:"missed"}`, but nothing carries that
  off-device. Without this PRD, the safety-critical case (she didn't take
  her heart pill) dies on the local bus.
- **kin already defines the link this should ride.** `visions/kin.md`
  end-state #4 verbatim: *"If she hasn't interacted … you get a single
  soft nudge — not an alarm."* kin establishes `wm.family.*` and an
  off-device delivery daemon. almanac's missed-medication notice is the
  most load-bearing instance of that nudge; it should reuse kin's channel,
  not build a second off-device path (scope boundary: kin owns delivery).
- **Decoupling keeps almanac shippable before kin.** Emitting a dedicated
  `wm.almanac.missed` envelope first, and bridging to kin second, means
  almanac is complete and testable even while kin is still in flight.

## What this builds

Extends `wintermute-almanac`:

- A small subscriber (folded into the tick-daemon process, or a sibling
  `wm-almanac watch` mode) that listens for `wm.almanac.ack {state:"missed"}`
  (from PRD-almanac-acknowledge) and:
  1. **Always** publishes a normalized `wm.almanac.missed {id, label,
     category, missed_ts}` envelope.
  2. **If** the kin family topic is configured/available, also publishes
     `wm.family.message` with a gentle, human-phrased body — e.g.
     `"Mom hasn't acknowledged her morning pills (due 8:00am)."` — tagged
     so kin's delivery daemon routes it to jsy. The exact `wm.family.*`
     schema is read from kin's published convention; if kin is not yet
     present, this step is skipped (logged, not errored).
- Severity shaping: `category=med` → bridged to kin immediately;
  `meal`/`activity`/`appointment` → `wm.almanac.missed` only by default
  (a per-entry `notify_on_miss: bool` can opt non-med entries into the kin
  bridge). This keeps jsy from being paged about a skipped walk.
- Degrade-out-loud: if the kin bridge is configured but the family topic
  publish fails, emit `wm.health.almanac` (companion-degrade discipline) so
  a silent delivery failure on a medication miss is impossible.

## Acceptance criteria

1. A `wm.almanac.ack {id, state:"missed", category:"med"}` causes publication of a `wm.almanac.missed` envelope with `id`, `label`, `category`, `missed_ts`.
2. When the kin family topic is configured, the same missed-med event also publishes a `wm.family.message` whose body names the entry and its due time in human language; assert via the publish-sink test double (no live bus).
3. With kin **not** configured/available, only `wm.almanac.missed` is published; the absence of the family topic is logged at INFO and does not error or panic.
4. A `category=meal` (or activity/appointment) miss with `notify_on_miss=false` (default) publishes `wm.almanac.missed` but does **not** publish `wm.family.message`; setting `notify_on_miss=true` on that entry opts it into the kin bridge.
5. If the kin bridge is configured and the `wm.family.message` publish fails, a `wm.health.almanac` diagnostic is emitted — no silent drop on a medication miss.
6. The `wm.family.*` field names/shape used match kin's published convention (cite the kin source/README at build time); a comment records which kin version the bridge was written against.
7. `cargo test` green, covering: med→both topics, non-med→missed-only, kin-absent→missed-only, publish-failure→health envelope.
