# PRD: agentns-session-receipt — turn the per-namespace counters into a session ledger

**Author:** Claude (Opus 4.8), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** [visions/signet.md](visions/signet.md)
**Depends on:** [PRD-agentns-doctor.md](PRD-agentns-doctor.md) shipped
**build_target:** rust-extend
**build_into:** `/home/jsy/wintermute/agentns-doctor`

---

## TL;DR

The wintermute kernel maintains seven per-namespace counters at
`/proc/$PID/agent_counters` — `total_syscalls`, `openat_count`,
`write_bytes`, `connect_count`, `unlink_count`, `fork_count`,
`elapsed_ns`. **No userspace tool reads them** (verified this session:
of eight `~/.local/bin/` tools, only `agentns-claude` touches the
surface, and only to *write* the session id; `procstat` reads cgroup
not agentns). This PRD extends `agentns-doctor` with a `receipt`
subcommand that snapshots those counters for a session, keyed by
`agent_session_id` + `intent_tag`, into a JSON ledger at
`~/.cache/agentns/receipts/<sid>.json` — a per-session resource record
joinable with ctrace's eBPF session histograms and recall's session
stamp.

This is the "now that identity is live, here's what it unlocks" layer.
It is honest about the precondition: in the init namespace every
counter is zero, so `receipt` produces meaningful data **only for a
wrapped session** (one launched through `agentns-claude`, per onramp's
`claude-agentns-wrap`). A `--require-wrapped` flag exits non-zero in
`init` state so it never litters zeros-receipts.

---

## 1. Why this exists

### 1.1 A live kernel surface with zero readers

Probed 2026-05-29 on kernel `7.0.10-arch1-5-wintermute`:

```
$ cat /proc/self/agent_counters
{ "total_syscalls": 0, "openat_count": 0, "write_bytes": 0,
  "connect_count": 0, "unlink_count": 0, "fork_count": 0,
  "elapsed_ns": 0 }
$ grep -rl agent_counters ~/.local/bin/    # → only agentns-claude
$ procstat --help                          # snap|self|watch; no agent axis
```

The kernel counts syscalls/writes/forks per agent namespace for free
(the counter hooks are wired — agentns README Phase 1 "done"), but the
tally is invisible. The doctor (`PRD-agentns-doctor.md`) made it
*readable*; this PRD makes it *durable and joinable*.

### 1.2 ctrace and agentns count the same session from opposite sides

ctrace observes a session's syscalls from *outside* via eBPF
(`~/.cache/ctrace/sessions/<id>.ndjson`); agentns counts them from
*inside* the namespace via the kernel's own per-ns hooks. The two are
independent measurements of the same session — a join on
`agent_session_id` lets each validate the other (and surfaces drift
if eBPF missed events). Today there is no agentns side to join. The
receipt creates it. (Complements scribe's hole-free ctrace record and
session-postmortem's multi-substrate join — see vision §Relationship.)

### 1.3 Session-stamped recall wants this too

`PRD-recall-session-stamp.md` (continuity Fleet 1) wants to stamp
memories with `agent_session`. A receipt keyed by the same id gives
those stamped memories a resource context ("this session that wrote
these 3 memories did 1.2M syscalls and 40MB of writes over 21h") —
the kind of episodic texture session-postmortem consumes.

---

## 2. What this builds

A new subcommand on the existing `agentns-doctor` binary (rust-extend,
minor version bump). Reuses the doctor's `/proc` reader and state
classifier.

### 2.1 `agentns-doctor receipt --emit [--pid <PID>] [--require-wrapped]`

1. Read `agent_session` (id), `agent_counters` (the seven fields), and
   the `intent_tag` (from `/proc/<pid>/agent_intent_tag` if present)
   for `<pid>` (default self).
2. Classify state via the doctor's existing logic. With
   `--require-wrapped`, exit `2` and write nothing when state is
   `init` or `absent` (so self-review can call it without producing
   zeros-receipts pre-wrap).
3. Write `~/.cache/agentns/receipts/<sid>.json` (dir created
   `0700`; mirrors ctrace's `~/.cache/ctrace/sessions/` layout):

```json
{
  "session_id": "a1b2...e9f0",
  "intent_tag": "/build",
  "pid": 325184,
  "state": "live",
  "counters": { "total_syscalls": 1203481, "openat_count": 88123,
    "write_bytes": 41203992, "connect_count": 91, "unlink_count": 2044,
    "fork_count": 312, "elapsed_ns": 75600000000000 },
  "emitted_at": "<rfc3339>",
  "schema": "agentns-receipt/1"
}
```

   Atomic write (temp + rename), like recall's atomic writes.

### 2.2 `agentns-doctor receipt --list [--format text|json]`

Lists receipts under `~/.cache/agentns/receipts/`, newest first:
`<sid-prefix>  <intent_tag>  syscalls=… write=…  <emitted_at>`.

### 2.3 `agentns-doctor receipt --show <sid>` / `--join-ctrace <sid>`

`--show` prints one receipt. `--join-ctrace <sid>` looks for a
matching `~/.cache/ctrace/sessions/*<sid-prefix>*` record and prints a
side-by-side of agentns counters vs the ctrace histogram totals
(syscalls, writes), flagging any >10% divergence as a note. If no
ctrace record matches, it says so and prints the receipt alone.

### 2.4 What this does NOT do

- Does not auto-emit on SessionEnd — emission is on-demand /
  pull-based in v0.1 (SessionEnd is unreliable for headless sessions,
  the SIGKILL-skips-hook problem scribe is fixing; vision Open Q).
- Does not write into recall or memlog — the join is read-only; it
  reads ctrace records but does not mutate them.
- Does not enforce or read budgets.
- Does not wrap anything (`agentns-claude` / `claude-agentns-wrap`).

---

## 3. Acceptance criteria

1. **`--require-wrapped` refuses init ns.** On this init-ns session,
   `agentns-doctor receipt --emit --require-wrapped` exits `2`, writes
   no file, and prints a one-line reason ("not wrapped; init ns").
2. **Emit writes a well-formed receipt.** With a `live` fixture (via
   the doctor's `--proc-root` hook, or a wrapped session), `receipt
   --emit` writes `~/.cache/agentns/receipts/<sid>.json` containing all
   keys in §2.1 with `schema":"agentns-receipt/1"`; the file parses as
   JSON and round-trips through `serde`.
3. **Atomic write.** A `receipt --emit` interrupted mid-write leaves no
   partial file (temp+rename verified by a test that checks no
   `*.tmp` survives and the target is either absent or complete).
4. **`--list` orders newest-first.** Three fixture receipts with
   distinct `emitted_at` list in descending time order in both `text`
   and `json` formats.
5. **`--show` round-trips.** `receipt --show <sid>` prints the same
   object that `--emit` wrote (byte-stable JSON for `--format json`).
6. **`--join-ctrace` matches and diverges.** Given a fixture receipt
   and a fixture ctrace ndjson for the same sid with syscall totals
   within 10%, the join prints "agreement"; with totals >10% apart it
   prints a divergence note. With no matching ctrace record it prints
   the receipt alone and says no ctrace match.
7. **Counters fidelity.** The seven counter fields in the emitted
   receipt equal the values read from `agent_counters` (init-ns
   fixture: all zero; live fixture: the fixture values).
8. **Dir perms.** `~/.cache/agentns/receipts/` is created `0700` if
   absent.
9. **Schema pinned.** A golden-file test pins the receipt key set so
   the ctrace/recall join contract doesn't silently break.
10. **`receipt --help`** documents `--emit`, `--list`, `--show`,
    `--join-ctrace`, `--require-wrapped`.

ACs 1, 3, 4, 5, 8, 9, 10 are today-testable (init ns + fixtures). ACs
2, 6, 7's *live* (non-zero counters) form is wrap-gated; their
fixture form (via `--proc-root` and synthetic ctrace files) is
today-testable. Declare the live half as `deferred_acs` if the fixture
half suffices for archive.

## 4. Bootstrap notes

- `rust-extend` into `~/wintermute/agentns-doctor` (minor bump). Reuses
  the doctor's `/proc` reader, classifier, and `--proc-root` test hook.
- Honest precondition: until a session is wrapped (onramp's
  `claude-agentns-wrap`), every real emit in `init` ns is a
  zeros-receipt — hence `--require-wrapped` as the default guard for
  any automated caller (self-review).
- Receipt location `~/.cache/agentns/receipts/<sid>.json` mirrors
  ctrace's `~/.cache/ctrace/sessions/` so the join is a sibling-dir
  glob, not a config lookup (vision Open Q: confirm before wiring).
- Pull-based emission (self-review calls `receipt --emit --pid <live-claude-pid>
  --require-wrapped`) is more robust than push-on-SessionEnd given the
  headless-SIGKILL-skips-hook problem; defer the trigger decision to
  Fleet 2.
