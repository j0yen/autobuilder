# PRD-chord-async-delegate

Status: Shipped v1.0 (2026-05-29)
build_auto: false
build_target: shell
build_into: /home/jsy/.claude/scripts
Vision: visions/chord.md

## TL;DR

Replace the synchronous `delegate.run` RPC method in
`~/.claude/scripts/agorabus-worker.sh` with an **async ticket
pattern**: `delegate.start` returns a ticket immediately, the worker
runs the delegated `claude --print` in a backgrounded subprocess, and
caller subscribes to `delegate.result.<ticket>` for completion. Keeps
`delegate.run` available as a thin wrapper for back-compat. Removes
both the 300s timeout pressure and the head-of-line blocking that
currently serializes all RPCs while a delegation is in flight.

## Why this exists

The existing `delegate.run` (`agorabus-worker.sh` lines 106-160) does:

```bash
out=$(cd "$cwd" && \
    AGORABUS_DELEGATE_DEPTH=… \
    timeout "${timeout_secs}s" claude --print … "$prompt" 2>&1)
```

— a blocking command-substitution inside the worker's main dispatch
loop. Three real problems:

1. **Head-of-line blocking.** While a `delegate.run` is in flight,
   *no other RPC* (not even `ping`) to this session is processed.
   The worker's `while read line` loop is stuck on the
   `command-substitution`.
2. **Timeout cap.** Default 300s (`timeout_secs` defaulted to 300 on
   line 116). Per-call override exists but the user's feedback memory
   `feedback_delegate_run_300s_cap.md` notes "too short for multi-PRD
   delegations." Raising the default doesn't help — the blocking
   nature compounds the cost.
3. **Caller blocks too.** The current caller protocol per
   `AGORABUS_RPC.md` is "subscribe to `rpc.reply.<self>` with
   `--max-events 1`, publish request, read reply." That subscriber
   blocks for the full duration of the delegation, with no progress
   visibility.

The chord vision (visions/chord.md §End-state #3) requires that
delegations don't head-of-line-block. This PRD ships the async shape.

## What this builds

### New methods on agorabus-worker.sh

```
delegate.start  → returns {ticket_id, started_unix} immediately.
                  Spawns the claude --print invocation in a
                  background subshell (`&`); records ticket state to
                  ~/.cache/agorabus/tickets/<ticket>.json.

delegate.poll   → reads ticket state file, returns
                  {status: pending|running|done|failed|timeout,
                   started_unix, finished_unix?, exit_code?,
                   bytes_written}. Does NOT include stdout (too big);
                  caller fetches stdout via delegate.result.

delegate.result → reads ticket state + stdout file, returns
                  {status, stdout, exit_code, duration_ms}. Cleanable
                  via `delegate.cleanup`.

delegate.cancel → sends SIGTERM to the recorded child PID, marks
                  ticket as cancelled.

delegate.cleanup → removes ticket files. Daemon also auto-cleans
                  done tickets older than 24h on next list.
```

`delegate.run` stays as a method but its implementation changes to
the obvious composition: call `delegate.start` internally, poll until
done-or-timeout, return the result envelope it returns today. Pure
back-compat for any existing caller.

### Background subprocess shape

A new helper `~/.claude/scripts/agorabus-delegate-runner.sh`:

```bash
#!/usr/bin/env bash
# Run a single delegation; write progress+result to ticket files.
# Invoked detached from agorabus-worker.sh via `setsid … &`.
ticket=$1; cwd=$2; prompt=$3; ttl=$4; agorabus_bin=$5; from_sid=$6
state=~/.cache/agorabus/tickets/$ticket.json
out=~/.cache/agorabus/tickets/$ticket.stdout
…
# Publishes delegate.progress.<ticket> and delegate.result.<ticket>
# on completion.
```

Key shape:
- `setsid` so the runner survives a worker restart.
- Writes ticket state file atomically (write `.tmp`, rename).
- Publishes progress events at fixed intervals (start, every 30s,
  end) on `delegate.progress.<ticket>` and a final
  `delegate.result.<ticket>` event.
- Honors `AGORABUS_DELEGATE_DEPTH` recursion guard same as today.

### Caller-side helper (optional convenience)

A new client-side helper `agorabus delegate-call` (added to agorabus
proper as a thin wrapper, or just documented in
`AGORABUS_RPC.md`) that:

1. Subscribes to `delegate.result.<self>-<rand>` in background.
2. Sends `delegate.start` with that ticket pattern.
3. Reads progress events as they arrive, prints to stderr.
4. Blocks on result, prints stdout to stdout, exits with the
   delegated process's exit code.

That helper restores the ergonomics of `delegate.run` without the
head-of-line cost. Today's callers can either keep using
`delegate.run` (still works, slower) or switch to `delegate-call`.

## Acceptance criteria

1. **AC1 — start returns immediately.** A `delegate.start` request
   with a 60s sleep prompt returns within 500ms with
   `{"ok":true,"result":{"ticket_id":"<ulid>","started_unix":<now>}}`.
   The worker's main loop is free to process other RPCs (e.g. a
   subsequent `ping` in the same second returns within 100ms).

2. **AC2 — poll progression.** Polling the AC1 ticket immediately
   returns `status: "running"` (or `"pending"` for the first 100ms
   if the runner hasn't exec'd yet). After the sleep completes,
   poll returns `status: "done"`.

3. **AC3 — result has stdout.** After AC2 reports done,
   `delegate.result` for the same ticket returns
   `{"ok":true,"result":{"status":"done","stdout":"…","exit_code":0,"duration_ms":N}}`
   where stdout matches what `claude --print --output-format text`
   produced.

4. **AC4 — progress events fire.** Subscribing to
   `delegate.progress.<ticket>` before AC1 yields at least one
   start event and one done event for a 60s-sleep call (no
   guarantee on intermediate events for short calls, but the
   30s-interval rule is documented).

5. **AC5 — cancel.** Issuing `delegate.cancel <ticket>` against a
   running delegation: returns within 500ms; the runner's claude
   subprocess receives SIGTERM; ticket state transitions to
   `cancelled`; a `delegate.result.<ticket>` event is published
   with `status:"cancelled"`.

6. **AC6 — back-compat for delegate.run.** A `delegate.run`
   request with a 5s prompt returns the same envelope shape it does
   today (`{stdout, exit_code, duration_ms, cwd}`). Caller code that
   used delegate.run unchanged continues to work. (Worker
   implementation now delegates to start+poll internally; observable
   behavior unchanged.)

7. **AC7 — no head-of-line.** With one `delegate.run` of 60s in
   flight (via the back-compat wrapper, so the *caller* is blocked
   but the worker isn't), a concurrent `ping` from a different
   `from` session_id is replied to within 200ms.

8. **AC8 — ttl honored.** `delegate.start` with `--ttl 5` against a
   60s sleep: ticket transitions to `status:"timeout"` at ~T+5s; a
   `delegate.result` event with `status:"timeout"` is published; the
   runner subprocess is SIGTERM'd then SIGKILLed (10s grace).

9. **AC9 — recursion guard preserved.** A delegated claude
   (invoked via this path) running its own SessionStart still hits
   the `AGORABUS_DELEGATE_DEPTH > 0` exit, same as today's worker.

10. **AC10 — ticket cleanup.** `delegate.cleanup <ticket>` removes
    `~/.cache/agorabus/tickets/<ticket>.{json,stdout}`. Tickets older
    than 24h in `done|failed|timeout|cancelled` state are auto-pruned
    on the first `delegate.poll` or `delegate.cleanup` call of the
    day.

## Risks / trade-offs

- **Per-ticket file proliferation.** Each delegation creates 2 files
  under `~/.cache/agorabus/tickets/`. At 100 delegations/day this is
  ~6KB/day cumulative pre-cleanup. AC10 handles the long tail.
- **Stdout in a file vs in an event.** Putting stdout on the bus
  (event payload) is conceptually cleaner but agorabus has no message
  size cap documented; large outputs would fill the subscription
  buffer. Files are safer and the cost (one extra round-trip to fetch)
  is small. Documented in AGORABUS_RPC.md vNext as the streaming
  convention for chord.
- **AGORABUS_RPC.md update lag.** This PRD intentionally only ships
  the methods; updating the convention doc (and bumping its version to
  v0.2) is a follow-up commit in the same PR. The doc bump must
  describe `delegate.start`/`delegate.poll`/`delegate.result`/
  `delegate.cancel`/`delegate.cleanup` and the
  `delegate.progress.<ticket>` / `delegate.result.<ticket>` topics.
- **Worker restart loses state for in-flight tickets.** The runner
  survives (setsid), but if the worker restarts, the new worker
  doesn't know which tickets exist until it lists
  `~/.cache/agorabus/tickets/`. AC of worker startup: scan that
  directory on boot, mark stale-running tickets (no live runner pid)
  as `failed:"worker_restart"`. Document this in the runner script.
- **Shell-first, Rust-later.** Bash with jq for v1 keeps the change
  local and reviewable. If hot/contended, promote ticket state to
  agorabus daemon (rust-extend, new methods on the daemon side).
  Don't promote preemptively.

## Out of scope

- Migrating callers to `delegate-call`. (Document the new shape;
  let usage migrate organically.)
- Streaming stdout incrementally (live tail of claude's output).
  Possible Fleet 2; needs care around event ordering.
- Replacing `delegate.run` entirely. (Keep as back-compat for
  callers that don't care about head-of-line; warn but don't break.)

## Provenance

- Vision doc: `visions/chord.md` (§End-state #3, §Components #3,
  §Open questions).
- Feedback memory: `feedback_delegate_run_300s_cap.md`.
- Existing worker: `~/.claude/scripts/agorabus-worker.sh` lines
  106-160 (synchronous delegate.run today).
- /dream session 2026-05-25, seed: reflection.
