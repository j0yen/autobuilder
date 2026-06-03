# PRD: recall-corpus-vacuum — sweep low-utility-high-surface memories on a schedule

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/fidelity.md](visions/fidelity.md)
**Depends on:** [PRD-recall-doctor-utility.md](PRD-recall-doctor-utility.md) shipped (utility data exposed)
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.5` (minor — new `recall vacuum`
subcommand; periodic action surface).

---

## TL;DR

The doctor exposes high-surface-low-use memories; this PRD acts on
them. `recall vacuum` is a sweep that, by default, lists candidate
ids matching `surfaced_count >= 20 AND used_count == 0` (the
pure-noise corpus). With `--apply` it executes one of three
configurable actions: aggressive decay (`confidence -= 0.10`),
supersede-proposal (writes under `~/.claude/recall/proposals/` for
user review, same surface braid uses), or archive (moves the file
to `memories-archive/`). Default action: decay. Plus a self-review
playbook entry that runs `recall vacuum --dry-run` weekly and
surfaces the count. Last PRD of the fidelity vision; closes the
loop from "measure utility" to "act on it."

---

## 1. Why this exists

1. **Without action, doctor's utility surface is just a complaint.**
   PRD #4 lists the bad-fit memories; this PRD does something
   about them.
2. **The bar `surfaced_count >= 20 AND used_count == 0` is strict
   on purpose.** A memory surfaced 20+ times with zero use is not
   "underused" — it's noise. 20 surfacings = ~20 sessions of
   ranking that earned the memory a spot in the context without
   ever delivering value.
3. **Decay (default) is recoverable.** A memory whose confidence
   drops too low can be lifted manually via `recall update <id>
   --confidence 0.7` if the user disagrees. Archive is heavier and
   opt-in via config.
4. **Self-review wants a recurring action, not a one-shot.** A
   weekly `recall vacuum --dry-run` invocation lets self-review
   surface "5 memories are candidates for vacuum" as a Pending item
   the user reviews. Matches existing self-review pattern.

---

## 2. What this builds

### 2.1 New subcommand: `recall vacuum`

```
recall vacuum [--dry-run] [--apply] [--action decay|supersede|archive]
              [--min-surfaced N] [--max-used M] [--format text|json]
```

- Default: `--dry-run` (lists candidates, takes no action).
- `--apply`: actually performs the configured action.
- `--action`: override the default action (`decay`).
- `--min-surfaced`: candidate threshold (default 20).
- `--max-used`: candidate threshold (default 0).

Output:

```json
{
  "candidates": 7,
  "would_apply": "decay",
  "memories": [
    {
      "id": "01KS...",
      "subject": "self",
      "surfaced": 24,
      "used": 0,
      "confidence_before": 0.58,
      "confidence_after": 0.48,
      "action_applied": "decay"
    }
  ]
}
```

(In `--dry-run`, `confidence_after` is the projected post-apply
value and `action_applied` is `null`.)

### 2.2 Action: decay

`vacuum --apply --action decay` calls the existing
`apply_feedback_delta` path with a custom delta = `-config.decay`
(default 0.10, configurable in `recall.toml`). Same clamp rules as
reject; floor 0.05.

### 2.3 Action: supersede-proposal

`vacuum --apply --action supersede` writes a proposal file under
`~/.claude/recall/proposals/<ULID>.md` with the memory id, the
surfaced/used counts, the suggested action ("delete this memory"),
and a `pending:` field. Same shape as braid's proposals; the user
reviews via the existing proposal-review flow. This action does NOT
modify the memory directly — it stays in the corpus until the user
approves the proposal.

### 2.4 Action: archive

`vacuum --apply --action archive` moves the memory's markdown file
from `~/.claude/recall/memories/<kind>/<id>.md` to
`~/.claude/recall/memories-archive/<kind>/<id>.md` AND removes its
row from `memories_meta` so retrieval skips it. The file remains
recoverable by hand if the user wants it back.

### 2.5 Self-review playbook

`~/.claude/skills/self-review/playbooks/recall_corpus_vacuum.md`:

```markdown
# playbook: recall-corpus-vacuum

Trigger: weekly heartbeat (every 7th invocation of self-review,
or first invocation after Sunday 06:00 local).

Action: run `recall vacuum --dry-run --format json | jq '.candidates'`.
If count > 0, surface in "Pending your call":
> recall vacuum: <N> memories surfaced >=20 times with 0 use.
> Run `recall vacuum --apply` to decay them, or
> `recall vacuum --apply --action archive` to move them out.

Not auto-applied. The user owns the apply step.
```

### 2.6 Configuration

`recall.toml` `[vacuum]` section:

```toml
[vacuum]
default_action  = "decay"     # decay | supersede | archive
min_surfaced    = 20
max_used        = 0
decay_amount    = 0.10        # per --action decay sweep
```

### 2.7 Out of scope

- **No automatic apply.** Even with `--apply`, the user must run the
  command; vacuum doesn't fire on its own. Self-review surfaces; the
  user applies.
- **No batch retroactive cleanup of past surfacings.** Pre-PRD-#1
  surfacings have no `surfaced_count`, so the bar of 20 requires
  fresh surface data going forward.
- **No vacuum scheduling daemon.** A systemd-user timer is a v2 idea
  if surface-cleanup demand emerges.

---

## 3. Acceptance criteria

1. **AC1 — `recall vacuum --dry-run` lists candidates without
   mutation.** Synthetic store with one memory at (surf=25, used=0,
   conf=0.55) and one at (surf=25, used=3, conf=0.55). Dry-run
   returns 1 candidate; both memories' confidence + surfaced_count
   + used_count post-call unchanged. Test:
   `tests/vacuum_dry_run.rs`.
2. **AC2 — `--apply --action decay` decreases confidence by
   `decay_amount`, clamped at floor.** Same fixture; after apply,
   the one candidate's confidence is 0.55 - 0.10 = 0.45. AC pass
   if equal. Test:
   `tests/vacuum_apply_decay.rs`.
3. **AC3 — `--apply --action archive` moves the file and removes the
   row.** Synthetic store; after apply, the file is at
   `memories-archive/<kind>/<id>.md`, the original path is gone,
   and `recall list` no longer returns the id.
4. **AC4 — `--apply --action supersede` writes a proposal file.**
   After apply, `~/.claude/recall/proposals/<ULID>.md` exists and
   contains the memory id, surfaced/used counts, and `pending: true`.
   Original memory unchanged.
5. **AC5 — `--min-surfaced` and `--max-used` flags override
   defaults.** Synthetic fixture: 30 memories with random
   surfaced/used. Run with `--min-surfaced 50 --max-used 5`; assert
   only matching candidates returned.
6. **AC6 — Re-running `--apply --action decay` is idempotent within
   one decay cycle.** Apply once; re-apply on the same candidate
   set; second pass produces 0 candidates (because their confidence
   has already dropped AND they're now below threshold OR they've
   slipped out of the candidate set — depending on the fixture).
   This AC verifies that the threshold gate works.
7. **AC7 — `recall doctor` reflects vacuum decay.** Decay a
   candidate, then run `recall doctor --format json`; the candidate's
   `confidence` in the utility section reflects the post-decay value.
   (Regression test that vacuum and doctor share state.)
8. **AC8 — Self-review playbook surfaces non-zero candidate count.**
   Sim test: pre-seed surfaced/used counters; invoke playbook script;
   assert that the playbook prints a count > 0 line to its output.

---

## 4. Implementation notes

### 4.1 Candidate query

```sql
SELECT id, kind, subject, confidence, surfaced_count, used_count
FROM memories_meta
WHERE surfaced_count >= ?1 AND used_count <= ?2
ORDER BY surfaced_count DESC;
```

Limit not enforced — full list per call. If corpus grows huge, add
a `--top N` flag in v2.

### 4.2 Archive operation

```rust
let src = format!("{}/memories/{}/{}.md", root, kind, id);
let dst = format!("{}/memories-archive/{}/{}.md", root, kind, id);
fs::create_dir_all(Path::new(&dst).parent().unwrap())?;
fs::rename(&src, &dst)?;
conn.execute("DELETE FROM memories_meta WHERE id = ?1", [&id])?;
```

Atomicity caveat: `fs::rename` succeeds before SQLite delete; if
process dies between, we have a stranded archive file with a live
SQLite row. Recovery: next `recall doctor` reports file-vs-index
divergence and the user can hand-resolve. Tolerable for v1.

### 4.3 Supersede-proposal format

```markdown
---
id: <new ULID>
proposal_type: vacuum
target_id: <victim ULID>
target_subject: <subject>
surfaced: <n>
used: <n>
created_at: <ISO ts>
pending: true
---

# Vacuum candidate: <victim id>

Surfaced <N> times across recent sessions with <M> evidence of use.
Suggested action: delete or supersede with a more accurate variant.

Original body:
> <first 300 chars of victim body>

Apply with:
  recall update <victim id> --supersedes <new id>
  # OR
  recall feedback --reject <victim id>

Discard this proposal by deleting this file.
```

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Decay too aggressive on near-threshold memories | Decay default 0.10 = 5 sweeps to confidence floor. User can reduce in config. Decay is recoverable via `recall update`. |
| Archive irreversible | Archived files are on disk; `recall import` could restore. Default action is decay (recoverable); archive is opt-in via `--action archive`. |
| User forgets to run vacuum | Self-review playbook surfaces the candidates weekly. The user owns the apply, but the reminder is automatic. |
| Vacuum runs while a session is mid-surfacing | Vacuum operates on persisted SQLite rows; mid-session surface updates to `surfaced.json` (per-session file) don't conflict. SQLite WAL handles concurrent reads. |
| False-positive decay on a genuinely-useful-but-paraphrased memory | Use-evidence false-negative is the upstream root; PRD #2 acknowledges it. Decay is recoverable via `recall update <id> --confidence 0.7`. |

---

## 6. Phasing

- **v0.7.5** (this PRD): `recall vacuum` subcommand with three
  action modes, config section, self-review playbook entry.
- (Vision closed after this PRD.) Future v2 work: semantic
  use-detection (PRD #2 §5), automatic scheduling, query-time
  ranking input from `used_count`.
