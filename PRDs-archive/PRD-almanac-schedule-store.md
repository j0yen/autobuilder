# PRD: almanac-schedule-store — the local recurring-routine model

Status: Draft v0.1
build_target: rust-cli
build_into: /home/jsy/wintermute/wintermute-almanac
Vision: visions/almanac.md

## TL;DR

Wintermute has no place to record "the blue pill, every morning at 8."
This PRD creates the new `wintermute-almanac` crate and its `wm-almanac`
CLI: a durable, **local, offline** store of recurring routine entries for
the elder, with add / list / remove / enable subcommands. No bus, no
speech — just the model every other almanac PRD hangs on.

## Why this exists

Confirmed live in Phase 1 (2026-05-29):

- **No clock-driven schedule anywhere in the fleet.** `wintermute-brain`'s
  only proactive turn is `recap_opener` (`daemon.rs:1352`), fired once at
  session start. There is no recurring-time concept in `BrainConfig`
  (`lib.rs:80-118` — `user_name`, `timezone`, `recap_opener`, but nothing
  scheduled).
- **`wm-cal` is the wrong shape.** `wintermute-calendar` is a CalDAV daemon
  for *jsy's* appointments: credentials in SecretService (`creds.rs:16`),
  RRULE expansion (`caldav.rs:397`), and an explicit caregiver-facing
  rationale ("the caregiver must also see jsy's appointments",
  `agent/intent-card.json:17`). It requires network + an account. A
  medication prompt for an elder on a possibly-offline desk must be local
  and credential-free. almanac is that local store; it is not CalDAV.
- **The state-file convention already exists to mirror.** `wm-cal` keeps
  `wm-cal/state.json` under the XDG base (`events.rs:103`). almanac follows
  the same pattern at `wm-almanac/`.

## What this builds

New crate `wintermute-almanac` at `~/wintermute/wintermute-almanac`, binary
`wm-almanac`.

**Model** (`src/entry.rs`): `Entry { id: Ulid-ish string, label: String,
say: String, recurrence: Recurrence, local_time: "HH:MM", tz: String
(IANA, default from $WM_TIMEZONE or system), category: Category, opt_in:
bool (default true), snooze_min: u32 (default 10), max_snoozes: u32
(default 2), created_ms: u64 }`.
- `Recurrence`: `Daily | Weekly { days: Vec<Weekday> } | Once { date: "YYYY-MM-DD" }`.
- `Category`: `Med | Meal | Appointment | Activity`.

**Store** (`src/store.rs`): a TOML file at
`$XDG_DATA_HOME/wm-almanac/schedule.toml` (fall back to
`~/.local/share/wm-almanac/`). Atomic write (temp + rename). Load tolerates
a missing file (empty schedule). Reuse `chrono` + `chrono-tz = "0.9"`
(already fleet deps per `wintermute-calendar/Cargo.toml`) for tz validation.

**CLI** (`src/main.rs`, clap, matching `wm-cal`'s clap style):
- `wm-almanac add --label <s> --at <HH:MM> --every <daily|mon,wed,fri|once:YYYY-MM-DD> --say <s> [--category med|meal|appt|activity] [--tz <IANA>] [--snooze-min N] [--max-snoozes N]`
- `wm-almanac list [--format text|json]`
- `wm-almanac remove <id>`
- `wm-almanac enable <id>` / `wm-almanac disable <id>` (toggles `opt_in`)
- `wm-almanac next [--format json]` — print the next due entry and its
  next-fire timestamp in the entry's tz (the computation tick-daemon reuses)

## Acceptance criteria

1. `wm-almanac add --label "morning pills" --at 08:00 --every daily --say "time for your blue pill" --category med` exits 0 and persists one entry to `schedule.toml`.
2. `wm-almanac list --format json` returns a JSON array containing that entry with all fields; `--format text` prints a human line per entry. Default is text.
3. `wm-almanac remove <id>` deletes only that entry; `list` no longer shows it; other entries untouched.
4. `wm-almanac disable <id>` sets `opt_in=false` (entry retained, shown as disabled in `list`); `enable <id>` restores it.
5. `--every mon,wed,fri` parses to `Weekly{days:[Mon,Wed,Fri]}`; `--every once:2026-06-01` parses to `Once`; an invalid `--every` exits non-zero with a clear message.
6. An invalid `--at` (e.g. `25:00`) or invalid `--tz` exits non-zero without writing the store.
7. `wm-almanac next --format json` computes the next fire instant for the soonest enabled entry in its IANA tz (DST-correct via chrono-tz) and prints `{id, fire_ts_unix, label}`; with an empty/all-disabled store it exits 0 emitting `null`/empty and a "no entries due" note.
8. Store writes are atomic (temp-file + rename); a load against a missing file yields an empty schedule, not an error.
9. `cargo test` green; `cargo build --release` produces `wm-almanac`; `wm-almanac --help` lists all subcommands. No network, no SecretService, no bus dependency in this crate.
