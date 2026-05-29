# PRD: wintermute-homestead — readiness beacon (`wm ready`)

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/homestead.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-platform
**build_version_bump:** minor
**Depends on:** PRD-wintermute-fleet-install-doctor
**Codename:** *fit-to-serve* — the device knows, and says, whether it can do its job.

## TL;DR

`wm doctor` (fleet-install-doctor) proves every unit *can start*. But a
device can have all its units running and still be unable to do its job:
right now `WM_ANTHROPIC_API_KEY` in `/etc/wintermute/conf.d/00-bootstrap.env`
is **empty**, so wm-brain runs but cannot reason — and nothing produces a
verdict that says so. This PRD adds `wm ready`: a single standing
readiness check that joins the doctor's per-unit verdict with the things
a *working* companion needs (API key present or degrade-configured, an
audio source and sink, agorabus reachable, `wintermute.target` active).
It speaks the verdict on boot in plain language and emits a
`wm.health.ready` envelope the device can beacon off-device (the health
hook `vision-kin` wants). This is the deploy/boot readiness voice —
distinct from companion-degrade's mid-conversation failure voice.

## 1. Why this exists

- **Live silent unreadiness.** `WM_ANTHROPIC_API_KEY=` is empty
  (verified via `sudo grep` on `/etc/wintermute/conf.d/00-bootstrap.env`),
  yet `wmd.service` is `active running`. The brain is up and mute. The
  user-side key todo has been carried in reflective memory since the
  companion build day with no deploy-time gate to catch it.
- **"All units active" ≠ "ready to serve."** doctor answers "can each
  daemon start"; it does not answer "does the device have what it needs
  to actually converse." A device on a desk far away needs a single
  honest yes/no plus a reason.
- **The `wm.health.*` envelope already exists** in companion-degrade's
  design and is referenced by `vision-kin`'s family-health digest. A
  readiness beacon should *produce* into that envelope, not invent a
  parallel one.

## 2. What this builds

### 2.1 `wm ready` subcommand (extend `src/bin/wm.rs`)

Compute a readiness verdict over these checks:

- **Units** — call into the doctor logic (shared lib function): every
  wintermute unit's `ExecStart` resolves and required units are active.
- **Brain** — `WM_ANTHROPIC_API_KEY` is non-empty in the bootstrap env
  **or** a local/offline reasoning fallback is configured. Empty key
  with no fallback ⇒ a named NOT-READY reason.
- **Audio** — at least one PipeWire/Pulse source (mic) and one sink
  (speaker) present (the fleet already shells to `pw-cat`/`pactl`; reuse
  whatever wm-audio uses to enumerate, don't reinvent).
- **Bus** — agorabus reachable (a peer query succeeds within a timeout).
- **Target** — `wintermute.target` is active.

Output: a human verdict by default (`READY` or `NOT READY: <reasons>`),
`--format json` (`{ready: bool, checks: [{name, ok, detail}], ts}`), and
nonzero exit when not ready (so it can gate scripts).

### 2.2 Spoken boot verdict

On boot (a `wintermute-ready.service` ordered After the fleet, or a hook
the platform owns), speak the verdict through the existing wm-tts path:
- ready ⇒ a configurable boot phrase ("Wintermute is ready").
- not ready ⇒ a plain-language line naming the worst-failing subsystem
  ("Wintermute is up, but it can't reach its brain" for the empty-key
  case; "Wintermute can't hear — its microphone is missing"). One line,
  not a diagnostic dump.

**Phrase-bank boundary (flag for /build):** these are *boot/deploy*
phrases. They must not collide with companion-degrade's *mid-conversation*
phrase bank in wm-brain. Keep the boot phrases in platform; if a shared
bank is wanted, that's a follow-on, not this PRD.

### 2.3 Off-device beacon

Emit the verdict as a `wm.health.ready` event on agorabus (reusing
companion-degrade's `wm.health.*` envelope). This is the hook
`vision-kin`'s presence/health digest consumes — this PRD only *emits*;
the off-device delivery (email/ntfy) belongs to kin/wm-reach.

## 3. Acceptance tests

1. **AC1 — `cargo test --release --lib` ≥ current+6** covering: each
   check's pass/fail logic with injected state (empty key, missing sink,
   bus timeout, inactive target), verdict aggregation, worst-reason
   selection for the spoken line, `wm.health.ready` envelope shape,
   exit-code mapping.
2. **AC2 — catches the live empty key.** On this laptop, `wm ready`
   reports NOT READY with a reason naming the empty `WM_ANTHROPIC_API_KEY`,
   and exits nonzero. (After the user sets the key, it flips to the
   brain check passing — assertable by injecting a non-empty value in
   the test env.)
3. **AC3 — ready case.** With all checks injected green, `wm ready`
   exits 0, prints `READY`, and selects the configured boot phrase.
4. **AC4 — spoken line is one plain sentence.** For each NOT-READY
   reason, the selected utterance is a single non-technical sentence
   (no unit names, no exit codes) — verified against a phrase table.
5. **AC5 — envelope reuse.** The emitted event validates against
   companion-degrade's `wm.health.*` envelope schema (or, if not yet
   shipped, a shared schema fixture both reference) — no parallel
   envelope invented.
6. **AC6 — `--help` documents** `ready`, `--format`, the exit contract,
   and the boot-vs-conversation phrase-bank boundary.

## 4. Non-goals

- Mid-conversation failure phrases (companion-degrade owns those).
- Off-device delivery of the beacon (kin / wm-reach).
- Fixing any failing check (doctor/convention/watchdog/the user's key).

## 5. Files this PRD likely touches

- Modified: `src/bin/wm.rs` (`ready` subcommand), a shared lib module for
  the doctor check (so `ready` and `doctor` agree), `Cargo.toml`.
- New: `pkg/systemd/wintermute-ready.service` (boot verdict),
  `tests/acceptance_ready.rs`.

## 6. Open questions

- **Offline-brain fallback.** "Key present *or* offline fallback
  configured" assumes an offline path may exist. If none does, the brain
  check is simply "key non-empty." Confirm whether a local-LLM fallback
  is on the companion roadmap before wording the check (don't promise a
  fallback that doesn't exist).
- **Boot phrase ownership.** companion-boot also has a configurable boot
  phrase ("Wintermute is ready"). `wm ready` and companion-boot must
  share one phrase source, not two. Confirm which owns it (suggest:
  companion-boot owns the *ready* phrase; `wm ready` owns the *not-ready*
  reasons).
