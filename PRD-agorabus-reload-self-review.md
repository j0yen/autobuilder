# PRD: agorabus-reload-self-review — close the 4-run escalation loop

Status: Draft v0.1
build_target: shell
build_into: /home/jsy/.claude/skills/self-review
Vision: visions/vigil.md

## TL;DR

Self-review's `agorabus_daemon_stale_binary` playbook refuses to
auto-fix whenever subscribers exceed 5, because today a bounce strands
those subscribers (they can't reconnect). That ceiling is why the
stale-binary item has been carried forward, escalated-not-fixed, for
4+ consecutive runs. Once `agorabus reload` makes the bounce
non-destructive (Fleet 3 PRDs 1–4), this PRD rewrites the playbook to
call `agorabus reload` and raise the ceiling — so the recurring anomaly
gets fixed automatically within guardrails instead of re-escalated every
tick.

## Why this exists

- **The loop is documented and persistent.** `self-review/SKILL.md:247`
  defines the `agorabus_daemon_stale_binary` playbook. Its auto-fix
  conditions (`SKILL.md:256-259`) require subscribers ≤ 5; its escalation
  (`SKILL.md:270`) says: "if cargo build fails OR subscriber count >5,
  write to Pending … Other live Claude sessions will need to re-run their
  SessionStart hook to reattach after restart, and that's a user-visible
  disruption." The journals for 2026-05-27/28/29 show this escalation
  firing run after run (10 subscribers > 5; voice fleet + live /build +
  /dream), never resolving.
- **The premise is now false.** With PRD-agorabus-client-reconnect +
  PRD-agorabus-reload, a bounce no longer requires anyone to re-run a
  hook — subscribers reconnect themselves. The "user-visible disruption"
  that justified the ≤5 ceiling no longer holds, so the playbook can act
  at a higher subscriber count.
- **Dream rule 6 / honest dependency.** This PRD is only real once
  `agorabus reload` ships and is verified — it edits the playbook to
  *use* that command. It must not be built before PRD-agorabus-reload
  lands. (Stated here so /build serializes it last in Fleet 3.)

## What this builds

Edits `~/.claude/skills/self-review/SKILL.md` — the
`agorabus_daemon_stale_binary` playbook only (no other playbook
touched). `build_target: shell` because the deliverable is the SKILL.md
text + the apply-log contract it drives, not a binary.

- **Fix step rewrite (`SKILL.md:261-266`).** Replace the manual
  `cargo build → kill → nohup relaunch → verify socket → re-run hook`
  sequence with: `agorabus reload --build --format json` (with the repo
  dir configured), parsing the verdict. On `status:reloaded` log
  `step:fix_verified` with the verdict; on `reloaded-degraded` or
  `failed` log `step:fix_failed` with the verdict (incl. the missing
  session_ids) and fall through to escalation. Preserves the existing
  `apply-log.jsonl` `investigate.agorabus_daemon_stale_binary` event
  contract and the 5-minute loop-breaker (`SKILL.md:258`).
- **Ceiling change (`SKILL.md:259`).** Replace the hard "subscribers ≤ 5"
  auto-fix gate with a gate keyed on reload availability: if
  `agorabus reload` exists (`command -v agorabus && agorabus reload
  --help` succeeds) the subscriber ceiling rises to a higher bound
  (proposed 25 — generous, still a sanity backstop against a runaway
  fan-out), since reconnect handles the disruption. If `agorabus reload`
  is absent (older binary), fall back to the old ≤5 manual path
  unchanged — the playbook degrades gracefully on a bus that predates
  Fleet 3.
- **Escalation text update (`SKILL.md:270`).** Drop the "other sessions
  must re-run their SessionStart hook" warning for the reload path (no
  longer true); keep escalation for `failed`/`reloaded-degraded` verdicts
  and for cargo-build failures, now quoting the reload verdict.
- A short note in the playbook's investigation section recording that
  the bounce is non-destructive via reconnect, with a pointer to
  `visions/vigil.md` Fleet 3 for provenance.

No change to other self-review phases or playbooks.

## Acceptance criteria

1. **AC1 — playbook calls reload.** The `agorabus_daemon_stale_binary`
   fix step invokes `agorabus reload` (not a hand-rolled
   kill+relaunch) when `agorabus reload` is available. Verifiable by
   grepping the playbook text for `agorabus reload --build` and the
   absence of the old `kill <daemon-pid>` line in that fix path.
2. **AC2 — ceiling raised, backstop kept.** The auto-fix subscriber gate
   is > 5 (proposed 25) when reload is available, and the playbook still
   documents a finite backstop (not "unbounded"). Asserted by reading the
   gate condition.
3. **AC3 — graceful fallback.** When `agorabus reload` is unavailable the
   playbook retains the ≤5 manual rebuild/kill/relaunch path verbatim, so
   a pre-Fleet-3 agorabus is still handled. Asserted by the presence of
   both branches.
4. **AC4 — verdict drives the log.** The playbook maps
   `status:reloaded → step:fix_verified` and
   `reloaded-degraded|failed → step:fix_failed` + escalation, preserving
   the `apply-log.jsonl` event names and the 5-minute loop-breaker.
   Asserted by reading the mapping.
5. **AC5 — escalation text no longer claims hook re-run.** For the reload
   path, the escalation/Pending text does not tell the user to re-run the
   SessionStart hook (because reconnect handles it); the warning survives
   only on the legacy fallback branch.
6. **AC6 — scope contained.** `git diff` of SKILL.md touches only the
   `agorabus_daemon_stale_binary` playbook section (and, if needed, the
   one-line cross-reference at `SKILL.md:88`); no other playbook or phase
   text changes.
