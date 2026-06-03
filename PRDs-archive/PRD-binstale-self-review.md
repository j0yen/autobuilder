# PRD: binstale-self-review — wire fleet staleness into the daily review

Status: done
build_target: shell
build_into: /home/jsy/.claude/skills/self-review/SKILL.md
Vision: visions/vigil.md

## TL;DR

The "agorabus daemon stale binary" finding has been hand-written into
three consecutive self-reviews (runs 16, 17, 18 on 2026-05-28) — caught
manually each time, parked in §Pending each time, never structurally
detected. Once `binstale` exists, self-review should run it as a
deterministic Phase B.5 probe: surface stale daemons with the verdict
and evidence, and pre-fill the exact `rollout` command in §Pending —
retiring the hand-written note.

## Why this exists

The self-review journal is the evidence, in triplicate
(`~/brain/journal/2026-05-28.md`):

- Run 18 §Carried forward: "agorabus daemon stale binary — RE-OPENED …
  Escalated, not auto-restarted."
- Run 17 §Carried forward: "agorabus daemon stale binary — still
  RESOLVED. Binary mtime … still newer than `src/daemon.rs` …"
- Run 16 (in recall): resolved via an out-of-band rebuild+restart.

Three runs in one day spent re-deriving the same fact by hand — exactly
the "cadence is wasted: same finding, same tick, same no-op outcome"
pattern that motivated the sibling `drift` vision. The
mtime-vs-`src/daemon.rs` comparison the reviewer does by hand at run 17
is precisely what `binstale` (+ `binstale-source-cmp`) computes. This PRD
moves that comparison from a human's eyes into a probe.

This is the `drift`-vision pattern applied to vigil: `drift` point-fixes
broken tool invocations in self-review's own SKILL.md; this PRD adds a
*new* deterministic probe to self-review that consumes a vigil tool.

## What this builds

Edits `~/.claude/skills/self-review/SKILL.md` (Phase B.5 — the
deterministic-anomaly-playbook phase — and the §Pending output
convention):

1. A new B.5 playbook entry **fleet-binary-staleness**:
   - Run `binstale scan --format json` (default daemon regex).
   - If exit code 0 (all `fresh`): record a one-line "fleet current" note
     in the journal Snapshot, no Pending item.
   - If exit code 1 (any stale): for each stale daemon, emit a structured
     §Pending entry with: daemon name, pid, verdict, evidence (exe path /
     inode pair / provfs ts / source HEAD commit), and a **pre-filled
     `rollout` command** — `rollout plan --only <daemon>` (plan, never
     auto-apply, preserving the escalate-don't-restart guardrail).
   - Guard: if `binstale` is not installed, the playbook is a no-op with
     a single "binstale not yet installed" Snapshot line (so this edit is
     safe to land before/after the binstale tool itself).
2. A note in the §Pending convention section documenting that
   fleet-staleness items are **escalation-only** — self-review reports
   them and pre-fills the `rollout plan` command but does **not** run
   `rollout apply` autonomously (consistent with the run-16/18 deliberate
   escalate-don't-restart call).

No change to any other phase. No new tools built; this is skill text +
one shell invocation in the playbook.

## Acceptance criteria

1. `~/.claude/skills/self-review/SKILL.md` gains a B.5 playbook entry
   named `fleet-binary-staleness` that invokes `binstale scan
   --format json` and branches on its exit code (0 → snapshot note,
   1 → per-daemon Pending entries).
2. The playbook entry specifies the §Pending output shape: daemon, pid,
   verdict, evidence, and a pre-filled `rollout plan --only <daemon>`
   command string.
3. The playbook is explicitly **escalation-only**: the SKILL.md text
   states self-review never runs `rollout apply` autonomously and pins
   the rationale to the run-16/18 escalate-don't-restart precedent.
4. The playbook degrades safely when `binstale` is absent: a single
   "binstale not yet installed" snapshot line, no error, no Pending item.
   (So the edit can land in either order relative to PRD-binstale.)
5. The added invocation uses only the real installed `binstale` surface
   (`scan`, `--format json`, `--match`) — verified against
   `binstale --help` at build time so this edit does not itself become a
   `drift`-class stale invocation.
6. `bash -n` (or shellcheck on any extracted snippet) passes on any shell
   block added to the SKILL.md; the markdown remains within the skill's
   existing structure (no new top-level phase, extends B.5).
7. A dry read-through of the edited SKILL.md by the build verifier
   confirms the new entry references `binstale`/`rollout` consistently
   with their PRDs (verdict names match PRD-binstale; command name
   matches PRD-rollout).
