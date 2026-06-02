# PRD: agentns-doctor — name which agent-namespace state you are actually in

**Author:** Claude (Opus 4.8), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** [visions/signet.md](visions/signet.md)
**build_target:** rust-cli
**build_into:** (new repo) `/home/jsy/wintermute/agentns-doctor` → `j0yen/agentns-doctor`

---

## TL;DR

The wintermute kernel exposes a per-task agent-session signet at
`/proc/$PID/agent_session`, per-namespace counters at
`/proc/$PID/agent_counters`, and a namespace handle at
`/proc/$PID/ns/agent`. Three distinct system states produce three
distinct readings, and **nothing on this laptop tells them apart**:

1. **`absent`** — file missing → kernel has no `CLONE_NEWAGENT`
   (stock kernel).
2. **`init`** — file present, session reads 32 zeros, `ns/agent` is
   the init namespace → kernel is fine, the process simply was never
   `unshare`d. **This is expected and healthy**, not a fault.
3. **`live`** — session reads a non-zero 32-hex id → the process is in
   a fresh agent namespace; counters and `intent_tag` are meaningful.

`agentns-doctor` is a tiny read-only CLI that reads those `/proc`
surfaces and classifies the state, with a human `explain` mode and a
`counters` reader. It is the diagnostic that
[`PRD-claude-agentns-wrap.md`](PRD-claude-agentns-wrap.md) §Out-of-scope
explicitly deferred: *"A `claude-doctor` CLI to check namespace status
from outside. Could fold into onramp Fleet 2's onramp-doctor."*

---

## 1. Why this exists

### 1.1 Twenty self-review runs have mis-diagnosed a healthy kernel

`/self-review` has flagged `agentns` `/proc/self/agent_session`
all-zeros as the "lone broken kernel asset" for ~20 consecutive runs.
Verbatim from recall reflective `01KSS21WFN5H6V42JF723Z8K2J`
(run 19, 2026-05-28): *"agentns all-zeros ~20th run."* Earlier runs
(13-15, journal 2026-05-26) proposed "edit
`agorabus-session-start.sh` to unshare" — which `PRD-claude-agentns-wrap.md`
§1.2 proved is structurally impossible (`unshare` is per-process and
self-only; a hook can't enter a namespace on behalf of an
already-running `claude`).

The kernel is not broken. Probed live this session
(2026-05-29, kernel `7.0.10-arch1-5-wintermute`):

```
$ cat /proc/self/agent_session
00000000000000000000000000000000
$ cat /proc/self/agent_counters
{ "total_syscalls": 0, "openat_count": 0, "write_bytes": 0,
  "connect_count": 0, "unlink_count": 0, "fork_count": 0,
  "elapsed_ns": 0 }
$ zcat /proc/config.gz | grep AGENT
CONFIG_AGENT_NS=y
$ stat -L -c %i /proc/self/ns/agent
4026531996        # init-ns inode range (0xF0000000+)
```

All zeros is the *correct* reading of a process in the init agent
namespace. Nothing has wrapped the launch
(`PRD-claude-agentns-wrap.md` is the wrapper-routing PRD; it is
user-gated and not yet live). The defect is not in the kernel — it is
that **no tool distinguishes "unwrapped (expected)" from "broken."**

### 1.2 The self-review check itself only knows two states

`~/.claude/skills/self-review/SKILL.md:123-124`:

> **agentns**: `[ -f /proc/self/agent_session ]` and `cat
> /proc/self/agent_session`. If empty / file missing, **the namespace
> registration failed.**

The file is neither empty nor missing — it returns 32 zeros. There is
no branch for the present-but-all-zeros case, so a human (or Claude)
reading the output keeps concluding "broken." A machine-readable
tri-state verdict ends that. (`PRD-agentns-doctor-self-review.md`
consumes this CLI to fix the check; this PRD builds the CLI.)

### 1.3 The counters are a live surface no tool reads

`/proc/self/agent_counters` is valid JSON the kernel maintains per
namespace. Grepped this session: of the eight installed tools under
`~/.local/bin/`, **only `agentns-claude` references `agent_session` /
`agent_counters` — and it only *writes* the session id via `prctl`,
never reads the counters back.** `procstat` (the proc+cgroup JSON
tool) covers cgroup accounting but has zero `agent` references. So the
kernel's own per-session syscall/write/fork tally is currently
write-only-from-userspace. The doctor makes it readable; the receipt
PRD builds on that.

---

## 2. What this builds

A single small Rust binary, `agentns-doctor`, no async, no network.
Dependencies kept minimal: `clap` (derive) for args, `serde` +
`serde_json` for `--format json`. Reads only `/proc`; never writes,
never signals, never unshares.

### 2.1 `agentns-doctor status [--format text|json] [--pid <PID>]`

Reads (for `--pid`, default self):

- `/proc/<pid>/agent_session` — presence + value
- `/proc/<pid>/ns/agent` — inode via `stat`
- `/proc/<pid>/agent_session` all-zeros test

Classifies into exactly one `state`:

| state    | condition | meaning |
|----------|-----------|---------|
| `absent` | `agent_session` file does not exist | stock kernel, no `CLONE_NEWAGENT` |
| `init`   | file exists, value is 32 `0`s | kernel present, process unwrapped — **expected** |
| `live`   | file exists, value is non-zero 32-hex | wrapped; session id is meaningful |
| `malformed` | file exists but value is not 32 hex chars and not all-zero | genuine anomaly worth surfacing |

`--format text` (default) prints e.g.:

```
state:        init
session_id:   00000000000000000000000000000000
ns_inode:     4026531996
kernel:       CONFIG_AGENT_NS present
verdict:      Unwrapped — expected until launches route through agentns-claude.
```

`--format json` prints a stable object:

```json
{ "state": "init", "session_id": "0000...0000", "session_nonzero": false,
  "ns_inode": 4026531996, "intent_tag": null, "pid": 12345,
  "verdict": "unwrapped-expected" }
```

`verdict` is a short stable enum string (`unwrapped-expected`,
`wrapped`, `kernel-absent`, `malformed-surface`) so downstream shell
(self-review) can branch on it without parsing prose.

**Exit code:** `0` for `init` and `live` (both healthy); `0` for
`absent` by default (stock kernels are not a fault) but `2` when
`--expect-kernel` is passed (so a `-wintermute` kernel that somehow
lost the surface is caught); `3` for `malformed`.

### 2.2 `agentns-doctor counters [--pid <PID>] [--format text|json] [--delta <ms>]`

Reads and pretty-prints `/proc/<pid>/agent_counters`. With
`--delta <ms>`, samples twice `<ms>` apart and prints per-counter
deltas (rate observability for a wrapped session). In `init` state
all counters are zero and the output says so plainly (`note:
counters are per-namespace; init ns counters are always zero`).

### 2.3 `agentns-doctor explain [--pid <PID>]`

Prints one human paragraph for the current state — the
misdiagnosis-killer. For `init`:

> This process is in the **initial** agent namespace. The wintermute
> kernel (`CONFIG_AGENT_NS=y`) is present and working; the session id
> reads all-zeros because nothing called `unshare(CLONE_NEWAGENT)` on
> this process's launch path. This is expected and not a fault. To get
> a non-zero session id, route the launch through `agentns-claude`
> (see PRD-claude-agentns-wrap). A hook cannot fix this post-hoc —
> `unshare` is per-process and self-only.

### 2.4 What this does NOT do

- Does not unshare, set `intent_tag`, or set budgets — `agentns-claude`
  owns the *write* side; this is read-only.
- Does not restart, signal, or kill anything.
- Does not edit the self-review skill — that is
  `PRD-agentns-doctor-self-review.md`.
- Does not check memlog or provfs — those are onramp-doctor's other
  two thirds, out of scope here.
- Does not hardcode the init-ns inode as the classifier (see vision
  Open Q): classification is by `session == all-zeros AND file
  present`; the inode is reported but advisory.

---

## 3. Acceptance criteria

1. **Tri-state on init ns (today-testable).** Run on this stock-launch
   session: `agentns-doctor status --format json` emits
   `"state":"init"`, `"session_nonzero":false`,
   `"verdict":"unwrapped-expected"`, and a non-null integer
   `ns_inode`. Exit code `0`.
2. **`absent` classification.** With a mocked `/proc` view where
   `agent_session` is missing (point the reader at a temp dir via a
   `--proc-root <dir>` test hook, or unit-test the classifier
   function directly), `status` returns `state:absent`,
   `verdict:kernel-absent`, exit `0`; with `--expect-kernel`, exit `2`.
3. **`live` classification.** Given a fixture `agent_session`
   containing a non-zero 32-hex string (via `--proc-root` or unit
   test), `status` returns `state:live`, `session_nonzero:true`,
   `verdict:wrapped`, and echoes the 32-hex id. A wrapped-session
   smoke test using `agentns-claude --intent test -- agentns-doctor
   status` is the live form (deferred; boot/wrap-gated — see §5).
4. **`malformed` classification.** A fixture `agent_session` with a
   16-char or non-hex value yields `state:malformed`, exit `3`.
5. **`counters` reads and renders.** `agentns-doctor counters
   --format json` reproduces the seven kernel counter fields
   (`total_syscalls`, `openat_count`, `write_bytes`, `connect_count`,
   `unlink_count`, `fork_count`, `elapsed_ns`); on init ns all are `0`
   and a `note` field flags that init-ns counters are always zero.
6. **`counters --delta`.** Sampling twice produces a `delta` object
   with the same seven keys; on an idle init ns every delta is `0`.
7. **`explain` is stateful.** `explain` on init ns contains the
   substrings "initial agent namespace", "not a fault", and
   "agentns-claude"; on a `live` fixture it instead reports the session
   id and intent tag.
8. **Read-only guarantee.** Under `strace`/`ctrace` (or a unit-level
   assertion), no `write(2)` to any `/proc/*/agent_*` path, no
   `unshare`, no `kill`. (`ctrace` the test run; assert zero writes to
   `agent_session`/`agent_counters`.)
9. **`--help` and `--version`.** `agentns-doctor --help` lists
   `status`, `counters`, `explain`; `--version` prints `0.1.0`.
10. **Stable JSON schema.** The `status --format json` object keys are
    exactly `{state, session_id, session_nonzero, ns_inode, intent_tag,
    pid, verdict}` (a golden-file test pins the key set so downstream
    shell parsing doesn't break).

ACs 1, 2, 4, 5, 6, 7, 8, 9, 10 are today-testable (init ns + fixtures).
AC3's *live* form is wrap-gated and deferred per §5.

---

## 4. Out of scope (future signet fleets)

- Emitting a per-session receipt — `PRD-agentns-session-receipt.md`.
- Wiring into self-review — `PRD-agentns-doctor-self-review.md`.
- memlog/provfs checks (onramp-doctor's other thirds).
- Budget-status readout (`PR_GET_AGENT_COUNTERS` budget fields) —
  belongs with the budget-policy work in onramp/continuity Fleet 2.

## 5. Bootstrap notes

- New repo at `~/wintermute/agentns-doctor`, published `j0yen/agentns-doctor`.
- `--proc-root <dir>` test hook makes `absent`/`live`/`malformed`
  fixture-testable today without a wrapped session; default proc root
  is `/proc`.
- AC3's live verification (`agentns-claude … -- agentns-doctor status`
  returning `state:live`) is **wrap-gated**: it needs a real
  `unshare(CLONE_NEWAGENT)`, which works on this booted
  `-wintermute` kernel but requires the wrap path. Declare `deferred_acs`
  for the live half of AC3 if the today-testable fixture half is
  considered sufficient for archive; otherwise verify live in one
  session with `agentns-claude` installed (it is, at
  `~/.local/bin/agentns-claude`).
- Classify by value, not inode (vision Open Q on init-ns inode
  instability: `4026531996` this session vs `4026531837` recorded
  2026-05-27).
