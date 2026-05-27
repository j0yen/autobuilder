# PRD: memlog-witness — userspace daemon for per-session memlog persistence

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/continuity.md](visions/continuity.md)
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/memlog
**Boot-gated:** all live-data ACs gate on `/dev/memlog` being
available (i.e., booted into `linux-wintermute`). Scaffolding,
parser, and per-session writer can iterate against a fixture file
pre-boot.

---

## TL;DR

The `memlog` kernel module captures pre-compaction LLM context
snapshots into a per-uid circular ring at `/dev/memlog`. The
existing `memlog/cli/memlog` (Python) reads recent records ad-hoc.
What's missing is a long-running consumer that subscribes to the
device, demultiplexes records by `session_id` (read from
`/proc/$PID/agent_session` of the writer when available), and
persists each session's snapshots to
`~/.claude/memlog/<session-id>/snap-NNNN.json`. That gives downstream
tools (PRD #5 postmortem, future episodic-observer integration) a
durable per-session view that doesn't depend on `dmesg`-like ring
behavior or the writing process still being alive.

This PRD extends `~/wintermute/memlog/` with a new Rust binary
(`memlog-witness`) under `cli/` plus a small `src/persistence.rs`
module. The kernel module and existing `memlog show` cli are
untouched.

---

## 1. Why this exists

1. **The kernel ring is volatile by design.** Per `~/wintermute/memlog/README.md`,
   it is "a per-uid circular ring." If the user runs many Claude
   sessions in a day, older snapshots age out. The whole point of
   surviving process death is lost if the kernel ring overwrites
   before next-Claude reads.

2. **`memlog show` is interactive, not subscriber.** It reads
   recent records on each invocation. Nothing keeps a long-running
   subscriber up that drains the ring into stable per-session
   files.

3. **Per-session organization is the natural index.** Once snapshots
   live at `~/.claude/memlog/<session-id>/`, every downstream tool
   (postmortem, episode, letter-from-snapshot) gets a trivial
   lookup pattern. With `agentns-claude` (PRD #1) wrapping sessions,
   `session-id` is a primary-source kernel id; without it, fall back
   to the `comm:<name>:pid:<n>` form that the provfs LSM uses, so
   the path structure is stable across kernels.

---

## 2. What this builds

### 2.1 New binary: `memlog-witness`

```
memlog-witness daemon [--out <dir>] [--device <path>] [--quota <bytes>]
memlog-witness status
memlog-witness drain --session <id>   # ad-hoc flush
```

- `daemon` — long-running. `open("/dev/memlog", O_RDONLY)`, blocking
  reads in a loop, parse each record, look up
  `/proc/<writer-pid>/agent_session` (cache by pid), write to
  `<out>/<session-id>/snap-<seq>.json`. Default `<out>` is
  `~/.claude/memlog/`. Default `--device` is `/dev/memlog`.
- `--quota <bytes>` — per-session disk budget (default 100 MB).
  When exceeded, oldest snapshots in that session's directory are
  deleted; a `_quota-trimmed` marker file records what was lost.
- `status` — prints currently-open sessions, snapshot counts,
  bytes-on-disk per session.
- `drain --session <id>` — flush any in-flight record for that
  session to disk and fsync; for use just before reboot.

### 2.2 Per-session file layout

```
~/.claude/memlog/
└── <session-id>/                  agentns hex id, OR "comm:<n>:pid:<p>:uid:<u>"
    ├── intent_tag                 single line, written once at first snap
    ├── opened_at                  ISO timestamp
    ├── snap-00000.json            first snapshot
    ├── snap-00001.json            …
    └── _quota-trimmed              optional marker, see quota above
```

Snapshot files are JSON because `/dream` Phase 1.5 brief says
`memlog show --format json` is the existing reader format. The
witness uses the same schema (records, not a single snapshot per
session — each compaction event is its own snap).

### 2.3 Pevent integration

`memlog-witness daemon` is supervised by `pevent`. PRD ships an
example unit definition + an install snippet:

```
pevent add memlog-witness \
    --restart on-fail \
    --backoff 5s,30s,5m \
    --run "memlog-witness daemon"
```

The witness is single-instance per uid (file-lock on
`<out>/.witness.lock`); if a second instance starts it exits 0
quietly. That makes restart safe.

### 2.4 Resilience

- **Kernel ring overruns.** When `/dev/memlog` reports a missed
  record (the existing memlog driver has an overrun counter per
  kernel module README §0/1), `memlog-witness` writes a
  `_overrun-NNNN.json` sentinel into the most-recent session
  directory it knew about (best-effort attribution).
- **Crash mid-write.** Snapshot files are written `snap.tmp` →
  fsync → rename. Atomic; never partial.
- **Daemon crash.** pevent restarts; on startup the daemon reads
  the next-sequence-number from the highest-numbered file in each
  session dir.

---

## 3. Non-goals (v0.1)

- A query/search UI for snapshots. (`memlog show` covers ad-hoc
  reads; future tools join snapshots into postmortems.)
- Cross-uid aggregation. The kernel ring is per-uid; the witness
  inherits the limitation.
- Encryption-at-rest. Snapshots may contain prompt fragments. They
  live in `~/.claude/` (user-private). Encryption is a deliberate
  future add if needed.
- Cloud sync, remote shipping, S3, syslog. All explicit non-goals.

---

## 4. Acceptance criteria

1. **AC1 — Builds inside `~/wintermute/memlog/`.** `cargo build
   --release` in the memlog repo root produces
   `target/release/memlog-witness` in addition to existing
   artifacts. Lib tests stay green.
2. **AC2 — Single-instance lock.** Running `memlog-witness daemon`
   twice in parallel: second exits 0 with a "already running"
   stderr; first keeps running.
3. **AC3 — Fixture replay.** With `--device /tmp/fixture-memlog`
   pointing at a pre-recorded JSONL fixture (committed to the
   repo), the daemon reads it, writes per-session snapshots,
   matches the expected directory layout golden test.
4. **AC4 — Atomic writes.** Kill -9 the daemon mid-write
   (instrumented via a `MEMLOG_WITNESS_DELAY_MS` env var that
   sleeps before rename); confirm no partial `snap-*.json` files
   exist; only `snap.tmp` may be left, which is ignored on next
   start.
5. **AC5 — Quota trim.** Set `--quota 1K`, write 10 snaps of ~200B
   each; daemon trims oldest, writes `_quota-trimmed` marker, total
   on-disk stays under quota.
6. **AC6 [boot] — Live `/dev/memlog`.** Boot into linux-wintermute,
   start daemon, trigger a memlog write from a child process (via
   the existing `libmemlog` bindings), confirm a snap-*.json file
   appears under `~/.claude/memlog/<session-id>/` within 1s.
7. **AC7 [boot] — Session attribution.** Wrap a process with
   `agentns-claude` (PRD #1), have it write to memlog, confirm the
   session_id in the file path matches `agentns-claude --verbose`'s
   reported session_id.
8. **AC8 [boot] — Overrun marker.** Force a ring overrun by
   pausing the daemon with SIGSTOP while a producer floods the
   ring, SIGCONT; confirm an `_overrun-*.json` sentinel exists.
9. **AC9 — `status` subcommand.** `memlog-witness status` prints
   one line per session with id, snap count, bytes-on-disk; exit 0.
10. **AC10 — README + CHANGELOG.** Memlog repo `README.md` adds a
    "memlog-witness" section. `CHANGELOG.md` gets a v0.2.0 (or
    whatever the next minor is) entry. `~/wintermute/REPOS.md`
    untouched (rust-extend rule).

---

## 5. Shape

```
~/wintermute/memlog/
├── cli/
│   ├── memlog                   existing python show tool — untouched
│   └── memlog-witness/          new Rust subcrate (or top-level [[bin]] in libmemlog)
│       └── src/main.rs
├── libmemlog/
│   └── src/
│       ├── persistence.rs       new module: atomic snap-N writer
│       ├── lock.rs              new module: single-instance file lock
│       └── lib.rs               existing — pub-re-exports new modules
└── tests/
    ├── fixture/                 new dir
    │   └── memlog-replay.jsonl  pre-recorded device output
    └── persistence.rs           new integration test
```

Dependencies (new): `tokio` is overkill — use blocking std::io; the
device read is the only blocking op. `nix` for fcntl flock. `serde_json`
already in the workspace. `walkdir` for status.

---

## 6. Open questions

- The kernel module exposes records in some C struct over the char
  device. The existing `memlog show` python tool implies the
  serialization format already exists. `memlog-witness` should
  share that parser with `libmemlog` — confirm what's already in
  `libmemlog/src/` and reuse rather than re-implement.
- Should the witness pre-populate `intent_tag` from a SessionStart
  hook side-channel (`~/.claude/agentns-session-id` per PRD #1
  §2.3)? Or read it from `/proc/$PID/agent_intent_tag` per writer?
  Leaning the latter — same source as session_id, no side-channel.
- File-naming: monotonically increasing `snap-NNNNN.json` or
  `snap-<ts>.json`? Leaning monotonic for trivial sort; `ts` is
  inside the JSON anyway.

---

## 7. Provenance

- Kernel surface per `/dream` Phase 1.5 brief: `/dev/memlog`
  per-uid circular log, group `memlog`, records survive process
  death. `cli/memlog show --since 1h --format json` is the
  existing reader (per `~/wintermute/memlog/README.md`).
- Pain motivating per-session persistence: 2026-05-24 self-review
  run-1 journal — "Missing summary for
  claude-20260523T020109.ndjson (root_pid 60874 dead, SessionEnd
  hook never fired — reboot killed it mid-session)." A memlog
  snapshot written via this witness would have been there.
- Vision: visions/continuity.md, Fleet 1 PRD #3.
