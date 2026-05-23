# PRD: Attribution-Aware Filesystem Timeline (codename: *fsstory*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1
**Date:** 2026-05-22
**Builds on:** `ctrace` (PID-attributed syscall events), `wchg` (FS-change deltas). Neither alone answers the question this PRD targets.

---

## TL;DR

When I look at a file, I want to know: who last touched it, when, with what intent, and through what tool. Today I can run `stat <path>` (mtime, owner) and `git log -- <path>` (committed changes). Neither tells me: *was the last write me, the user, or a background process? Through which Claude session? Via Edit, Write, or a Bash side-effect?* `ctrace` knows the PID and `comm` of every write; `wchg` knows what changed since when. `fsstory` joins them: a per-path timeline of every write since N hours/days ago, each event attributed to an actor (claude-session-id | user-interactive | tool:<name> | package-manager:<name> | system | unknown). The output answers "did this change underneath me?" and "is this state mine or theirs?" — questions I currently guess at.

---

## 1. Why this exists

Concrete moments where I'd benefit:

1. **A config file looks different than I remember.** Did the user edit it? Did `pacman -Syu` overwrite it? Did I write it last session and forget? Today the only honest answer is "I don't know." `stat` says when, not who-or-why.
2. **A test starts failing after a tool I ran.** Was it my edit? Was it a side effect of `cargo build` regenerating a lockfile? `git diff` shows the change but not the *cause*.
3. **`/self-review` Phase A says "X files changed under ~/.claude."** It does not say *which actor* changed them. Many are session JSONLs that the Claude binary wrote about itself. Some might be user-edited skill files. The skill currently can't differentiate.
4. **Trust calibration.** If I read a file and it's been ≥ 7 days since *I* touched it and the user has touched it since, I should re-read carefully. If I just wrote it 5 minutes ago and nothing else has touched it, I can trust my mental model. There's no signal for this today.
5. **Forensics after an unexpected change.** When the autobuilder reviewer-agent flagged unauthorized drift on `agent/intent-card.json`, the chain of "what process modified that file when" took me three Bash commands to reconstruct. It should be one.

---

## 2. Who this is for

Me. The output is consumed by:
- My own reasoning ("can I trust this file's contents to be what I last set them to?")
- `/self-review` Phase A (replace "N files changed" with "N files changed: M by me, K by the user, J by package manager")
- Possibly debugging when the user asks "did you change this?"

---

## 3. What I would use it for (concretely)

| Question I'd ask                                                              | `fsstory` answer |
| ----------------------------------------------------------------------------- | ---------------- |
| "Who last touched `~/.claude/settings.json`?"                                 | "2026-05-22 16:48 Claude session `df04d4…` via `Skill(update-config)` (high confidence)" |
| "What did *I* change under `~/.claude/skills/self-review/` in the last hour?" | List of 3 paths with the Edit/Write tool calls that produced each, plus surrounding session turn references |
| "Did anything change `agent/intent-card.json` outside autobuilder?"           | "No — only autobuilder commits, both via `git` at 16:46" |
| "Has the user been active in `~/projects/recall/` while I was working?"      | "No interactive `vim`/`code`/`nvim` writes; all changes attributed to claude or git" |
| `/self-review` Phase A: change-set summary                                    | "12 files changed in ~/.claude. 11 by claude (8 session-JSONL self-writes, 3 file-history snapshots). 1 by user (`settings.json:hooks`). 0 by package manager." |

---

## 4. Functional requirements

### 4.1 Data sources

`fsstory` is read-only joiner. It does not produce events; it consumes them.

| Source                                  | Provides                                              |
| --------------------------------------- | ----------------------------------------------------- |
| `ctrace` ndjson logs (when active)      | per-PID `openat`(write)/`unlinkat` events with `comm`, `file`, `pid`, `ppid` |
| `~/.claude/projects/*/[uuid].jsonl`     | which session's Edit/Write tool calls hit which path (turn-level attribution) |
| journald (`journalctl --user`)          | systemd-user unit activity (e.g. timers, services)    |
| pacman log (`/var/log/pacman.log`)      | package-manager writes (installs, upgrades, removals) |
| `stat` + path heuristics (last-resort)  | unattributed events |

### 4.2 Attribution actors

Each event resolves to one of:

```
claude-session:<jsonl-basename>:<turn-index>     # Edit, Write, NotebookEdit, or recorded MCP tool
claude-bash:<jsonl-basename>:<turn-index>        # Bash tool call (side effects)
user-interactive                                  # PID's comm matches vim/nvim/code/zed/etc.; no claude ancestor
pacman | makepkg | yay                            # package manager
systemd:<unit>                                    # background user service
fastembed | recall | ctrace | wchg | ...          # named local tool with claude ancestor: still claude-bash, parent attribution
unknown                                           # ctrace had no event AND no other source attributes it
```

Each attribution carries a confidence score:
- **high**: ctrace event + jsonl tool-call match (Edit/Write with this path in the same window)
- **medium**: ctrace event with `comm` that matches a known actor and a plausible time window
- **low**: only `stat` mtime; no ctrace event covers it (e.g. ctrace was off)

### 4.3 CLI surface

```
fsstory path <path> [--since 24h] [--format json|text]
fsstory ls <dir> [--since 24h] [--by-actor]
fsstory summary [--since 24h] [--root /home/jsy/.claude]
fsstory who-wrote <path>           # latest event only
fsstory diff <path> [--since 24h]  # event chain + per-event diff snippets if git is available
```

Output shape (JSON) for `fsstory path <path>`:

```json
{
  "path": "/home/jsy/.claude/settings.json",
  "events": [
    {
      "ts": "2026-05-22T23:48:04Z",
      "actor": "claude-session:df04d4...:33",
      "via": "Skill(update-config)",
      "op": "write",
      "size_delta_bytes": 412,
      "confidence": "high",
      "evidence": {
        "ctrace_log": "/home/jsy/.cache/ctrace/sessions/claude-20260522T173251.ndjson",
        "ctrace_ts": 9384721,
        "jsonl_session": "df04d4-...jsonl",
        "jsonl_turn": 33
      }
    },
    ...
  ]
}
```

### 4.4 `/self-review` integration

A new field in Phase A's snapshot block, replacing the current "N files changed since last run":

```
~/.claude: 30M · 12 files changed since last run
  by me (claude):       11 (8 session-JSONL self-writes, 3 file-history)
  by user-interactive:   1 (.claude/settings.json — hooks block edited)
  by package-manager:    0
  by unknown:            0
```

Computed by piping wchg's file list through `fsstory who-wrote --batch`.

### 4.5 Graceful degradation when ctrace is off

If ctrace is not running (or its logs are stale), confidence drops to `low` and the event list falls back to `stat` mtime + path-heuristics only. `fsstory` should still produce *something* useful; it never errors solely because ctrace wasn't on.

---

## 5. Architecture

Single binary, `~/.local/bin/fsstory`. Pure Rust; reads ndjson and JSONL with streaming parsers.

No persistent index — the data sources (`~/.cache/ctrace/sessions/*.ndjson`, `~/.claude/projects/*.jsonl`, pacman log) are themselves the index. `fsstory` is a *query* tool over those, not a store. A per-query in-memory join is fine at the scale of one day's data; for week+ queries an optional SQLite-backed cache at `~/.cache/fsstory/index.sqlite` can be enabled via `fsstory index --since 30d`.

Three core modules:
- `sources/` — ctrace, jsonl, pacman, journald, stat fallback. Each yields `RawEvent`.
- `attributor.rs` — joins RawEvents from different sources into Attributed events with confidence.
- `query.rs` — CLI handlers.

---

## 6. Non-goals

1. Modifying any of the data sources. Strictly read-only.
2. Real-time stream. Snapshot-on-demand only.
3. Cross-host attribution. Single-laptop.
4. Inferring *intent* beyond what the source provides. If the user's Edit message turn says "fix the typo," we surface that; we don't infer purposes from diffs.
5. Replacing `git blame`. For tracked repo files, `fsstory` can call into `git log -- <path>` for commit-level attribution and merge it in, but it doesn't reimplement git's algorithms.
6. Detecting tampering. If the user manually edits a JSONL or ctrace log, `fsstory` reports what's in the files; trust is the user's problem.

---

## 7. Phasing

| Phase | Scope                                                                                 |
| ----- | ------------------------------------------------------------------------------------- |
| 0     | `sources/ctrace`, `sources/jsonl`, `attributor` (claude-session + claude-bash + unknown), `fsstory path` + `who-wrote` |
| 1     | Add `sources/pacman`, `sources/stat`, `sources/journald`. Add `ls` + `summary` + `diff`. |
| 2     | `/self-review` Phase A integration: replace the "N files changed" line with the by-actor breakdown. |
| 3     | Optional SQLite cache for week+ queries. `fsstory index --since 30d` builds it; subsequent queries hit the cache. |

---

## 8. Risks

- **ctrace gaps mean attribution gaps.** When ctrace was off (boot before SessionStart hook fired, or a leaked-tracer-was-reaped period), events are confidence=low. *Mitigation:* surface confidence in every result; never claim high confidence on stat-only.
- **JSONL turn boundaries are fuzzy.** A `Bash` tool call might touch many files; matching ctrace events to a turn requires a time window. *Mitigation:* attribution is a (PID, time-window, tool-name) match — accept that some Bash-induced writes attribute to the *bash invocation*, not a specific file the user named.
- **Privacy.** Like recall and transcript, `fsstory` reads from places that contain user prompts and bodies. Same local-only invariant.
- **Performance on hot paths.** A query for "everything under ~/" with --since=30d could be slow. *Mitigation:* default `--since 24h`; the SQLite cache lands in Phase 3.

---

## 9. Open questions

1. Is there a useful "actor: me-the-current-session" vs "actor: other-claude-session" split? When I run `fsstory`, I am *some* claude session — should events from my session look different than events from siblings? Probably yes (I'd treat my own as more trusted).
2. Should `fsstory who-wrote` be exposed as an MCP tool so I can call it inline during work, not just at /self-review time? Tempting but it makes the agent slower.
3. Pacman attribution: each pacman transaction modifies hundreds of files. Should they collapse into a single "pacman -Syu 2026-05-22" event in the timeline, or list all the files? Probably collapse, with a `--expand` flag.
4. Should `fsstory` learn over time which `comm` strings map to which actor categories? Today the mapping is hardcoded (vim/nvim/code → user-interactive). A small learned classifier could help, but adds complexity for marginal lift.
