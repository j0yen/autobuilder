# PRD: warden-deadman — arming the enforcer cannot strand you

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/warden.md (Fleet 1)
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/bpolicy
**build_version_bump:** minor
**Depends on:** PRD-warden-home (Fleet 1); pairs with PRD-warden-policy
**Codename:** *railing-on-the-cliff* — a too-tight policy self-heals on a clock.

## TL;DR

A `file_open` LSM hook that denies writes is, by construction, the kind
of thing that can brick the laptop: arm a too-tight allow-list on your
own session and you can no longer write the file that would loosen it.
That is the real reason a careful user never runs `bpolicy load` — there
is no railing. This PRD adds two: **`--audit`**, a log-only mode where
every would-be denial is counted and logged but nothing is blocked, so
you can watch what a profile *would* do against a live workload before
trusting it; and **`--ttl` + `renew`**, a deadman timer that auto-unloads
the enforcer if it isn't renewed, so a bad arm is self-healing within
minutes instead of permanent. With these, `bpolicy load` stops being a
cliff and becomes a thing a cautious person can actually try.

## Why this exists

- The self-review Pending line is consistent: bpolicy is *never loaded*
  (`{"loaded": false}` every run, 2026-05-29 runs 1/2). The journals
  treat loading as a thing that "drops the voice fleet + live /build +
  /dream + interactive sessions" — i.e. as irreversible-feeling and
  high-blast-radius. A deadman + an audit mode are exactly the
  blast-radius controls whose absence keeps the tool inert.
- The toolkit memory documents the enforcer as adversary-proof
  ("survives the agent being adversarial or buggy") — but an
  adversary-proof guardrail you dare not turn on protects nothing. The
  missing piece is operator safety, not enforcement strength.
- Audit mode is also the only honest way to *tune* a warden-policy
  profile: load it in `--audit`, run a normal session, read which paths
  it would have denied, widen the profile, repeat — all without ever
  blocking a real write. Without audit mode, profile tuning is
  trial-and-brick.
- Runtime BPF-LSM only — no kernel-package change (Phase 1.5 note). The
  audit flag is a BPF config-map value read by the existing hook; the
  deadman is pure userspace (a timer that calls `unload`).

## What this builds

`rust-extend` into `~/wintermute/bpolicy`.

**Audit mode (`--audit`):**
- Add a single-entry `config` BPF map with a `mode` field
  (`0=enforce`, `1=audit`). The `file_open` hook reads it: in `audit`
  mode it performs the same allow/deny *evaluation* and bumps the
  `denied` stat (and, with warden-policy's per-prefix counter if present,
  the per-rule counter) but **returns 0** (allow) regardless.
- `bpolicy load --audit [--profile <name>]` sets `mode=1` before attach.
- `bpolicy status` reports `"mode": "audit" | "enforce"` (additive
  field; absent ⇒ `enforce`, the historical behavior).
- `bpolicy enforce --pid` works the same; the difference is only whether
  a denial *blocks* or merely *counts*.

**Deadman timer (`--ttl`, `renew`):**
- `bpolicy load --ttl <dur>` records an expiry (`now + dur`) in a state
  file (`~/.config/bpolicy/deadman.json`: `{loaded_at, ttl_secs,
  expires_at, pid_of_arm}`) and arms a userspace watchdog: a detached
  `systemd-run --user --on-active=<dur>` transient unit (or a `pevent`
  supervised sleeper) that runs `bpolicy unload` at expiry unless the
  state file's `expires_at` has moved forward.
- `bpolicy renew [--ttl <dur>]` pushes `expires_at` forward and resets
  the watchdog timer. Cheap; meant to be called from a heartbeat.
- `bpolicy unload` cancels the watchdog and clears the state file.
- **Default-on:** a bare `bpolicy load` defaults to `--ttl 30m` (the
  user who types it bare is the one who most needs the railing). Opt out
  with `--ttl 0` for a permanent arm; `status` shows `"ttl_remaining_s"`
  (or `null` when permanent).
- The watchdog uses `systemd-run --user` so it survives the arming
  shell's exit (per the detached-build lesson, memory
  `self_build_detached_cgroup_teardown` — don't let teardown kill the
  thing that's supposed to outlive you).

**Safety interlock:**
- `bpolicy load` (enforce mode, non-zero TTL, real profile) prints a
  one-line summary of what it will deny and the TTL, and — when stdin is
  a TTY — requires `--yes` or a confirm. Headless callers pass `--yes`.
  Audit mode never prompts (it blocks nothing).

## Acceptance criteria

1. `bpolicy load --audit` sets the config map `mode=1`; a mocked hook
   harness shows a would-deny write is *counted* (`stats.denied`
   increments) but *allowed* (hook returns 0). Tested at the BPF-logic
   spec level (the userspace mirror) and at the control-plane mock.
2. `bpolicy status` reports `"mode": "audit"` after `load --audit` and
   `"mode": "enforce"` after `load`; the field is additive and
   warden-home's golden status test still passes.
3. `bpolicy load --ttl 15m` writes `~/.config/bpolicy/deadman.json` with
   a correct `expires_at` and arms a `systemd-run --user` transient unit
   that would call `bpolicy unload`. Verified by inspecting the created
   unit (`systemctl --user list-timers` / `show`) in a smoke test, or
   mocked at the `systemd-run` boundary with the exact argv asserted.
4. `bpolicy renew --ttl 15m` moves `expires_at` forward and re-arms the
   watchdog; the old timer is cancelled (no double-unload). Tested:
   two renews leave exactly one live watchdog.
5. A bare `bpolicy load` (no `--ttl`) defaults to a 30-minute deadman;
   `--ttl 0` arms permanently and `status` shows `ttl_remaining_s: null`.
6. `unload` cancels the watchdog and removes the state file; a second
   `unload` is idempotent and leaves no orphan timer.
7. Enforce-mode `load` on a TTY without `--yes` refuses and prints the
   would-deny summary + TTL; with `--yes` it proceeds; `--audit` never
   prompts. Tested with a faked TTY/non-TTY stdin.
8. End-to-end deadman smoke (VM or privileged, else deferred-AC with
   reason): `load --ttl 30s --audit`, wait past expiry, assert the
   enforcer is unloaded (`status` → `{"loaded": false}`) without manual
   intervention.
9. `cargo clippy -D warnings` + `cargo test` green; the watchdog and
   state-file logic are unit-tested with time injected (no real sleeps
   in tests — pass a clock).

## Notes

- **This is the PRD that makes arming reasonable to recommend.** Until it
  lands, the honest advice stays "don't load it on a session you care
  about." After it lands, `bpolicy load --audit` against any session is
  safe (blocks nothing) and `bpolicy load --ttl 15m` against a sandboxed
  or headless session is recoverable by construction.
- No clock/random in tests (autobuilder constraint): inject the clock,
  assert argv at the `systemd-run` boundary, never `sleep` in a unit
  test. The single end-to-end timing AC is the one allowed real-wall
  smoke and is deferred-gated on a privileged build env.
- Serialize with **PRD-warden-policy** — same `build_into`, never build
  in parallel. If warden-policy ships first, the per-prefix audit
  counter (its open question) lands here as additive; if deadman ships
  first, audit mode counts the single `denied` scalar and the per-prefix
  refinement folds in when policy lands.
- Default-on TTL is an open question flagged to the user in the vision;
  this PRD drafts it defaulted-on (30m) as the safer default. If the
  user prefers opt-in, the change is a one-line default flip.
