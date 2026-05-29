# PRD: wintermute-reach — the off-device transport to jsy

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-cli
**build_version_bump:** n/a (new repo j0yen/wintermute-reach)
**Depends on:** PRD-wintermute-family-intents
**Codename:** *courier* — carries her words off the device and brings yours back.

## TL;DR

Everything else in kin happens on the bus, which is local. This daemon is
the one boundary that crosses off-device: it subscribes to `wm.family.message`
and `wm.family.distress`, delivers them to jsy through a real transport
(email first; ntfy / generic webhook behind Cargo features), acks delivery
back onto the bus so the dialog can say "I let Joe know," and — when an
inbound reply channel is configured — publishes jsy's reply as
`wm.family.reply` for wintermute to speak. Distress bypasses any batching
and goes out ahead of everything.

## 1. Why this exists

- **kin vision Component 3; the transport boundary.** No daemon in
  `~/wintermute` can reach off-device today — Phase 1 grep for
  `twilio|ntfy|gotify|webhook|sms` across `**/*.{rs,md,sh,toml}` returned
  zero real hits. This is the new capability, isolated to one daemon so the
  transport choice is swappable without touching the dialog FSM.
- **The dialog FSM already expects an ack.** `PRD-wintermute-family-intents`
  enters `FamilyPending` and waits for `wm.family.ack`; without this daemon,
  every family message times out into "I couldn't reach Joe." reach closes
  that loop.
- **Distress needs a privileged path.** `PRD-wintermute-family-distress`
  publishes `wm.family.distress` as its own topic precisely so a delivery
  daemon can treat it as bypass-batching, highest-priority. reach honors that.

## 2. What this builds

A new repo `j0yen/wintermute-reach`, a long-running agorabus daemon.

### 2.1 Subscribe + dispatch

- Subscribes `wm.family.message` and `wm.family.distress` (multi-prefix —
  reuse the multi-prefix-subscribe fix from the companion fleet).
- `wm.family.distress` → deliver immediately, synchronously, ahead of any
  queued normal messages.
- `wm.family.message { urgency }` → deliver (urgency may inform batching in a
  later version; v1 delivers promptly regardless).
- Applies the wm-* self-emitted-topic filter to its own `wm.family.ack` /
  `wm.family.reply` publishes.

### 2.2 Transport (`src/transport/`)

A `Transport` trait with selectable backends, configured via
`/etc/wintermute/conf.d/` (see family-enroll):

- `email` (default, always compiled): hand off to system `sendmail` or an
  SMTP submission config. No new account if the box has an MTA; otherwise a
  documented SMTP env block. This is the honest headless minimum.
- `ntfy` (Cargo feature): POST to an ntfy topic URL — best phone-push story.
- `webhook` (Cargo feature): POST JSON to a generic URL.

Each backend returns a `Delivered { transport, ref }` or an error; the daemon
maps that to `wm.family.ack { ref, delivered, transport, ts }`.

### 2.3 Inbound reply (v1 stub, v2 real)

- v1: a stubbed reply path — a local `wm-reach reply "<text>"` CLI subcommand
  that publishes `wm.family.reply` (lets the whole loop be tested end-to-end
  without an external inbound channel).
- v2 (deferred, noted in vision OQ#4): poll an email folder / accept a webhook
  so jsy's actual reply flows back automatically.

### 2.4 CLI surface

- `wm-reach daemon` — run the subscribe loop (systemd `wm-reach.service`).
- `wm-reach send --to joe --body "…"` — manual one-shot delivery (testing).
- `wm-reach reply "<text>"` — publish a `wm.family.reply` (v1 inbound stub).
- `wm-reach test-transport` — dry-run the configured backend, print result.

## 3. Acceptance criteria

1. `wm-reach --help` lists `daemon`, `send`, `reply`, `test-transport`.
2. The `Transport` trait has an `email` impl that, with `WM_REACH_SENDMAIL`
   pointed at a capture script, produces a message containing the family body
   (integration test using a fake sendmail).
3. A published `wm.family.message { to: "joe", body: "heating broken" }`
   results in one transport delivery and one `wm.family.ack { delivered: true,
   transport: "email" }` on the bus (bus integration test).
4. A published `wm.family.distress` is delivered ahead of a `wm.family.message`
   that was queued first (ordering/priority test).
5. A transport error yields `wm.family.ack { delivered: false }` (not a panic,
   not a silent drop) — the dialog must be able to speak the failure.
6. `wm-reach reply "Joe says hi"` publishes a `wm.family.reply { from, body }`
   that a bus subscriber receives (round-trip test).
7. ntfy and webhook backends compile behind their Cargo features and are
   excluded from the default build (feature-gating test / `cargo build`
   without features omits them).
8. The daemon applies the self-emitted-topic filter (does not re-consume its
   own `wm.family.ack`/`reply`).
9. systemd unit `wm-reach.service` installs pointing at the same bin path the
   install step uses (no cargo-bin-vs-local-bin drift — the companion-fleet
   regression).
10. No secret (SMTP password, ntfy token) is logged; config is read from
    `/etc/wintermute/conf.d/`, never hard-coded (grep-clean test).
11. `cargo test` green; `cargo clippy` clean; release gate receipts produced
    per autobuilder.
