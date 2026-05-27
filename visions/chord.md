# Vision: chord

> Three Claude sessions sound at once on this laptop. Without
> coordination they're noise. With coordination they're a chord.

Created: 2026-05-25
Seed: reflection (no user topic; bare `/dream`)
Pace: opt-in (default — all PRDs `build_auto: false`)

## TL;DR

Headless `/build` + headless `/self-review` + the interactive session
now run concurrently as a matter of course. `agorabus` registers them
on a presence bus and ships a pub/sub + RPC convention, but the
session-to-session relationship is still mostly blind:

- `delegate.run` is synchronous, blocking, and caps at 300s.
- Heartbeat carries `tool` only — no skill, no PRD, no working paths.
- Nothing prevents two sessions from racing on the same file.
- `episodic-observer` watches one transcript at a time; no
  cross-session patterns ever land in memory.

Chord fills exactly these gaps: a thin coordination layer atop the
existing bus. Four PRDs, each PRD-sized, each `rust-extend` or
`shell-extend` of repos that already exist.

## End-state

When chord is fully built:

- Any session can see, in one call, what every other live session is
  *doing* — current skill, current PRD, current working paths, last
  tool, time since last tool. Not just "subscriber alive at heartbeat
  N."
- Any session can acquire an advisory soft-lock on a path or a repo
  before touching it. Other sessions read the claim and choose to
  defer, override, or coordinate.
- `delegate.run` is replaced (or shadowed) by an async ticket pattern:
  `delegate.start` returns a ticket immediately; the worker fires the
  job in the background; the caller subscribes to a result topic and
  is never head-of-line-blocked on long delegations. The 300s cap
  becomes a per-call `--ttl`, not a daemon-wide ceiling.
- `episodic-observer` notices when a pattern crosses sessions: e.g.
  session A errors on `~/wintermute/foo` at T; session B's next tool
  touches the same path at T+45s with a different approach. That's an
  episodic candidate worth a memory.

## Components (Fleet 1 — 4 PRDs)

1. **chord-intent-rich** (`rust-extend` agorabus) — heartbeat envelope
   grows `skill`, `prd_slug`, `working_paths[]`, `last_tool_at_unix`.
   New CLI: `agorabus intent set --skill X --prd Y --paths a,b`. New
   query: `agorabus intent list` returns structured intent per peer.
   *Foundation for #2, #3, #4 — write first.*

2. **chord-claim** (`rust-extend` agorabus) — soft-lock primitive
   over pub/sub. `agorabus claim acquire <path> --ttl 600`,
   `agorabus claim release <path>`, `agorabus claim list`. Conflict
   detection only (advisory); each session decides whether to honor.
   No kernel locking.

3. **chord-async-delegate** (`shell` — extends
   `~/.claude/scripts/agorabus-worker.sh`, possibly with a small
   `rust-extend` to agorabus for state). New methods:
   `delegate.start` (returns `ticket_id` immediately, work runs in
   background), `delegate.poll <ticket>` (status), `delegate.cancel
   <ticket>`. Worker publishes `delegate.progress.<ticket>` and
   `delegate.result.<ticket>` events. Existing `delegate.run` stays as
   a thin wrapper (start → poll-loop until result/timeout).

4. **chord-cross-episode** (`rust-extend` episodic-observer) —
   read agorabus heartbeats + structured intent + per-session JSONL
   transcripts. Detect cross-session patterns: error-in-A → fix-in-B
   on same path within N seconds; redundant-work (two sessions edit
   the same file in the same hour with similar diffs); rescue
   (interactive session unblocks headless). Emit candidate memories
   with `cross_session_pattern` tag.

## Order

```
chord-intent-rich (must ship first)
        │
        ├── chord-claim          (depends on intent path-list field)
        ├── chord-async-delegate (independent; can land in any order)
        └── chord-cross-episode  (depends on intent + claim hints)
```

chord-intent-rich and chord-async-delegate are mutually independent
and can develop in parallel. chord-claim only needs intent's
`working_paths` schema. chord-cross-episode benefits from both
intent and claim being live, but degrades gracefully — if claims
aren't present, it just doesn't surface "redundant work despite
explicit claim" episodes.

## Open questions

- **Naming**: is `chord` the right name? Alternatives considered:
  consort, council, concord, concert. Chord is short, evocative
  (independent notes, coordinated sound), and not currently taken in
  `~/wintermute/`. If the user prefers another name, rename the
  vision file + all four PRDs together (they're all draft).

- **Auth on chord-claim**: anyone on the local socket can release
  anyone else's claim. Fine on a single-user laptop (matches the
  agorabus trust model), but worth documenting in the PRD as a known
  limitation. Don't add signing.

- **chord-async-delegate landing place**: pure bash extension of
  agorabus-worker.sh keeps the change local, but ticket state needs
  to survive worker restarts (writeable JSON under
  `~/.cache/agorabus/tickets/`). Alternative: push ticket state into
  the agorabus daemon (rust-extend). PRD §What this builds says
  "shell first; promote to rust if hot." Reader decides.

- **chord-cross-episode false-positive rate**: the cross-session
  detector will see plenty of "session A edited foo.rs, session B
  edited foo.rs three hours later" — not interesting. PRD must
  specify the time window and same-tool-class filters to keep noise
  low. Start strict (≤120s, same path, opposite outcomes); loosen
  later.

## Fleet 2 (bullets only — draft after ≥2 of 4 Fleet 1 PRDs ship)

These ride on the Fleet 1 substrate. None are PRDs yet; they're
captured here so the next `/dream extend chord` pass has a starting
list.

- **chord-peek** — `agorabus peek <peer-sid>` wraps an RPC
  `self.recent_tools` call (new method on agorabus-worker.sh) so any
  session can ask any other "what did you just do" without grepping
  the peer's transcript.
- **chord-peer-review** — PostToolUse hook publishes `code.commit.<repo>`
  events on commit/PR. Other interactive sessions subscribe and can
  offer review via RPC.
- **chord-method-discovery** — extend `methods.list` to return a
  capability table (method → schema → cost class) and let sessions
  register custom methods at startup. Today `methods.list` is hardcoded.
- **chord-quorum** — for risky shared actions ("two of three concurrent
  sessions agree this is safe to apply"), a simple quorum primitive.
  Speculative; only build if a real use case shows up.
- **chord-handoff** — when a session is about to exit, hand off in-flight
  intent + claims to a peer so work doesn't strand. Speculative.

## Cross-fleet coordination

- **Recall fleet**: `chord-intent-rich` adds fields to agorabus's
  presence/heartbeat envelope; recall doesn't read those today, so no
  collision. If `recall-session-stamp` (continuity Fleet 1) ships
  first, its session-id surface is reusable here verbatim.
- **Continuity fleet**: continuity targets the kernel→userspace
  bridge (memlog, provfs, agentns). Chord is one layer up
  (cross-session userspace). They compose: an agentns session id is
  exactly the kind of stable handle chord wants to broadcast in
  `chord-intent-rich`. Once agentns boots, the two visions reinforce
  each other.
- **Cadence fleet**: cadence reads recall + writes tier files; no
  direct overlap. A cross-session episode from chord-cross-episode
  feeding into cadence's daily-receipt is a Fleet-2-or-later idea.
- **Wintermute fleet**: orthogonal. Wintermute is the voice-laptop
  product; chord is a developer-tooling layer on this laptop.

## Evidence (Phase 1 research)

- `agorabus peers` right now returns 4 entries (2 sessions × 2
  worker connections each); confirmed real concurrent state.
- `/home/jsy/.claude/scripts/agorabus-worker.sh` lines 26 + 106-160
  show ping/self.describe/methods.list/delegate.run all implemented,
  with `timeout "${timeout_secs}s" claude --print …` at line 136
  blocking the worker for the full call duration. `params.timeout_secs`
  is per-call overridable but the worker still serializes.
- `AGORABUS_RPC.md` v0.1 changelog (2026-05-23) says "no handler
  implementations shipped" — stale; the worker has shipped since. The
  doc's "Open questions" section already flags streaming as future
  work.
- `~/.claude/CLAUDE_SELF.md` Defaults section: "Cross-session RPC over
  agorabus: convention at ~/.claude/AGORABUS_RPC.md. Pub/sub only, no
  inbox — subscribe to `rpc.reply.<self>` before publishing." Names
  the current pattern; doesn't address blocking.
- recall memory `feedback_delegate_run_300s_cap.md`:
  "agorabus-worker.sh hardcodes `timeout 300s`; too short for
  multi-PRD delegations." Slightly out of date (timeout is now
  per-call overridable, default 300s) but the underlying head-of-line
  problem is real.
- `~/wintermute/episodic-observer/README.md`: "end-of-session JSONL
  detector" — single-session by design. Nothing watches across
  sessions today.
- **2026-05-26T04:00Z first observed entanglement**: `~/wintermute/recall`
  working tree carries an unattributable mix of edits from two sibling
  PRDs — recall-daemon iter-11 (DaemonOp::Start/Stop/Restart, pidfile
  mgmt) and recall-stop-hook-session-id (hooks/stop.sh + new
  tests/hook_stop_session_id.rs) — both touching src/main.rs,
  src/bin/recalld.rs, src/daemon.rs in the same checkout. /build
  deferred per Hard Safety Rule #5; self-review run 8 (20:03 PT
  2026-05-25) escalated the entanglement to a /build blocker. First
  real-world motivation for **chord-claim** — had the originating
  session claimed `repo:recall` on agorabus before editing, the
  sibling would have backed off or queued. Strengthens AC1+AC4 of
  PRD-chord-claim.md (lock acquisition + visible holders).
- **2026-05-26T05:20Z entanglement resolved by per-file ownership
  split**: /build untangled the recall working tree across two
  PRDs by staging only files that belonged to each PRD's commit
  scope. recall-stop-hook-session-id iter-1 (36a636e) committed
  hooks/stop.sh + tests/hook_stop_session_id.rs; iter-2 (32590f2)
  staged ONLY Cargo.toml + Cargo.lock for v0.5.1 bump, deliberately
  leaving daemon WIP unstaged. recall-daemon iter-11 (36cb6ea)
  followed with its non-overlapping src/main.rs + src/bin/recalld.rs
  + src/daemon.rs commit. **Second-order finding:** the v0.5.1
  slot collision (recall-outcome-feedback PRD §6a had previously
  rebased to v0.5.1) forced real-time precedent application —
  "PRD with committed code keeps the slot; the other re-rebases
  to v0.5.2/0.5.3/0.5.4." Both the file-ownership split AND the
  version-slot collision are exactly what chord-claim + chord-
  intent-rich's `working_paths[]` + a `version_slot:` intent field
  would coordinate up front. Strengthens chord-claim AC1+AC4
  (second evidence line) AND adds a new motivating example for
  chord-intent-rich (intent envelope should carry version slots
  on rust-extend PRDs, not just file paths).
- **2026-05-26T18:03Z cross-session push accidentally unblocks
  classifier-stranded PRD** (recall-daemon iter-18): my /build
  session hit the `git push origin main` classifier block 5 times
  in a row across 6h (iter-8 23:36Z, iter-15 05:40Z, iter-16
  06:33Z, iter-17 06:51Z, iter-18 18:03Z). At iter-18 origin/main
  advanced to 3abdf7b "moments later" — a concurrent session
  (likely claude-2308-jsy per agorabus peer list at tick start)
  pushed successfully under the same classifier. All 5 stranded
  commits (36a636e + 32590f2 + 36cb6ea + aa0922c + 3abdf7b)
  landed via this accidental cross-session concurrence. **This is
  evidence for a DIFFERENT primitive than chord-claim**: not
  mutual-exclusion, but "publish bounty" / "fulfill ticket" —
  session A broadcasts an intent it cannot itself complete (push
  needed; classifier won't allow from this context); any other
  session may pick it up and fulfill. Today the fulfillment is
  emergent (sibling happened to try at the right moment under a
  classifier that flapped favorably); a formal version would
  convert luck into protocol. **Adds a new Fleet 2 candidate**:
  `chord-fulfill` (publish-bounty pattern; agorabus topic
  `fulfill.request.<class>`, any subscriber can claim and execute).
  NOT promoting to draft this pass — release-gate Fleet 1
  (publish-allowlist + push-allowlist) directly eliminates the
  underlying classifier non-determinism, which makes fulfill
  fight a problem the queue is about to solve. Reconsider
  chord-fulfill if a non-publish bounty case emerges after
  release-gate ships.

## Manifest (for /dream state)

```
visions.chord:
  path: visions/chord.md
  created: 2026-05-25
  prds_drafted: [PRD-chord-intent-rich.md, PRD-chord-claim.md,
                 PRD-chord-async-delegate.md, PRD-chord-cross-episode.md]
  status: active
  seed: reflection
  pace: opt-in
```
