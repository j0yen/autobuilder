# PRD: agentsh — a shell for agents, not humans (codename: *agentsh*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — vendor-fork of zsh, or a zsh plugin if v0.1 is allowed to stay above the line
**Date:** 2026-05-22
**Forks:** zsh (`Src/exec.c`, `Src/hist.c`, `Src/init.c`) plus a new `Src/agent.c`. Or, less ambitiously, a `~/.zshrc` plugin layer that captures most of the benefit without the fork.

---

## TL;DR

Every Bash tool call I make on this laptop goes through `zsh` because it's the user's login shell. zsh is good at being a shell for humans typing interactively: history file, tab completion, fuzzy matching, autocorrection, prompt themes, "did you mean…?" suggestions. None of that is appropriate when *I'm* the one invoking commands — I want strict quoting, JSON history, per-session isolation, and zero auto-correction. Today the situation is a mess: my Bash tool calls pollute `.zsh_history`; my `cd` inside a script affects the user's next interactive prompt; my command results go to stdout/stderr with no structured wrapper. `agentsh` is a small shell — either a zsh fork or a `$CLAUDE_TOOL`-aware mode toggled inside zsh — that flips every default the other way when invoked by an agent: structured history, no globbing surprises, no expansion of unquoted variables, per-session isolated state, exit codes mean what they say. The user's interactive zsh is untouched.

---

## 1. Why this exists

Things I've actually tripped on today:

1. **`.zsh_history` pollution.** Every Bash tool call I make appends to the user's shared history file. When they hit Up Arrow tonight they'll cycle through *my* commands, which they didn't type. This is rude and confusing.
2. **Implicit globbing.** I write `ls *.md` in a tool call; zsh expands `*.md` against whatever the cwd happens to be, including paths that may not exist. A failure-to-match returns an error or zero output silently depending on `nullglob`. Human zsh users tune this; an agent should *always* fail loudly on unmatched globs.
3. **Tilde expansion.** `~/foo` becomes `/home/jsy/foo` because zsh looked at `$HOME`. If I'm running in a sandbox or under a sub-uid, that expansion is wrong. Better: never expand `~` in agent mode; require explicit `$HOME` or absolute paths.
4. **Autocorrection.** `CORRECT` and `CORRECT_ALL` are off on this laptop, but on many users' machines a typo'd command silently runs a "did you mean…?" suggestion. Agent mode must refuse.
5. **History sharing across PIDs.** Two parallel Claude sessions writing to the same `.zsh_history` race. INC_APPEND_HISTORY half-mitigates; SHARE_HISTORY makes it worse. Agent mode wants a per-session history, isolated.
6. **`cd` leakage.** A script that does `cd /tmp && do_work` leaves the parent shell in `/tmp` if it's sourced. Even when not sourced, my expectation of "cwd is where I started" is occasionally violated by snapshots/shell-state hooks.
7. **The shell is a string-machine.** Commands and their args are flat strings; quoting bugs are a class of vulnerability. An agent ought to pass commands as structured arrays from the start (which is what `subprocess.run(args=[...])` does in Python), but the shell layer forces stringification.
8. **No structured output channel.** Today I parse stdout with `jq`/`awk`. An agent-targeted shell could expose a side-channel where commands emit structured records.

---

## 2. Who this is for

Me, when I'm in a Bash tool call. The user's interactive shell stays plain zsh; agent mode kicks in based on environment.

---

## 3. What I'd use it for (concretely)

| Today's footgun                                      | agentsh behavior |
| ---------------------------------------------------- | ---------------- |
| `.zsh_history` mixes me and the user                 | `$AGENT_HISTFILE` is per-session: `~/.cache/agentsh/<session_id>.jsonl`. `.zsh_history` is untouched. |
| `ls *.md` silently no-ops if nothing matches         | `setopt FAILGLOB` (the agent default) — unmatched glob → exit 1 |
| `cd $TMPDIR/foo` if TMPDIR unset → `cd /foo` (root)  | `setopt NO_UNSET` (default in agent mode) — unset var → error |
| `rm $1/*` if `$1` is empty → `rm /*`                 | Same; refuse on unset                                            |
| `cmd "$varwithspaces"` works; `cmd $varwithspaces` splits | Agent mode treats unquoted vars as quoted by default (controversial; opt-in) |
| Two parallel Claude Bash calls clobber history       | Per-session isolated history file                               |
| `command_that_does_not_exist` produces a long error  | Single-line "no such command" — no autocorrection lookups       |
| User-friendly aliases (`l=ls -la`) apply to my calls | Aliases are off in agent mode unless `--enable-alias`             |

---

## 4. Functional requirements

### 4.1 Mode detection

Two paths, pick one:

**(a) Plugin (v0.1, realistic)**: a `~/.zshrc.d/agentsh.zsh` snippet that, if `$CLAUDE_TOOL` is set, applies the agent setopt block and rebinds key behaviors.

**(b) Fork (v0.2, ambitious)**: a separate binary `agentsh` (zsh source forked) that the harness invokes instead of `zsh -c` for tool calls. The user's interactive shell never loads this codepath.

I lean toward (a) for the first 90% — the setopt surface in zsh is rich enough to express most of these — and reserve (b) for the parts that can't be plugin'd (per-session history isolation, structured output channel).

### 4.2 Agent-mode setopts

```zsh
setopt FAILGLOB                # unmatched globs error
setopt NO_NOMATCH              # complement
setopt NO_UNSET                # unset var → error (a.k.a. `set -u`)
setopt PIPEFAIL                # pipeline exit = rightmost non-zero
setopt NO_AUTO_CD              # `dir` alone doesn't cd there
setopt NO_CORRECT              # no spell-correction
setopt NO_CORRECT_ALL
setopt NO_HIST_VERIFY          # never prompt
setopt NO_SHARE_HISTORY
setopt NO_INC_APPEND_HISTORY   # don't touch shared history
setopt NO_BANG_HIST            # don't expand `!!` etc.
setopt NO_RM_STAR_SILENT       # `rm *` doesn't prompt (we're non-interactive)
setopt NO_BEEP
unalias -m '*'                 # nuke aliases by default
```

### 4.3 Per-session history

When `$CLAUDE_TOOL` is set:

```zsh
HISTFILE=${CLAUDE_AGENT_HISTFILE:-~/.cache/agentsh/$CLAUDE_SESSION_ID.jsonl}
HIST_FORMAT=jsonl    # NEW — fork required
```

The fork's `Src/hist.c` writes one JSON record per command:

```json
{"ts":"2026-05-22T17:46:31Z","cwd":"/home/jsy/projects/recall","argv":["recall","query","foo"],"exit":0,"stdout_bytes":1248,"stderr_bytes":0,"duration_ms":42}
```

Plugin (v0.1) fallback: an `add-zsh-hook preexec` writes a JSON line to the file manually before each command. Less precise (no exit code in preexec) but mostly works.

### 4.4 Structured output channel

In the fork: a third fd (fd 3) is opened to `$CLAUDE_STRUCTURED_FD` if set. Commands that want to emit structured records can write to fd 3. `agentsh` collects fd 3 contents and surfaces them as a JSON array alongside the regular stdout/stderr.

Plugin fallback: not available. fd 3 plumbing requires shell-builtin awareness.

### 4.5 Argv arrays via JSON

`agentsh --argv '[":/bin/ls", "-la", "/home/jsy"]'` accepts a JSON-encoded argv. No shell quoting, no string parsing. The tool harness passes commands this way to bypass the quoting gauntlet entirely.

### 4.6 Per-session env scoping

`agentsh` runs every command inside an env scope where:
- `$HOME` is set to the user's home
- `$PWD` is whatever the caller passed in
- `$CLAUDE_*` env vars are inherited
- Everything else from the parent env is *filtered* by default (a small allowlist: `PATH`, `LANG`, `TERM`, `SHELL`)

The allowlist is configurable. Per-call overrides via `--env KEY=VAL`.

### 4.7 Backward compat with non-agent invocations

If `$CLAUDE_TOOL` is *not* set, `agentsh` (and the plugin) behave identically to vanilla zsh. The user's interactive prompt loads the plugin but the setopts only fire under the env var.

---

## 5. Architecture

**v0.1 (plugin):**

```
~/.zshrc.d/agentsh.zsh         # source of truth for the plugin
~/.cache/agentsh/              # per-session histories
~/.local/bin/agentsh-replay    # tail/inspect a session history
~/.local/bin/agentsh-stats     # aggregate by command/exit/duration
```

**v0.2 (fork):**

```
~/zsh-fork/                    # vendor fork of zsh
├── Src/agent.c                # NEW — agent-mode logic
├── Src/hist.c                 # MODIFIED — JSONL history format
├── Src/exec.c                 # MODIFIED — fd 3 structured channel
├── Src/init.c                 # MODIFIED — env scoping
└── Completion/                # AS-IS — completion is for humans anyway
```

Estimated fork diff: ~1500 LoC. zsh is a careful codebase; staying inside the existing setopt and module infrastructure is preferable to a hard fork.

---

## 6. Non-goals

1. Replacing the user's interactive shell. Agent mode is opt-in via env.
2. Tab-completion, prompt themes, vi-mode, etc. Agent mode doesn't need any of it.
3. Cross-shell portability. agentsh is zsh-shaped; bash users would need a parallel fork.
4. Sandboxing / containerization. Agent mode doesn't enforce filesystem boundaries; pair with `sbx` for that.
5. Replacing `subprocess.run` in Python or the equivalent in other languages. agentsh is for the cases where a shell is in the path (the Bash tool, hook scripts, the SessionStart hook). Where Python or Rust can call exec directly, they should.

---

## 7. Phasing

| Phase | Scope                                                              |
| ----- | ------------------------------------------------------------------ |
| 0     | Plugin: setopts + per-session HISTFILE + preexec JSON-line history |
| 1     | `agentsh-replay`, `agentsh-stats` over the new history format       |
| 2     | Per-session env scoping (still plugin)                              |
| 3     | Fork: real JSONL history format with exit codes + durations         |
| 4     | Fork: fd 3 structured output channel + `--argv json` mode           |

v0.1 (Phase 0–2) is genuinely a weekend project. v0.2 (Phase 3–4) is the audacious half.

---

## 8. Risks

- **zsh upstream divergence.** Maintaining the fork has cost. *Mitigation:* keep the diff small and stable; rebase quarterly.
- **Plugin path subtlety.** Sourcing the plugin from `.zshrc` might not fire for non-login shells. The Bash tool harness uses `zsh -c "..."` which sources `.zshenv` not `.zshrc` by default — needs verification.
- **Per-session history fragmentation.** After a year, `~/.cache/agentsh/` has thousands of session files. *Mitigation:* monthly rollup; `agentsh-stats --rollup` produces `2026-05.jsonl` and removes the per-session files.
- **NO_UNSET breaks half my shell snippets.** Every existing shell idiom that does `${VAR:-default}` is fine; every one that does `$VAR` unquoted breaks loudly. *Mitigation:* phase in by command — `$CLAUDE_TOOL_STRICT=1` is opt-in initially.

---

## 9. Open questions

1. Should agentsh emit OpenTelemetry-style spans for each command, on top of (or instead of) JSONL history? Tracing back to which Claude turn invoked which subprocess is useful — see [PRD-attribution-timeline.md](PRD-attribution-timeline.md).
2. fd 3 is conventional for "structured output"; should we instead use a named pipe at `$CLAUDE_STRUCTURED_SOCK`? UDS gives bidirectional, fd is one-way.
3. Should `agentsh` integrate with `spool` to log every Skill invocation as a structured event? Probably yes — agentsh-history *is* the natural ledger for skill-telemetry's data.
4. fish or nu shell as an alternative base? They have structured I/O already. The downside is the user's interactive shell is zsh, so the fork must coexist.
5. Should agent mode disable PATH-search and require absolute paths? Belt-and-suspenders; potentially too strict. Defer.
