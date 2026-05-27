# PRD-chord-claim

Status: Draft v0.1
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/agorabus
build_version_bump: minor
Vision: visions/chord.md

## TL;DR

Add an **advisory soft-lock** primitive to agorabus so a Claude
session can announce "I'm about to touch this path for the next N
seconds" and peer sessions can see the claim before they start their
own write. No kernel locks. No enforcement. Pure cooperation: each
session decides whether to honor, override, or coordinate.

## Why this exists

Three concurrent sessions is the steady state on this laptop. Today
the steady state is also: nobody knows what anyone else is editing.
Evidence:

- 2026-05-24 journal §Notable: "headless /build (PID 99933) running
  alongside this /self-review (PID 103334) plus interactive PID 930
  … all three registered as `claude-<rootpid>-jsy` on agorabus."
  Three sessions, one bus, zero collision detection.
- `agorabus` README §Why explicitly says: "concurrent Claude sessions
  on the same laptop are mutually blind, leading to clobbered shared
  files (settings.json, recall DB) and redundant work." The bus
  solves *presence*; this PRD solves *intent-on-shared-files*.
- Recall memory `feedback_classifier_per_command.md` notes that
  durable allow-rules are how the user prefers to handle repeated
  per-command friction — analog here is: an explicit claim is
  preferable to a guess-and-pray write.

The cost of *not* having claims is real: the recent `recall-daemon`
iter-2 work could theoretically race with `/self-review`'s recall
reindex; today neither knows about the other.

## What this builds

### CLI surface

```sh
# Acquire a claim. Publishes claim.acquire.<sid> on the bus.
# Refuses if an active claim on the same path exists from a different
# session_id (unless --force).
agorabus claim acquire <path> --ttl 600 [--reason "editing recall v0.5"]

# Release a claim. Idempotent (release of non-existent claim is OK).
agorabus claim release <path>

# List active claims. Filters: --path, --session-id, --include-expired.
agorabus claim list [--format text|json]
```

Paths are stored as canonicalized absolute paths (caller's CWD
applied). Two claims on `~/foo` and `/home/jsy/foo` are treated as
the same path.

### Wire format

Claims are agorabus events on a reserved topic:

- `claim.acquire` — published when a session acquires a claim.
  Payload: `{path, session_id, ttl_unix, reason?}`.
- `claim.release` — published on release. Payload:
  `{path, session_id}`.

The daemon also keeps an in-memory `claims` table and exposes a
`{"op":"claim_list"}` op so the `agorabus claim list` client can read
it without subscribing to history (claims aren't replayed on late
subscribe; the table is the source of truth).

### State persistence

In-memory only. If the daemon restarts, all claims are dropped (same
behavior as agorabus presence today; daemon restart is rare). TTL
expiry is checked on every `claim_list` read and on every
`claim_acquire` against the same path. No background sweeper needed.

### Conflict resolution

When `acquire` finds an active claim on the same path from a different
session, behavior depends on flags:

- Default: reject with
  `{"ok":false,"error":"claim_conflict","detail":{"holder":sid,"expires_unix":n,"reason":"..."}}`.
  Caller (typically a skill or hook) decides what to do.
- `--force`: overwrite. Publishes `claim.release` for the old holder
  and `claim.acquire` for the new one.
- `--wait <seconds>`: block in the client until either the existing
  claim expires/releases or the timeout fires. Implemented client-side
  via subscription to `claim.release` — no daemon-side queueing.

A claim from the same session on the same path is treated as
**renewal** (TTL bumped, no error).

## Acceptance criteria

1. **AC1 — acquire success.** `agorabus claim acquire ~/foo --ttl
   60 --session-id sid-A` on a free path returns
   `{"ok":true,"result":{"path":"/home/.../foo","ttl_unix":<now+60>}}`.
   Path is canonicalized to absolute.

2. **AC2 — list returns claim.** After AC1, `agorabus claim list`
   shows one entry with the canonicalized path, sid-A, ttl_unix.
   `--format json` produces parseable JSON.

3. **AC3 — conflict on different session.** With AC1's claim active,
   `agorabus claim acquire ~/foo --session-id sid-B` returns
   `{"ok":false,"error":"claim_conflict"}` with holder=sid-A.

4. **AC4 — renewal on same session.** With AC1's claim active,
   `agorabus claim acquire ~/foo --ttl 120 --session-id sid-A`
   succeeds and the new ttl_unix is ≥ original ttl_unix.

5. **AC5 — release is idempotent.** `agorabus claim release ~/foo
   --session-id sid-A` succeeds. A second release on the same path
   also succeeds (`{"ok":true,"result":{"released":false}}` — no error
   for unknown claim, just `released:false`).

6. **AC6 — force overrides.** With AC1's claim active and `sid-B`
   sending `claim acquire ~/foo --force`, the claim transfers; AC2's
   list now shows sid-B; a `claim.release` event for sid-A was
   published on the bus prior.

7. **AC7 — TTL expiry.** Acquire with `--ttl 1`. Sleep 2 seconds.
   `claim list` returns `[]` (expired claims are silently pruned on
   read, no event published).

8. **AC8 — fail-open on no daemon.** All claim subcommands with no
   daemon running exit 0; `acquire`/`release` emit nothing on stdout,
   `list` emits `[]`. (Matches existing agorabus client convention.)

9. **AC9 — wait flag client-side.** `agorabus claim acquire ~/foo
   --session-id sid-B --wait 5` against AC1's claim either succeeds
   when sid-A releases within 5s or returns
   `{"ok":false,"error":"claim_conflict","detail":{"holder":"sid-A","timed_out":true}}`.

10. **AC10 — version + changelog.** Cargo.toml minor bump. CHANGELOG
    entry. REPOS.md untouched.

## Risks / trade-offs

- **Advisory only.** A session can write to a claimed path anyway.
  This is intentional — kernel-enforced locks are over-engineered for
  single-user trust; recall-observer-correlation already demonstrates
  that advisory hooks are sufficient when sessions opt in.
- **No persistence across daemon restart.** A 1-hour TTL claim is lost
  if agorabus restarts. Acceptable — restarts are rare; if a session
  cares, it re-acquires on the next heartbeat. (Workaround for future:
  write claims to `~/.cache/agorabus/claims.json` on each change.
  Defer until needed.)
- **No granular scope (path glob, repo, etc.).** Only literal paths
  (canonicalized). Adding glob support requires conflict detection on
  globs, which gets messy fast. Start with exact paths; expand only
  if patterns of use show up.
- **`--wait` is client-side polling.** Implemented via subscription to
  `claim.release`; no server-side queueing. Means under heavy
  contention you could see a "shouting match" — N sessions all
  waiting on the same release race to acquire. Acceptable for the
  single-user, low-contention case.

## Out of scope

- Kernel-enforced locks. (See provfs LSM for a different model.)
- Glob/regex paths. (Fleet 2 if needed.)
- Persistence across daemon restart. (Add when first lost claim
  causes pain.)
- Hook integration (auto-claim on a tool call's target paths). The
  primitive lands here; wiring is a separate change.

## Provenance

- Vision doc: `visions/chord.md`
- Depends on (soft): `PRD-chord-intent-rich.md` —
  `working_paths` in intent and the claim path list reinforce each
  other; not strictly required to ship in order.
- /dream session 2026-05-25, seed: reflection.
