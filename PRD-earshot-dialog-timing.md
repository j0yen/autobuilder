# PRD: earshot-dialog-timing — conversation tempo a caregiver can tune

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-dialog
Vision: visions/earshot.md

## TL;DR

`wintermute-dialog`'s conversational deadlines are compile-time `const`s
tuned for a developer testing at his own pace. An elder who pauses to
think gets cut off by a deadline she never chose. This PRD lifts the
dialog FSM's timing constants into a `[timing]` configuration table with
elder-friendly defaults, threaded through the FSM and daemon so every
deadline is deployment-tunable — the same `const`→config move
`hearth`'s persona-config made for the persona string, here for tempo.

## Why this exists

Phase-1 source reading (2026-05-29) of `wintermute-dialog/src/fsm.rs`:

- `pub const CONFIRM_TIMEOUT_MS: u32 = 30_000;` (fsm.rs:28) — fixed.
- `pub const MAX_REPROMPTS: u8 = 1;` (fsm.rs:31) — fixed.
- The wider timing family — `CAPTURE_TIMEOUT_MS`, `TRANSCRIBE_TIMEOUT_MS`,
  `THINK_TIMEOUT_MS`, `STATE_HEARTBEAT_MS` — is re-exported from the FSM
  module (lib.rs:34-35) and is likewise `const`.

None of these live in a config table. The companion is *for* "a
non-technical elder, jsy's mother" (companion.md seed). Her cadence is
not the developer's: she speaks slower, pauses inside a sentence, takes a
beat before answering. A 30-second confirm window and developer-tuned
capture/transcribe deadlines were never calibrated for her. `hearth`
already established the pattern and precedent — persona was a `const`
(brain `DEFAULT_PERSONA`), persona-config lifts it to a `[persona]`
table. earshot-dialog-timing does the identical thing for the FSM's
tempo, and is the foundation the rest of the earshot fleet reads.

This PRD does **not** change reprompt *behavior* (that's
`earshot-gentle-reprompt`, which depends on this one) — it only makes the
existing knobs configurable. It also does not touch `degrade.rs` (owned
by `hearth-dialog-degrade-warmth`).

## What this builds

- A `DialogTimingConfig` struct (serde-deserialized from a `[timing]`
  table, loaded from the dialog daemon's existing config source) with
  fields for each lifted constant: `confirm_timeout_ms`,
  `capture_timeout_ms`, `transcribe_timeout_ms`, `think_timeout_ms`,
  `state_heartbeat_ms`, `max_reprompts`.
- Elder-friendly defaults (via `Default`) that are **more patient** than
  today's `const`s where patience helps the listener — e.g. a longer
  confirm window and `max_reprompts >= 2` — while leaving
  machine-internal deadlines (`think`, `transcribe`, heartbeat) at safe
  values. Defaults documented inline with the rationale.
- The FSM and daemon read deadlines from the config value rather than the
  `const`s. The `const`s either remain as the `Default` source of truth
  or are replaced by `Default` impl constants; no magic numbers left in
  the transition/timer code.
- Config is optional: absent `[timing]` table → elder-friendly defaults,
  so existing deployments keep working without a config edit.
- Tests that pin the old `const` values (e.g. `Action::StartConfirmTimer
  { ms } if *ms == CONFIRM_TIMEOUT_MS`, fsm.rs:642) are **rewritten** to
  assert against the config-sourced value, not deleted.

### Shape (non-binding)

```toml
[timing]
confirm_timeout_ms   = 45000   # was const 30_000 — give her time to answer
max_reprompts        = 2       # was const 1 — try twice before giving up
capture_timeout_ms   = <elder-friendly>
transcribe_timeout_ms = <machine deadline, unchanged-ish>
think_timeout_ms     = <machine deadline, unchanged-ish>
state_heartbeat_ms   = <unchanged>
```

## Acceptance criteria

1. A `[timing]` config table deserializes into a `DialogTimingConfig`;
   each field maps 1:1 to a previously-`const` timing value.
2. With no `[timing]` table present, the daemon starts and uses
   elder-friendly defaults; `confirm_timeout_ms` default ≥ the old
   30_000 and `max_reprompts` default ≥ 2.
3. The FSM/daemon timer-start paths read the deadline from the config
   value, not from a module `const`; no timing magic number remains in
   the transition or timer-scheduling code (grep-confirmable).
4. A non-default `[timing]` value (e.g. `confirm_timeout_ms = 12000`)
   demonstrably changes the scheduled `StartConfirmTimer { ms }` to that
   value, asserted by a test.
5. Pre-existing tests that asserted against the old `const` timing values
   are updated to assert against the configured value and pass; none are
   deleted to make the suite green.
6. `cargo test` and `cargo clippy` (the repo's existing
   `-D warnings`-grade lint bar, incl. the `unwrap/expect/panic`-deny
   set in Cargo.toml) pass.
7. No change to `degrade.rs` or to the silence→idle *behavior*; this PRD
   only parameterizes deadlines. (Behavioral warmth is
   `earshot-gentle-reprompt`.)
