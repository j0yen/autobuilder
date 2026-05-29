# Vision: homestead — a device far from its maker must keep itself alive

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Seed:** bare `/dream` + Phase-1 live inspection. The companion voice loop
runs on jsy's laptop (mic→bus→speaker structurally complete, 5 daemons
active). But the device this is *for* sits on a desk at jsy's mother's
home with no IT person. Between "the loop works here" and "it survives
there unattended" is a layer nobody has dreamed. Caught live this pass:
`wmd-init.service` is **failed (start-limit-hit)**, exec'ing a path that
does not exist.

---

## TL;DR

`companion-boot` gets a fresh device from power-button to boot phrase.
`companion-degrade` gives the *conversation* a voice when a subsystem
fails. `vigil` catches a running process exec'ing a **stale** binary.
None of them catch the three failures that will actually brick a device
at someone's mother's house:

1. **A unit whose `ExecStart` path was never populated.** Not stale —
   *absent*. Live right now: `wmd-init.service` →
   `ExecStart=/usr/local/bin/wmd-init`, which **does not exist**
   (the binary is at `~/.local/bin/wmd-init`). The supervisor meant to
   own the fleet is dead.
2. **A failed unit that no human will ever recover.** `wmd-init.service`
   has `Restart=always RestartSec=2`; it burned its default start-limit
   in ~10s and is now `failed (Result: start-limit-hit)` — *permanently*,
   until someone runs `systemctl reset-failed`. On mother's device there
   is no someone.
3. **A device that does not know it is not ready.** `WM_ANTHROPIC_API_KEY`
   in `/etc/wintermute/conf.d/00-bootstrap.env` is **empty** — wm-brain
   cannot reason — yet nothing produces a standing "this device is not
   deploy-ready, and here is why" verdict before or after boot.

homestead builds the unattended-survival layer: the fleet's wiring
matches where the bits are, a failed unit heals itself without a human,
and the device knows — and says — whether it is fit to serve.

## Why this is real (Phase 1 evidence, 2026-05-29 ~06:30 UTC)

Measured live this session against the running fleet:

```
unit          ExecStart (declared)        resolves to                      state    bin?
wm-audio      %h/.cargo/bin/wm-audio      /home/jsy/.cargo/bin/wm-audio    active   OK
wm-dialog     %h/.local/bin/wm-dialog     /home/jsy/.local/bin/wm-dialog   active   OK
wm-stt        %h/.local/bin/wm-stt        /home/jsy/.local/bin/wm-stt      active   OK
wm-tts        %h/.local/bin/wm-tts        /home/jsy/.local/bin/wm-tts      active   OK
wmd           %h/.local/bin/wmd           /home/jsy/.local/bin/wmd         active   OK
wmd-init      /usr/local/bin/wmd-init     /usr/local/bin/wmd-init          FAILED   MISSING
```

- **Three install conventions across six units** (`~/.cargo/bin`,
  `~/.local/bin`, `/usr/local/bin`). The odd-one-out (`/usr/local/bin`,
  no `%h` specifier, nothing installs there) is the one that's dead.
  The companion vision (`visions/companion.md`, Notes-for-/build)
  *named* this drift — "install-path drift (cargo install → ~/.cargo/bin;
  systemd → ~/.local/bin) bit four PRDs in a row today" — and assigned
  the fix to companion-boot. But companion-boot's ACs are kiosk/greeter/
  autologin/power-loss; the path convention is a "likely touches
  install.sh" aside, not an enforced acceptance criterion. The root
  cause is still live and now *failing*.
- `systemctl --user status wmd-init.service`: `Active: failed (Result:
  start-limit-hit) since Thu 2026-05-28 14:29:52 PDT; 9h ago`,
  `status=203/EXEC`. start-limit-hit recorded in 3 self-review runs and
  the `vision-kin` gossip aside ("flag for the companion-reliability
  surface / next self-review — not in kin's scope"). That surface has
  had no home until now. homestead is that home.
- `WM_ANTHROPIC_API_KEY=` → **empty** (verified via sudo grep). wm-brain
  is running but cannot reason; the user-side key todo has been carried
  in reflective memory since the companion build day with no deploy-time
  gate to catch it.

## End-state

When this vision is fulfilled:

1. **Every wintermute unit's `ExecStart` resolves to an executable that
   exists.** A unit pointing at a path nothing installs to is an
   *install error caught before deploy*, not a daemon that dies at boot.
2. **The fleet uses one install-path convention,** enforced by the
   installer, verified by a doctor the install runs before declaring
   success. `wmd-init` boots.
3. **A failed wintermute unit recovers without a human.** A watchdog
   detects `failed` state, clears it, and restarts with capped backoff;
   transient flaps no longer permanently brick a unit via start-limit.
4. **The device produces a standing readiness verdict** — binaries
   present, key present-or-degrade-configured, audio source+sink present,
   bus reachable, target active — speaks it on boot in plain language,
   and can beacon it off-device (the health hook `vision-kin` wants).

## Components (one bullet per PRD)

- **fleet-install-doctor** — `wm doctor` (extend platform's `wm` binary):
  enumerate every wintermute systemd unit (user + system), expand
  specifiers, verify `ExecStart` resolves to an executable, verify
  enablement vs `wintermute.target`, per-unit PASS/FAIL, nonzero exit on
  any failure. The pre-deploy gate. *Directly motivated by the live
  `wmd-init` 203/EXEC and the three-convention table above.*
- **install-path-convention** — extend `wintermute-platform/install.sh`:
  one convention, enforced idempotently (install/symlink every fleet
  binary to its unit's declared path, or rewrite the unit), install ends
  by running `wm doctor` which must pass. *Fixes the root cause the
  companion vision flagged; unbricks `wmd-init`.*
- **unit-recovery-watchdog** — `wintermute-watchdog` (new bin + systemd
  unit in platform): detect any wintermute unit in `failed`,
  `reset-failed` + restart with capped exponential backoff; tune
  `StartLimitIntervalSec`/`StartLimitBurst` on fleet units so a flap
  doesn't become a permanent death. *Motivated by the live
  start-limit-hit permanent failure.*
- **readiness-beacon** — `wm ready` (extend `wm`, consumes the doctor):
  one standing verdict over binaries + key + audio devices + bus +
  target; spoken on boot through wm-tts ("Wintermute is ready" /
  "Wintermute is up but can't reach its brain"); emits `wm.health.ready`
  for off-device beaconing. *Motivated by the empty API key with no
  deploy-time gate; the deploy-readiness voice, distinct from
  companion-degrade's conversational-failure voice.*

## Order

```
fleet-install-doctor  (foundation — others read its verdict)
        │
        ├──> install-path-convention  (runs doctor as its install gate)
        │
        ├──> readiness-beacon         (consumes doctor's per-unit result)
        │
unit-recovery-watchdog  (independent; can ship in parallel)
```

- doctor first: convention and beacon both consume it.
- convention depends on doctor (uses it as the post-install gate).
- watchdog is independent — no dep on doctor; can run in a parallel agent.
- beacon depends on doctor (joins its unit verdict with key/audio/bus).

## Boundaries (what this is NOT — to keep /build from merging scopes)

- **vs companion-boot:** boot owns power-button → boot phrase (kiosk,
  greeter, autologin, *reboot*-triggered power-loss recovery). homestead
  owns per-unit `ExecStart` correctness, *runtime* failed-unit recovery,
  and the standing readiness verdict. boot fires once per boot; homestead
  runs continuously.
- **vs companion-degrade:** degrade owns the *conversational* failure
  voice ("I can't reach my brain right now") via a phrase bank in
  wm-brain. homestead's beacon owns the *deploy/boot* readiness voice
  and the off-device health envelope. Both may speak through wm-tts;
  the phrase banks must not collide (degrade = mid-conversation,
  beacon = boot/health). Flagged for /build.
- **vs vigil/binstale:** vigil catches a running process exec'ing a
  **stale or deleted** binary (inode drift vs source HEAD). homestead
  catches a unit whose `ExecStart` is **absent** at the declared path
  (never installed there). Complementary detectors; a future PRD could
  unify them under one `wm doctor` surface.

## Open questions (for jsy)

1. **Which convention wins?** `~/.local/bin` (where 5/6 units already
   point, user-scope, matches `/build`'s publish target) or
   `/usr/local/bin` (system-scope, survives multi-user, where `wmd-init`
   already points). The convention PRD defaults to `~/.local/bin` (least
   churn: rewrite one unit, not five) but a kiosk device may want system
   scope. Decide before install-path-convention ships.
2. **Watchdog scope:** user-level (where the fleet lives) or system-level
   (more reliable, survives user-session teardown)? companion-boot raised
   the same question for its recovery service and defaulted system-level
   for recovery only. Stay consistent.
3. **Does the readiness beacon's off-device emit belong to kin?** The
   `wm.health.*` envelope already exists in companion-degrade's design.
   homestead's beacon should *reuse* it, and kin's family-health digest
   should *consume* it — confirm the envelope owner before duplicating.
