# PRD: daily-receipt-printer — physical thermal-strip emitter for the MASUNG IP1000

**Status:** Draft v0.1
**build_auto:** true
**build_target:** rust-cli
**build_into:** /home/jsy/wintermute/daily-receipt-printer
**build_version_bump:** N/A (new crate)
**Depends on:** PRD-daily-receipt.md (core encoder must be installed)
**Synergistic with:** PRD-cadence-bind-daily-receipt.md (lineage is nicer when present)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

`daily-receipt` already produces a byte-stable ESC/POS stream and stops
there by design. This PRD ships `daily-receipt-printer`, a small
Rust crate + binary `receipt` + systemd-user timer that takes that
byte stream and pushes it at `/dev/usb/lp0` once per day at a fixed
hour. The printer is a **MASUNG IP1000** (VID:PID `0485:7541`, 58mm,
ESC/POS, no auto-cutter — tear bar only), bound to the in-tree
`usblp` driver. The wrapper does *no* haiku composition, *no* day-type
override logic, and *no* glyph generation — that's all upstream.
It writes bytes, records that it wrote them, and stays out of the way.

## Why this exists

Phase 1 research, 2026-05-27:

- `~/wintermute/daily-receipt/` exists and ships v0.1 with 7 ACs green
  (ESC/POS encoder, day-type classifier, glyph renderer). README
  explicitly states "Rust core never touches the printer; downstream
  wrappers and a future `daily-receipt-printer` crate own that."
- Printer arrived 2026-05-27. Smoke test wrote bytes to `/dev/usb/lp0`
  without paper loaded; kernel accepted, physical output deferred to
  paper-load. Device confirmed at `/sys/devices/.../usbmisc/lp0`.
- Group `lp` membership for `jsy` already added via `usermod -aG lp jsy`
  this session. Effective after re-login or `newgrp lp`.
- Without this PRD, daily-receipt's output sits in `/tmp` or a state
  directory, never reaches paper, and the year-end scroll never
  accumulates. The whole point of the project is the daily tactile
  beat — without the printer wrapper, the project is silent.

## What this builds

### Crate shape

- Name: `daily-receipt-printer`
- Binary: `receipt`
- LOC budget: ≤500 src, ≤300 tests.
- Dependencies: stdlib + `clap` + `chrono` + `daily-receipt` (path or git).
  Explicitly NOT `libusb` / `rusb` — uses the kernel character device.

### CLI surface

```
receipt today             # render + print today's strip, idempotent
receipt today --force     # reprint even if today already marked done
receipt today --dry-run   # render to stdout, do not write to printer
receipt status            # print device-status JSON: present, group ok, last-printed-date
receipt test              # send a known-good 3-line smoke strip and exit
```

`receipt today` flow:
1. Resolve today's ISO date (`chrono::Local`).
2. Check state file `~/.local/state/daily-receipt/last-printed-date.txt`.
   If equal to today and not `--force`, exit 0 silently with stderr note.
3. Resolve content payload from (in order):
   - `--content <path>` flag (explicit override)
   - `$DAILY_RECEIPT_CONTENT_DIR/<today>.json` (default
     `~/.claude/daily-receipt/`)
   - If neither: emit a deterministic quiet-day glyph with seed
     `u64::from_le_bytes(today.format("%Y%m%d").parse::<u32>())`.
4. Resolve summary payload from `--summary <path>` or
   `$DAILY_RECEIPT_SUMMARY_DIR/<today>.json` (default same dir).
   If absent: synthesize a minimal summary (`{date, commits:0,
   repos:[]}`) — yields a quiet day.
5. Shell out to `daily-receipt render` with those paths → bytes file.
6. Open `/dev/usb/lp0` for write (O_WRONLY). Write all bytes. Close.
7. On successful write, atomically update state file (write to
   `.tmp`, rename).
8. Exit 0.

### Scheduling

Ship two systemd-user units in `units/`:

- `daily-receipt.service` — `Type=oneshot`, `ExecStart=%h/.local/bin/receipt today`.
- `daily-receipt.timer` — `OnCalendar=*-*-* 21:30:00`, `Persistent=true`.

`install.sh` copies these to `~/.config/systemd/user/` and runs
`systemctl --user enable --now daily-receipt.timer`. Idempotent.

### Device-status query

`receipt status` returns JSON:

```json
{
  "device_present": true,
  "device_path": "/dev/usb/lp0",
  "group_ok": true,
  "user_in_lp": true,
  "last_printed_date": "2026-05-27",
  "state_file": "/home/jsy/.local/state/daily-receipt/last-printed-date.txt"
}
```

No ESC/POS status query (DLE EOT 1) in v0.1 — that's v0.2 once we
confirm the IP1000 supports it. v0.1 is best-effort write: if the
kernel accepts the bytes, we mark printed.

## Out of scope (v0.1)

- ESC/POS bidirectional status (paper-out detect, cover-open).
- Auto-cutter handling — IP1000 is tear-bar; we still emit the
  GS V byte (AC2 of core), printer treats as no-op.
- Haiku composition / Claude API calls.
- Day-type override CLI (the core PRD said `receipt today --type
  <kind>`; deferred to v0.2 once we know what the year-end scroll
  actually wants).
- Retry on transient error. v0.1 fails fast.

## Acceptance criteria

- **AC1**: `receipt today --dry-run` with a fixture summary + content
  writes a non-empty byte stream to stdout that begins with ESC '@'
  (`0x1B 0x40`). Same payload twice = same bytes (delegated to
  daily-receipt's AC3 determinism).
- **AC2**: With no `$DAILY_RECEIPT_*` env vars and no flags,
  `receipt today --dry-run --date 2026-05-27` synthesizes a quiet-day
  glyph and renders without panicking. Output contains GS '*' raster
  command bytes (`0x1D 0x2A`).
- **AC3**: Idempotency: running `receipt today --dry-run` twice for
  the same date writes the byte stream the first time and exits 0
  with an "already printed YYYY-MM-DD" stderr message the second time.
  State file matches today's ISO date. `--force` overrides.
- **AC4**: State file write is atomic: middle-of-write SIGKILL
  (simulated by writing to a path the test then truncates) leaves
  the original file unchanged. Tested via `tempfile::NamedTempFile`
  rename semantics.
- **AC5**: `receipt status --json` returns valid JSON with all keys
  present (`device_present`, `device_path`, `group_ok`, `user_in_lp`,
  `last_printed_date`, `state_file`) even when the printer is absent
  or `/dev/usb/lp0` does not exist. `device_present: false` is not
  an error — exit 0.
- **AC6**: `receipt today` against a non-existent device path
  (`DAILY_RECEIPT_DEVICE=/tmp/nope` overrides default) exits with
  code 4 and writes no state-file update. Error message names the
  path and suggests checking the cable.
- **AC7**: systemd unit files in `units/` parse cleanly with
  `systemd-analyze verify` (test runs verify against the installed
  copies in a tempdir). Timer's `OnCalendar` parses as `*-*-* 21:30:00`.
- **AC8**: `install.sh` is idempotent: running it twice produces
  identical `~/.config/systemd/user/` contents and the second run
  exits 0. Tested in a `tempfile::TempDir` sandboxed `$HOME`.
- **AC9**: `receipt test` writes the canonical 3-line ESC/POS smoke
  strip (`\x1B@` + "MASUNG IP1000 smoke test\n2026-05-27 receipt\nOK\n"
  + 4 line feeds + `\x1DV\x42\x00`) to the device path. With
  `--dry-run`, writes to stdout. Byte count exactly 78 ± date format
  drift; covered by a snapshot test.

## Files this will create

```
~/wintermute/daily-receipt-printer/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs          # clap dispatch
│   ├── today.rs         # render+print flow
│   ├── state.rs         # atomic state file
│   ├── status.rs        # status JSON
│   └── smoke.rs         # `receipt test` payload
├── units/
│   ├── daily-receipt.service
│   └── daily-receipt.timer
└── tests/
    ├── ac1_dryrun_init.rs
    ├── ac2_quiet_fallback.rs
    ├── ac3_idempotent.rs
    ├── ac4_atomic_state.rs
    ├── ac5_status_json.rs
    ├── ac6_missing_device.rs
    ├── ac7_systemd_verify.rs
    ├── ac8_install_idempotent.rs
    └── ac9_smoke_strip.rs
```

## Non-functional

- No network calls. No telemetry. No `unsafe`.
- Single-binary install, no daemon. The timer is the daemon.
- All paths via `XDG_STATE_HOME`/`XDG_CONFIG_HOME` with `~/.local`
  fallback.
- Failures are loud on stderr, exit codes are stable:
  - 0: success or already-printed-today
  - 2: bad CLI args
  - 3: render failure (delegated from daily-receipt)
  - 4: device not present / not writable
  - 5: state-file write failure

## After this lands

The next PRD (`PRD-daily-receipt-haiku.md`, future) generates today's
content payload — the Claude-API haiku call that the original PRD
deferred. The printer wrapper stays mechanical; the artful part is
upstream of it.
