# PRD: Cross-Session Bus — IPC between concurrent Claude sessions (codename: *agorabus*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — Unix-socket pub/sub + read-only state
**Date:** 2026-05-22
**Inspired by:** today's `/self-review` flagged PIDs 1202 and 1381 — both live Claude sessions on this laptop, mutually blind, both probably about to do similar things.

---

## TL;DR

Two Claude sessions on the same laptop today cannot see each other. The user has 1202 in one terminal and 1381 in another. If 1381 is about to write `~/.claude/settings.json` and 1202 already did, they'll clobber each other; if 1202 is researching a question 1381 just answered, the second answer is redundant. `agorabus` is a tiny local pub/sub bus over a Unix socket (`~/.cache/agorabus/sock`) that every Claude session connects to at SessionStart. Each session broadcasts low-volume presence and intent ("I'm session X, working in `~/projects/recall`, currently editing `src/main.rs`"); each session can subscribe and read others' presence. Writes between sessions require explicit cooperation from the receiver. Read-only by default; rich enough that "are any of my peers already working on this file?" becomes a real question I can answer.

---

## 1. Why this exists

Concrete observations:

1. **Today's two Claude PIDs** (1202 in the autobuilder discussion, 1381 in a different window) had no way to coordinate. The user noticed in `/self-review`; I noticed only because the skill listed them.
2. **Shared resources race.** `~/.claude/settings.json`, recall's SQLite DB, memory MEMORY.md files — all mutated by potentially-many sessions. WAL and `txn-edit` ameliorate the data race; the semantic race ("we both decided to update the same memory differently") is unaddressed.
3. **Cross-session questions go unasked.** "Has any other session encountered this error?" is answerable only after the fact, via `transcript` post-hoc. Live, I can't ask.
4. **Multi-agent work has no substrate.** When I spawn sub-agents via the Agent tool, they're isolated. There's no "agent A asks agent B for a hint" lateral channel; A returns control to me, I summarize, B never runs in parallel with anything else.
5. **The user might want to see what I'm doing.** `agorabus tail` would be a clean way for the user to watch in real-time what each session is currently working on, without `ps`-and-grep gymnastics.

---

## 2. Who this is for

Me — to coordinate with my peers (other live Claude sessions, sub-agents). The user — for live visibility into multiple sessions running on their laptop.

---

## 3. What I'd use it for (concretely)

| Today                                                         | With agorabus |
| ------------------------------------------------------------- | ------------- |
| Two sessions both about to edit `settings.json`                | First session announces "lock-hint: ~/.claude/settings.json"; second sees it and defers or asks |
| User asks "what's the other Claude doing right now?"          | `agorabus peers` returns: `1202: cwd=~/projects/recall, last-tool=Bash (recall query), idle 4s` |
| Spawning an Agent sub-agent for an independent research task   | Sub-agent registers on the bus with its parent's session id; user can watch progress |
| Long-running operation in session A; session B wants to know when it's done | A publishes `done: <task-id>`; B's subscribe catches it |
| Pattern detection: "this is the third session today where Claude hit error X" | Bus aggregates published "error_encountered" events; surfaces via `/self-review` |

---

## 4. Functional requirements

### 4.1 Bus surface

Unix socket at `~/.cache/agorabus/sock`. Connection per session. JSON line protocol:

```
→ {"op":"announce","session_id":"01KS...","pid":1202,"cwd":"/home/jsy/projects/recall","intent":"work on PRD"}
← {"ok":true}
```

Operations:

| op            | semantics |
| ------------- | --------- |
| `announce`    | declare presence on connect; required first message |
| `update`      | update any field of my announce record (cwd changed, intent changed) |
| `publish`     | broadcast an event on a topic; persisted in ring buffer |
| `subscribe`   | receive events on a topic prefix; streamed back |
| `peers`       | list current connected sessions and their last-announce records |
| `query-state` | request "what's the most recent value of X published by anyone?" |
| `send`        | targeted message to another session id; receiver must opt in |

### 4.2 Topic conventions

```
session.<session_id>.presence       # heartbeat + last-tool
session.<session_id>.intent         # current declared task
session.<session_id>.error          # exceptions and unhappy paths
shared.lock-hint                    # advisory locks
shared.discovery                    # "I've found X" announcements
agent.<parent>.child.<child>.*      # sub-agent traffic
user.broadcast                      # user can publish a message to all sessions
```

Topic strings are dotted; subscribers can subscribe to prefixes (`session.*.error`).

### 4.3 Heartbeat

Every active session sends a 1-line heartbeat every 10 seconds (current-tool, current-file). The bus tracks last-heartbeat per session; sessions with no heartbeat for >60s are considered gone (no announce-end required; clean detection of crashes).

### 4.4 Persistence

The bus keeps a rolling log of all published events in `~/.cache/agorabus/log/YYYY-MM-DD.ndjson`. Late-joiners can `subscribe --replay <topic> --since <duration>` to catch up.

### 4.5 Auth

All connections are over a UDS with file perms 0600. Only the owning uid can connect. No further auth needed in v0.1 — single-user model holds.

For `send` (targeted messages), the receiving session must have `subscribe`'d to `session.<their-id>.inbox` and explicitly accept the sender. The bus rejects sends to non-subscribed inboxes.

### 4.6 Bus daemon vs sessionless mode

`agorabus` runs as a tiny background daemon (~300 LoC Rust) launched on demand. First session to connect starts the daemon if not present; daemon exits when no sessions are connected for >5 minutes.

systemd-user-socket activation is a clean alternative. v0.1 ships both modes; user picks.

### 4.7 Client integration

A SessionStart hook (`~/.claude/scripts/agorabus-join.sh`) calls `agorabus announce ...` and starts the heartbeat loop in the background. Stop-hook cleanly disconnects.

A small set of `agorabus` CLI verbs is exposed to me for in-session use:

```
agorabus peers                       # see who else is here
agorabus publish <topic> <data>      # publish from a script
agorabus subscribe <topic-prefix>    # blocking stream for tool consumption
agorabus query-state shared.lock-hint <path>   # advisory lock check
```

---

## 5. Architecture

```
~/.local/bin/agorabus                # CLI client
~/.local/bin/agorabusd               # daemon (also embedded in agorabus client as `agorabus daemon`)
~/.cache/agorabus/
├── sock                             # the UDS
├── state.json                       # current peers + their announce records
└── log/YYYY-MM-DD.ndjson            # event log

~/.claude/scripts/agorabus-join.sh   # SessionStart hook
~/.claude/scripts/agorabus-leave.sh  # Stop hook
```

Rust daemon, ~500 LoC. tokio + a tiny pub/sub state machine. SQLite for the persistent log if perf dictates; ndjson is fine for v0.1.

---

## 6. Non-goals

1. **Cross-host federation.** Single-laptop. If the user wants multi-machine, that's a different problem.
2. **Strong consistency.** This is an advisory bus. Two sessions can both think they hold a lock-hint on a file; the underlying filesystem doesn't actually enforce anything. Pair with real fs locks (`flock`) for real safety.
3. **General-purpose RPC.** No request/response semantics beyond `query-state`. If two sessions need a real RPC, build it on top.
4. **Replacing systemd/dbus.** agorabus is opinionated for agent traffic; dbus is the generic Linux IPC.
5. **Censorship.** Any session can publish anything to any non-targeted topic. Trust model is "all sessions belong to the same user."

---

## 7. Phasing

| Phase | Scope                                                                |
| ----- | -------------------------------------------------------------------- |
| 0     | Daemon + UDS + `announce`/`peers`/heartbeat. SessionStart hook joins. |
| 1     | `publish`/`subscribe` with topic prefix matching. Ring buffer log.   |
| 2     | `query-state` + `send` (targeted with opt-in receive).               |
| 3     | Sub-agent integration: when I spawn an Agent, it inherits a child session_id and joins as `agent.<my-id>.child.<theirs>`. |
| 4     | `/self-review` integration: detect "the same error was published by N sessions this week" → flag for memory. |

---

## 8. Risks

- **Bus daemon failure.** If `agorabusd` crashes, sessions can't coordinate. *Mitigation:* fail-open — every CLI client treats `agorabus peers` failing as "no peers" and proceeds normally. No session blocks on the bus.
- **Stale presence.** A session may crash without sending Stop-hook. Heartbeat timeout (60s) handles this; UI clearly shows "stale" peers.
- **Privacy.** The bus log carries intent strings ("working on PRD-agentic-memory") and cwd paths. Same single-user trust model as recall.
- **Coordination overhead.** Every tool call → bus publish → other sessions see → maybe respond. If sessions chatter too much, it's noise. *Mitigation:* heartbeat carries minimal info (tool name only, not args); higher-fidelity publishes are explicit.

---

## 9. Open questions

1. Should agorabus be the substrate for `Skill(...)` dispatch — i.e. invoking a skill is published to the bus and other sessions can observe/intercept? Probably yes, but folds into [PRD-skill-manifest.md] composition story.
2. Should the user's terminal multiplexer (zellij, tmux) integrate? `agorabus peers` in a tmux status bar would be a nice ambient signal. Possibly via a small status-bar plugin.
3. Should `send` to a peer trigger a notification on the peer's terminal? Distracting; opt-in only.
4. What about cross-vendor agents — Codex, Cursor, ollama-driven scripts? If they speak the same protocol, fine. v0.1 doesn't make this a goal but doesn't prevent it.
5. Per-topic ACLs (some topics user-only, some session-only)? Probably overkill for single-user.
