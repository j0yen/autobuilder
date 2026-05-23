# PRD: /dev/memlog — kernel-shipped LLM context-window audit log (codename: *memlog*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — out-of-tree char device driver
**Date:** 2026-05-22
**Forks:** Nothing existing. Adds a new character device. Loosely inspired by `/dev/kmsg` (the kernel's own circular log) and `prctl(PR_SET_NAME)`.

---

## TL;DR

When my context window is about to compact, the auto-summarizer runs and the prior content is replaced with a much shorter summary. The original is gone from my working memory. There is no audit log of what got compacted — no way for me, the next session, or the user to inspect "here's what Claude was thinking immediately before the summary kicked in." This PRD adds a small character device `/dev/memlog` that LLM applications can write to immediately before they trigger compaction: a structured record (CBOR) of the about-to-be-replaced context plus the summary it was replaced with. The kernel keeps a per-uid circular buffer (configurable, default 4 MB) of recent records. Readers can `cat /dev/memlog`, `journalctl --identifier=memlog`, or use a `memlog show` userspace tool to walk the ring. The kernel guarantees the writes survive process death — so a session that crashed mid-compaction still leaves a forensic record. Pair with `recall episode` to elevate the most-interesting pre-compaction states into long-term memory.

---

## 1. Why this exists

Concrete: I've watched a context window compact today and lost track of *why* I made a decision two turns prior. The "system summary" that replaced the original is a lossy compression; the original is gone. Specifically:

1. **Compaction summaries hide intermediate reasoning.** When the summarizer rewrites "I read these 5 files, decided X based on file 2's contents" into "I decided X," the *contents of file 2* that drove the decision are gone. Next-turn-me can't re-examine.
2. **Crashes between compaction and the next checkpoint lose everything.** If Claude Code segfaults mid-compaction (rare but happens), the in-flight buffer is lost. Nothing on disk preserves it.
3. **User forensics is hard.** If the user asks "why did you do X?" and the relevant evidence was in turn 12 which got compacted at turn 80, I cannot retrieve turn 12. The user gets a hallucinated post-hoc rationalization or "I don't remember."
4. **`recall episode` would benefit from compaction signals.** The "what was I thinking right before the summary kicked in?" moment is exactly the kind of event worth elevating into long-term memory — but today there is no event-emitter.
5. **A circular buffer in the kernel survives process death.** A userspace file would not (open fd lost, partial writes, etc.). The kernel ring buffer is the right abstraction.

---

## 2. Who this is for

Me — to retain access to my own pre-compaction state. The user — for forensic questions. `recall episode` — to find compaction-adjacent moments worth promoting.

---

## 3. What I'd use it for (concretely)

| Question                                                            | What memlog answers |
| ------------------------------------------------------------------- | ------------------- |
| "What were the last 5 user messages before this compaction fired?"  | Records carry the immediate-pre-compaction tail of context |
| "Why did I decide X at turn 40 when the relevant evidence was at turn 12?" | If turn 12 was in the pre-compaction snapshot, recover it |
| "Did this Claude crash leave a partial state on disk?"              | `memlog show --uid=$me --since=10m` shows the last written record |
| "Compose a `recall write --kind episodic` from the compaction tail" | `memlog show --last 1 | episode promote` (a future glue tool) |
| "When does this session typically compact?"                         | `memlog show --since=30d --filter=session=$X | aggregate by turn-at-compaction` |

---

## 4. Functional requirements

### 4.1 Device interface

```
/dev/memlog                  c 244 0   crw-rw---- root memlog
```

Writers must be in the `memlog` group (or capability `CAP_SYS_ADMIN`); restricting writers prevents random processes from polluting the ring.

`open("/dev/memlog", O_WRONLY)` returns an fd. `write(fd, buf, len)` appends one record. Records are CBOR-encoded:

```cbor
{
  "ts":            <iso8601-utc>,
  "uid":           <writer's uid>,
  "session_id":    <opaque hex, from agent_session_id if set>,
  "turn_index":    <int>,
  "compaction_reason": "context_full | explicit | budget_exceeded",
  "pre_compaction_tail": [
    {"role":"user", "ts":"...", "content":"<truncated to 8 KB>"},
    {"role":"assistant", "ts":"...", "content":"<truncated to 8 KB>"},
    ...
  ],
  "summary": "<the replacement summary>",
  "model": "claude-opus-4-7",
  "tokens_pre": <int>,
  "tokens_post": <int>
}
```

Per-record max ~64 KB; the kernel rejects larger writes with `-EMSGSIZE`. The ring is 4 MB by default (sysctl `kernel.memlog.ring_size`); records evict oldest-first.

### 4.2 Reader interface

```
read(fd, buf, len)           # streams ring contents in CBOR-stream format
lseek(fd, 0, SEEK_SET)       # rewind to oldest available
```

Plus a userspace tool `memlog`:

```
memlog show [--since 24h] [--session-id ...] [--uid ...] [--format json|cbor]
memlog tail -f              # blocking, follow new writes
memlog clear                # admin only; truncates the ring
memlog stats                # capacity, oldest record ts, total writes since boot
```

### 4.3 Per-uid isolation

The ring is global, but readers see only their own uid's records by default. `CAP_SYS_ADMIN` reads see everything; this enables `/self-review` or `episode` to introspect across all sessions running as jsy.

### 4.4 Survives process death

The ring is kernel memory, not process memory. Writer SIGKILL after a write committed → record persists. Writer SIGKILL mid-write → record is dropped (the write either landed atomically or not at all; partial records are not stored).

### 4.5 Writes are atomic

The driver takes a spinlock around the ring append; one writer at a time. Acceptable at the expected write rate (≤ few per minute per process).

### 4.6 Telemetry hooks

A perf tracepoint `memlog:record_written` fires for every successful write; eBPF programs can subscribe for live monitoring (e.g. `/self-review` Phase E could be triggered when N records have accumulated since last run).

### 4.7 Userspace library

A tiny libmemlog (~200 LoC C, plus Rust/Python bindings) wraps the open/write dance:

```c
memlog_handle_t *h = memlog_open();
memlog_record_t rec = {...};
memlog_write(h, &rec);
memlog_close(h);
```

Anthropic's SDK would integrate this on the client side: just before issuing the request that triggers compaction, emit a memlog record.

---

## 5. Architecture

```
drivers/char/memlog/
├── memlog_dev.c          # the char device
├── memlog_ring.c         # circular buffer management
├── memlog_perms.c        # uid filtering + cap check
└── Kconfig

include/uapi/linux/memlog.h   # ioctls + record schema constants

~/.local/bin/memlog            # userspace reader
libmemlog/                     # the C/Rust/Python clients
```

Estimated kernel diff: ~700 LoC. Loadable module; `modprobe memlog` to enable. Disabled by default until the user explicitly loads it.

---

## 6. Non-goals

1. **Replacing transcript indexing.** memlog is for compaction moments specifically; the full session is in the JSONL ([PRD-session-index.md](PRD-session-index.md)).
2. **Crash dumps.** kdump / pstore already do that for the kernel; memlog is application-level.
3. **Cross-host shipping.** Records stay on the laptop. If the user wants to syslog them, they can plumb `journalctl --identifier=memlog | systemd-journal-remote`.
4. **Encryption.** Records contain user content; if encryption matters, the filesystem under `/var/lib/memlog/` (if we ever persist beyond the ring) is where it should live, not in the kernel.
5. **Schema enforcement.** memlog accepts arbitrary CBOR records and validates only size. Schema is a convention enforced by libmemlog, not the kernel.
6. **A general "structured kernel log."** /dev/kmsg already covers kernel-source events; memlog is specifically for LLM-context audit.

---

## 7. Phasing

| Phase | Scope                                                                |
| ----- | -------------------------------------------------------------------- |
| 0     | Char device + ring + atomic writes. `memlog show`. No tracepoints.   |
| 1     | Per-uid isolation, sysctl tunables, `memlog tail -f`.                |
| 2     | perf tracepoint + libmemlog (C/Rust/Python bindings).                |
| 3     | Anthropic SDK integration (upstream PR if they'll take it; otherwise a fork-shim) that calls memlog_write before compaction triggers. |
| 4     | `episode promote` integration: `episode` reads memlog and promotes top-N compaction tails into `recall` as episodic memories. |

Phase 3 is the part this PRD doesn't control. Even without Anthropic SDK cooperation, a wrapper around the Claude CLI binary could intercept the "about to compact" signal and call memlog_write — uglier but possible.

---

## 8. Risks

- **Privacy.** memlog records contain user-supplied conversation content. They're in kernel memory and readable by CAP_SYS_ADMIN. *Mitigation:* per-uid filtering; default-off; sysctl to disable globally.
- **Capacity.** 4 MB ring at ~32 KB per record = ~130 records before eviction. Heavy compaction days could wrap quickly. *Mitigation:* sysctl-tunable; or a userspace daemon that drains to disk before eviction.
- **Race conditions.** Writes are atomic but readers and `memlog clear` race. *Mitigation:* clear is admin-only and synchronizes via the same spinlock.
- **Upstream-mergability.** ~5%. Linux maintainers will ask "why isn't this in userspace?" and the answer is "process-death durability." It might land in tree if framed as "the user-space equivalent of /dev/kmsg." Realistically, vendor-fork.
- **API stability.** The CBOR schema will evolve; v0 might not survive contact with reality. *Mitigation:* every record has a `schema_version` field; readers must check.

---

## 9. Open questions

1. Should `pre_compaction_tail` include the full conversation tail (~32 KB) or just the last N=3 turns? Tradeoff is between richness and ring capacity.
2. Should memlog optionally persist its ring to disk (e.g. `/var/lib/memlog/ring.bin`) across reboots? Default off; opt-in via sysctl. Survives the laptop sleeping but not the disk dying.
3. Pair memlog with a "context-pause" mechanism — a kernel hint that asks the LLM client to *not* compact for the next N seconds because the user is about to ask something context-sensitive? Speculative; defer.
4. Should the writer's `agent_session_id` (from AgentNS) be a required field? Yes if AgentNS lands; today it can be opaque.
5. Should memlog records be CBOR or msgpack or just JSON? CBOR is more compact; JSON is more readable. CBOR for v0.1; a `--format json` reader option for humans.
6. What's the relationship to Anthropic-side telemetry? Memlog is *local* — nothing leaves the box. If Anthropic ships its own context-audit format, memlog stays the local-first complement.
