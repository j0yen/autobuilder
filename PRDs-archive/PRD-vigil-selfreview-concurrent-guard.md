# PRD: vigil-selfreview-concurrent-guard — don't auto-fix a daemon /build is mid-building

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-30
**Vision:** visions/vigil.md (Fleet 4)
**build_target:** shell
**Depends on:** none (serialize on SKILL.md with PRD-agorabus-reload-self-review — same playbook block)
**Codename:** *yield-to-build* — two hands on the same daemon is one too many.
**deferred_acs:** [7]
**deferred_ac_reasons:** {"7": "[user-verify] requires a live agorabus daemon in a stale-binary condition plus a running/simulated /build tick; explicitly marked [user-verify] in AC7"}

## TL;DR

Self-review's `agorabus_daemon_stale_binary` playbook auto-fixes a stale
bus daemon by rebuilding, reinstalling, and restarting it. Its "Auto-fix
conditions (ALL must hold)" list guards against missing cargo, an
`unknown` verdict, and a <5-minute restart loop — but **not** against a
/build tick that is *concurrently* rebuilding and committing the same
crate. When both fire at once, self-review and /build race on the same
binary, daemon, and socket. On 2026-05-29 (run 11) a human caught this
and deferred the auto-fix by hand. This PRD codifies that deferral: add
a concurrent-build guard to the playbook so the auto-fix yields to an
in-flight /build instead of colliding with it.

## Why this exists

Verbatim from the run-11 reflective memory (`01KSVDJF...`, 2026-05-29
20:05 PDT):

> PLAYBOOK GAP — `agorabus_daemon_stale_binary` auto-fix conditions
> don't check whether /build is concurrently building the same crate;
> that concurrent-build race is the real reason to defer, not the
> subscriber ceiling.

And the deferral that run, in the same memory:

> Deferred because active /build tick was concurrently building +
> committing agorabus → a self-review rebuild/install/restart would
> RACE /build on the same daemon/socket.

The current "Auto-fix conditions" in
`~/.claude/skills/self-review/SKILL.md:262-272` (read live 2026-05-30)
list four gates: `cargo` on PATH, verdict ≠ `unknown`, no fix attempt in
the last 5 minutes, and the subscriber ceiling. None of them detects a
concurrent /build. The race is real and current: /build now runs its
ticks detached in `claude-build-work.service` (a transient systemd-user
unit, 30-min cap — per the 2026-05-29 gossip "BUILD STALL fixed"
note), so its in-flight state is *directly observable* via
`systemctl --user is-active claude-build-work.service`. There is a clean
signal to gate on; the playbook just doesn't read it.

This is the *reaction-path safety* half of Fleet 4. The *prevention*
half (PRD-vigil-build-restart-wiring) reduces how often staleness occurs
at all; this PRD makes the auto-fix that handles the residual staleness
safe to run unattended.

## What this builds

A new bullet in the `agorabus_daemon_stale_binary` playbook's "Auto-fix
conditions (ALL must hold)" list, plus the matching escalation text.

- **New gate (added to the ALL-must-hold list):**

  > - **No concurrent /build on agorabus.** `systemctl --user is-active
  >   claude-build-work.service` is NOT `active`, AND
  >   `~/wintermute/agorabus/.git/index.lock` does not exist. Either
  >   signal means a /build tick may be mid-rebuild/commit of agorabus;
  >   a self-review rebuild+install+restart would race it on the same
  >   binary, daemon, and socket. **Defer** — do not auto-fix.

- **Escalation text:** when the guard trips, write to Pending:
  "agorabus stale (<verdict>) but a /build tick is concurrently building
  the crate (claude-build-work.service active / index.lock present) — a
  self-review reload would race it on the same daemon + socket. Deferred;
  re-check next run." Log an apply-log entry
  `step:fix_deferred_concurrent_build` with the observed signal so the
  pattern is auditable across runs (and so a future run can tell
  "deferred for /build" apart from "no staleness").
- **Recheck-not-loop:** the deferral does NOT count as a `fix_attempted`
  for the 5-minute loop-breaker — it's a yield, not a failure — so the
  next run re-evaluates cleanly once /build is quiescent.
- **Scope:** edits only the `agorabus_daemon_stale_binary` playbook
  block. The guard is agorabus-specific (that's the crate the race was
  observed on and the only daemon with an auto-fix today); generalising
  it to other daemons is a Fleet 2 deferral, honest until another daemon
  gains a self-review auto-fix.

**Serialization note:** PRD-agorabus-reload-self-review (vigil Fleet 3)
also edits this same playbook block (it swaps the fix to use
`agorabus reload --build` and lifts the subscriber ceiling). These two
must **serialize** on SKILL.md to avoid clobbering each other's edits —
order doesn't matter semantically (the guard is an independent
additional condition), but they must not be applied in parallel. Note
this for /build in gossip.

## Acceptance criteria

1. The `agorabus_daemon_stale_binary` "Auto-fix conditions (ALL must
   hold)" list gains a concurrent-build gate checking BOTH
   `systemctl --user is-active claude-build-work.service` ≠ `active` AND
   absence of `~/wintermute/agorabus/.git/index.lock`.
2. When the gate trips, the playbook escalates to Pending with the
   verdict, the tripping signal (which of the two fired), and the
   "deferred — would race /build" reason; it does NOT rebuild, install,
   or restart.
3. A deferral logs `step:fix_deferred_concurrent_build` (with the
   observed signal) to apply-log.jsonl, distinct from `fix_attempted`,
   `fix_verified`, and `fix_failed`.
4. The deferral does not arm the 5-minute loop-breaker: a subsequent run
   with `claude-build-work.service` inactive and no index.lock is free to
   auto-fix immediately (no artificial cooldown from a prior yield).
5. The guard is purely additive — when no /build is running and no
   index.lock exists, the playbook's existing behaviour
   (doctor-led reload path / legacy path) is unchanged.
6. The edit is confined to the `agorabus_daemon_stale_binary` block;
   no other playbook is modified. (`grep` shows the only diff is within
   that section.)
7. **[user-verify]** Synthetic test: start a sleeper that holds
   `~/wintermute/agorabus/.git/index.lock` (or a stub
   `claude-build-work.service`), force a stale-binary condition, and
   confirm the playbook defers-with-Pending rather than restarting;
   remove the signal and confirm the next run auto-fixes normally.
