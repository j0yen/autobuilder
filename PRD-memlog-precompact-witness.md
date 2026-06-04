# PRD: memlog-precompact-witness — give /dev/memlog a producer and a reader

Status: in_progress
deferred_acs: [6]
# AC6 (live-group survival smoke) is deferred: the memlog group is not yet
# joined on this box, so /dev/memlog is unwritable. ACs 1-5,7 are built and
# proven; the write mechanism is verified against a writable stub device.
build_target: mixed
build_into: /home/jsy
Vision: visions/onramp.md
Author: Claude (Opus 4.8), with jsy
Date: 2026-05-30
Depends on: PRD-memlog-group-autojoin.md (group must exist + user joined)
  + PRD-memlog-activation-self-review.md (activation surfaced) — both must
  ship AND the package must be activated (group readable) before this PRD's
  writes can succeed. Until then this PRD's hook fails open (writes nothing).

## TL;DR

`/dev/memlog` is, per its kernel design, "a per-uid circular log of
pre-compaction context snapshots." But nothing on this laptop writes to it,
and the reader CLI isn't even installed. The only PreCompact hook configured
is `peon-ping/peon.sh` — a sound effect. So the single kernel primitive
purpose-built to survive context compaction captures nothing; every
compaction still discards context into the void. This PRD wires the producer
(a PreCompact hook that appends the about-to-be-discarded context to
`/dev/memlog`) and installs the consumer (`memlog show`), turning the dead
device into the working substrate the continuity vision gates on.

## Why this exists

Verified on this laptop 2026-05-30:

- `grep PreCompact ~/.claude/settings.json` → one hook, command
  `/home/jsy/.claude/hooks/peon-ping/peon.sh` (a notification sound). No
  hook references memlog or the transcript.
- `~/.local/bin/memlog-witness` is installed (436 KB, built 2026-05-29) with
  a `daemon|status|drain` interface — but the **reader** `memlog` is NOT in
  `~/.local/bin` (`ls ~/.local/bin/memlog` → no such file). So even if
  something wrote, nothing here reads it back.
- The onramp vision's architecture diagram names `memlog-witness` and a
  `session-postmortem` consumer in its top "CONSUMERS" layer as the things
  "usefully running [that] gate on this vision" — none are wired to the live
  device.
- `/dev/memlog` is a live char device on the *current* pkgrel-5 boot (the
  driver loads independent of the group fix), so once the group is joined,
  writes are immediately possible — no kernel change needed.

The Claude Code PreCompact hook receives the session `transcript_path` on
stdin (JSON) before compaction runs. That is exactly the content the kernel
buffer was built to preserve. The plumbing exists on both ends; only the
connection is missing.

## What this builds

1. **Install the reader.** `install -m755
   ~/wintermute/memlog/cli/memlog` (build with `cargo build --release -p
   memlog` first) into `~/.local/bin/memlog`, alongside the already-present
   `memlog-witness`. Adds `memlog` to the toolkit so `memlog show --since
   1h --format json` works (the reader named in /dream Phase 1.5).

2. **PreCompact producer hook.** A new shell hook
   `~/.claude/scripts/memlog-precompact.sh` that:
   - reads the PreCompact JSON on stdin, extracts `transcript_path`;
   - composes a bounded snapshot record (session id from
     `/proc/self/agent_session` if non-zero else the agorabus sid; timestamp;
     last-N-turns digest or a head/tail slice of the transcript, capped to
     the device's record-size limit);
   - appends it to `/dev/memlog` via the memlog write path (libmemlog /
     `memlog append`, or hands the digest to `memlog-witness drain` — the
     build picks whichever the installed interface exposes);
   - **fails open**: if the group isn't joined yet (EACCES), the device is
     absent, or the record is empty, it logs one line to a rotating
     `~/.cache/memlog/precompact.log` and exits 0 — never blocking
     compaction. This is what makes the hook safe to install *before* the
     group is activated.
   Registered as a **second** PreCompact hook entry in `settings.json`
   (additive — does not replace peon-ping), `async: true`, `timeout: 10`.

3. **Survival smoke.** A test that runs the hook against a synthetic
   transcript, then `memlog show --since 1m` returns the record — proving
   the snapshot survives in the circular buffer and is readable back.

The settings.json edit goes through the `update-config` discipline (the
harness owns hooks); the hook script and `memlog` install are plain files.

## Acceptance criteria

1. `~/.local/bin/memlog` is installed from `~/wintermute/memlog` and
   `memlog show --since 1h --format json` exits 0 with valid JSON (empty
   array acceptable when the buffer holds nothing).
2. `~/.claude/scripts/memlog-precompact.sh` parses a PreCompact JSON stdin
   payload, extracts `transcript_path`, and composes a record bounded to the
   device's max record size (no oversized write).
3. The record's session-id field is `/proc/self/agent_session` when
   non-zero, else the agorabus sid, else `comm:`-form — never empty.
4. When `/dev/memlog` is unwritable (EACCES / absent), the hook logs one
   line and exits 0; compaction is never blocked. Verified by running the
   hook as a non-member of `memlog` and asserting exit 0 + a log line.
5. `settings.json` gains a **second** PreCompact hook entry (peon-ping
   retained), `async:true`, applied via the update-config path; the JSON
   remains valid (`jq . settings.json`).
6. Survival smoke: feed a synthetic transcript through the hook (as a
   memlog-group member), then `memlog show --since 1m` returns a record
   whose digest matches the synthetic input. (This AC is **deferred-gated**
   on memlog-group activation — declare `deferred_acs` if the group is not
   yet joined at build time, same pattern as agentns-claude's boot-gated
   ACs.)
7. `bash -n` clean on the hook; a rotating `~/.cache/memlog/precompact.log`
   is pruned to the last 14 days (reuse the rotation idiom from
   `agorabus-session-start.sh`).

## Notes

- This is the consumer/producer the onramp vision's top layer "gates on" —
  it's why the kernel-pkg-postinstall and group-autojoin work matters.
- Hard-ordered after group activation for the *write* path, but the reader
  install (AC1) and the fail-open hook (ACs 2–5,7) are testable and useful
  immediately; only AC6 needs the live group. Build the safe parts now,
  defer AC6.
- Does not touch `memlog-witness`'s daemon role; if a witness daemon is
  later run to roll up records, this hook is its upstream producer.
- Pairs with PRD-session-postmortem (existing, in queue): once memlog holds
  pre-compaction snapshots, a post-mortem can read them via `memlog show`.
