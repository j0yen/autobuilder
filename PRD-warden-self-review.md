# PRD: warden-self-review — report the guardrail once, not every run

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/warden.md (Fleet 1)
**build_target:** shell
**build_into:** /home/jsy/.claude/skills/self-review (SKILL.md + Phase A/B.5 blocks)
**build_version_bump:** n/a
**Depends on:** PRD-warden-home (Fleet 1) — needs the stable `status` JSON
**Codename:** *escalate-once* — a standing blind spot should be stated once and tracked, not re-noticed forever.

## TL;DR

`/self-review` re-discovers the same fact every run: `bpolicy status` →
`{"loaded": false}`, written into the Pending section as *"bpolicy not
loaded — no enforcement; loading needs sudo + a user-owned policy file."*
It is true, it is unchanging, and re-flagging it every run is noise that
buries the things that actually changed. This PRD does two small things:
adds a `warden:` health line to Phase A (Snapshot) so the enforcer's
state is *visible* at a glance, and adds a Phase B.5 playbook that turns
the recurring inert-state observation into a **single durable
escalation** (one docket finding / one journal note) that subsequent
runs recognize and skip, instead of re-emitting the same Pending line.

## Why this exists

- The Pending line appears verbatim across consecutive self-review runs
  (2026-05-29 runs 1 and 2 both carry "bpolicy not loaded
  (`{"loaded":false}`) … loading needs sudo + a user-owned policy
  file"). This is the same anti-pattern the `docket` vision was built to
  kill — *"the self-review notices the same thing every run; it writes
  it down, then forgets it noticed"* — applied to the enforcement tool.
- There is currently **no** `warden:`/`bpolicy:` line in the Phase A
  Snapshot at all; the enforcer's state only surfaces when a run happens
  to mention it in Pending. A standing health line makes "is the
  guardrail armed, and in what mode" a first-class, glanceable fact.
- This PRD has the smallest blast radius of the warden fleet (shell +
  SKILL.md edits, no kernel, no BPF, no privileged op) and the most
  immediate payoff (less recurring noise), so it can ship the moment
  warden-home pins the `status` JSON it parses.

## What this builds

**Phase A — Snapshot line.** Add a `warden:` line to the Snapshot
section, sourced from `bpolicy status` (the warden-home JSON):
```
- warden: not loaded            # {"loaded": false}
# or, once armable:
- warden: enforce · profile=workspace · 2 pids · 0 denied · ttl 22m
- warden: audit · profile=tight · 1 pid · 14 would-deny · ttl —
```
Parsing keys off the warden-home/`policy`/`deadman` fields (`loaded`,
`mode`, `profile`, `protected_pids`, `stats.denied`, `ttl_remaining_s`);
fields absent in an older `bpolicy` degrade gracefully (just `loaded`).

**Phase B.5 — escalate-once playbook** `warden_enforcer_inert`:
- **Detect:** `bpolicy status` → `{"loaded": false}` AND the user has
  not opted into leaving it unloaded (a marker file
  `~/.config/bpolicy/intentionally-unloaded`).
- **First sighting:** write **one** durable record — a `docket` finding
  if docket has shipped (preferred: it dedupes by design), else a single
  dated journal note under a `## warden` heading — stating: enforcer
  built + present, never armed this boot, what arming would require
  (warden-policy profile + warden-deadman safe-load), and the explicit
  user decision needed ("arm on headless/sandboxed sessions? leave off?").
  Record a fingerprint (`~/.config/bpolicy/.selfreview-escalated`).
- **Subsequent runs:** if the fingerprint exists and state is unchanged,
  the Phase A line still shows `not loaded` but B.5 **does not re-emit**
  the Pending paragraph — it adds at most a one-token carry ("warden:
  inert (escalated, see docket)"). The noise stops.
- **State change resets it:** if `status` ever shows `loaded: true`, or
  the user drops `intentionally-unloaded`, the fingerprint is cleared so
  a future regression re-escalates.

**Idempotence + safety:**
- The playbook **never loads, unloads, or enforces** anything. It is
  observe-and-record only — arming is a user decision, consistent with
  the rest of warden. (Mirrors the self-review guardrail that escalates
  rather than auto-fixes high-blast-radius items.)
- All edits are anchor-based inserts into SKILL.md (Phase A Snapshot
  list; Phase B.5 playbook table) — additive, idempotent, no rewrite of
  existing blocks.

## Acceptance criteria

1. Phase A emits a `warden:` line for all three states: not-loaded,
   audit, enforce. A fixture feeding each `bpolicy status` JSON shape
   produces the documented one-line render; absent optional fields
   (older bpolicy) degrade to `warden: not loaded` / `warden: loaded`
   without error.
2. The `warden:` line parses **only** documented warden-home/policy/
   deadman fields; a malformed or empty `bpolicy status` yields
   `warden: status unavailable` (never a crash or a stack trace in the
   journal).
3. First B.5 run with enforcer inert writes exactly one durable record
   (docket finding if available, else one journal note) and creates the
   escalation fingerprint. Verified by running the playbook twice
   against an unchanged inert state and asserting the record count is 1,
   not 2.
4. Second run with unchanged inert state does **not** re-emit the full
   Pending paragraph; it emits at most the one-token carry. Asserted by
   diffing two consecutive simulated run outputs.
5. Dropping `~/.config/bpolicy/intentionally-unloaded` suppresses the
   escalation entirely (Phase A line still shows state; B.5 stays
   silent). Tested.
6. A transition to `loaded: true` clears the fingerprint so a later
   return to inert re-escalates once. Tested with a state sequence
   inert→armed→inert.
7. The playbook performs no privileged or state-changing operation:
   a trace/dry-run asserts no `bpolicy load|unload|enforce|release` and
   no `sudo` is invoked. (Use `ctrace`/a command shim in the test.)
8. SKILL.md edits are anchor-based and idempotent: applying the install
   twice leaves the file identical (verified by hashing before/after a
   second apply).

## Notes

- Depends only on warden-home's `status` JSON being stable; does **not**
  depend on warden-policy or warden-deadman. The Phase A line shows the
  richer fields when they exist and degrades cleanly when they don't, so
  this PRD can ship before policy/deadman and gain detail as they land.
- This is the `docket` pattern applied to one specific recurring item.
  If `docket-core` has shipped, prefer a docket finding (it already
  solves dedup + escalation lifecycle); the journal-note path is the
  graceful fallback so this PRD does not hard-depend on the docket fleet.
- Edits SKILL.md for `/self-review`. If a concurrent self-review tick is
  mid-run, serialize (the playbook block is the same file other
  self-review PRDs may touch — coordinate as the vigil gossip notes did
  for the agorabus playbook block).
