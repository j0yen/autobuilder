# PRD: recall-doctor-claims — spot-check memory claims against live state

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-25
**Vision:** [visions/freshness.md](visions/freshness.md)
build_auto: false
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
deferred_acs: [5, 10]
**Version target:** `recall v0.7.0` (minor — adds `--check-claims` mode
to `doctor`; non-breaking; old `recall doctor` invocations unchanged).
**Coordinated with:** `recall-daemon` (v0.5.0 in-flight),
`recall-outcome-feedback` (v0.5.1–0.5.3 rebased), `recall-session-stamp`
(v0.6.0). v0.7.0 reserves clean space; if a recall PRD jumps the line
to v0.7.0, rebase this to next free minor.

---

## TL;DR

`recall doctor` audits structural store health (disk-vs-index drift,
supersedes integrity, embedder mix). It says nothing about whether a
memory's *body* still describes the world accurately. This PRD adds
`recall doctor --check-claims`: it extracts conservatively-scoped
filesystem-path and version-number assertions from memory bodies,
verifies them against the live filesystem and the active recall
binary, and parks drift candidates as proposals under
`~/.claude/recall/proposals/` — same review surface `recall observe`
already uses.

No auto-edits. No network. Drift proposals carry the original line,
the matched assertion, the live evidence, and a suggested
`supersedes` action. The user reviews and promotes (or discards).

---

## 1. Why this exists

1. **Observed staleness, twice in one hour, during ordinary
   research.** During `/dream` chord-vision Phase 1 (gossip entry
   2026-05-25T05:25), grounded verification flagged two memories/docs
   whose bodies contradicted live state:

   - `feedback_delegate_run_300s_cap.md` asserts the worker
     "hardcodes timeout 300s". The shipped
     `~/.claude/scripts/agorabus-worker.sh` makes the timeout
     overridable per-call via `params.timeout_secs`. The memory's
     claim is technically falsified; the *underlying* problem
     (head-of-line blocking) is the real issue but the memory's
     wording leads the reader astray.
   - `AGORABUS_RPC.md` v0.1 changelog (line ~end of file in
     `~/.claude/AGORABUS_RPC.md`) says "no handler implementations
     shipped." Stale: ping / self.describe / methods.list /
     delegate.run all ship in the worker today.

   Both were caught because I happened to read both the memory/doc
   AND the live source in the same Phase 1. A deliberate sweep would
   catch more.

2. **recall doctor already exists and is the right home.** Its
   current Audit-the-store framing is structural (disk-vs-index
   drift, supersedes chain integrity, embedder mix). Adding a
   `--check-claims` mode extends the same verb into content-drift
   territory without inventing a new subcommand. Users already know
   to run `recall doctor` after suspicious behavior.

3. **The proposal review flow already exists.**
   `recall observe` parks proposals under
   `~/.claude/recall/proposals/`; `recall proposals` lists them; the
   user promotes or discards. The braid hooks (v0.4.2, shipped
   today) populate this queue from error/correction pairs. Drift
   proposals slot into the same queue with a different `source`
   tag — no new UX.

4. **Memories that decay silently erode trust.** A self-memory that
   used to be right and is now wrong is worse than no memory: the
   reader follows it and is misled. Periodic spot-checks let the
   store admit its own staleness rather than pretending durability
   it doesn't have.

---

## 2. What this builds

### 2.1 New CLI mode

```
recall doctor --check-claims
recall doctor --check-claims --subject self            # filter
recall doctor --check-claims --since 30d               # only recent writes
recall doctor --check-claims --dry-run                 # report, don't park
recall doctor --check-claims --format json             # machine-readable
```

Default behavior:
- Walk all memory files (respecting `--subject` and `--since` filters).
- For each, extract assertions (see §2.2).
- Verify each assertion (see §2.3).
- For disconfirmed assertions, write one proposal file per memory
  under `~/.claude/recall/proposals/` (atomic write via tempfile).
- Print a summary: memories scanned, assertions extracted,
  disconfirmed count, proposals written.

`--dry-run` runs the verification but does not write proposals —
useful for inspecting the extractor's behavior without polluting the
queue.

### 2.2 Assertion extraction (Fleet 1 scope)

Two assertion kinds in Fleet 1; both deliberately conservative to
keep false-positive rate low.

**Kind A: filesystem-path assertion**

Trigger patterns (case-sensitive on the path itself):
- Inside fenced code blocks (\`\`\` / \`\`\`bash / etc.), any token
  matching `(?:/|~/)[A-Za-z0-9_.\-/]+` that looks like a path
  (contains `/`, no spaces).
- In prose, only paths introduced by one of: `see `, `at `, `path: `,
  `(see `, `lives in `, `lives at `, `is at `. This avoids matching
  example paths cited as illustration.

For each candidate path:
- Tilde-expand to absolute.
- `stat()` it.
- If the path doesn't exist, mark as **disconfirmed**.
- If it exists, mark as **confirmed** (Fleet 1 doesn't check
  whether it's a file vs directory; Fleet 2 may add type checks).

**Kind B: version-number assertion**

Trigger patterns:
- Tokens matching `v?\d+\.\d+(?:\.\d+)?` adjacent (within 5 tokens)
  to a binary name from a small whitelist: `recall`, `cargo`,
  `rustc`, `agorabus`, `episodic-observer`, `daily-receipt`,
  `confidant`, `letter-curate`, `zine`, `reliquary`, `cadence`,
  `peon-ping`, `bpolicy`, `ctrace`, `wchg`, `procstat`, `txn-edit`,
  `tcap`, `sbx`, `pevent`.
- Or explicit `<binary> v<version>` form (e.g. "recall v0.4.0",
  "cargo 1.85").

For each candidate:
- Look up the binary in `~/.local/bin/` or `~/.cargo/bin/` or
  `which <binary>`.
- Run `<binary> --version` with a 2s timeout in the user's shell
  ($PATH respected).
- Parse the output for a `\d+\.\d+(?:\.\d+)?` token.
- If it doesn't match the asserted version: **disconfirmed**.
- If the binary is not on $PATH at all: **disconfirmed** (claim
  references something that doesn't exist).

No other assertion kinds in Fleet 1.

### 2.3 Proposal shape

One proposal file per memory with at least one disconfirmed
assertion. Path:
`~/.claude/recall/proposals/<ulid>.md` (existing convention).

```yaml
---
name: drift-proposal-feedback-delegate-run-300s-cap
description: feedback_delegate_run_300s_cap memory contains claims
  that no longer match live state.
metadata:
  source: doctor-claims        # NEW source tag
  type: drift-proposal
  about_memory: feedback_delegate_run_300s_cap
  about_memory_path: /home/jsy/.claude/projects/-home-jsy/memory/feedback_delegate_run_300s_cap.md
  detected_at: 2026-05-25T06:30:00Z
  suggested_action: supersede   # or "update", per heuristic in §2.4
---

# Drift candidates in `feedback_delegate_run_300s_cap`

## Assertion 1 (disconfirmed — version)

> Line 14: "agorabus-worker.sh hardcodes `timeout 300s`"

**Live evidence:**
- File `~/.claude/scripts/agorabus-worker.sh` contains
  `timeout "${TIMEOUT:-300}s"` at line 47 (extracted by grep at
  detection time). The default is 300s; the value is overridable
  via the `TIMEOUT` env var or `params.timeout_secs` per call.
- Memory's "hardcodes" wording is incorrect; the underlying
  head-of-line-blocking concern remains valid but is a separate
  claim.

## Suggested action

`supersede` this memory with a new version that:
- Removes the "hardcodes" wording.
- States the timeout is configurable but defaults to 300s.
- Retains the head-of-line-blocking concern as the actionable part.

(User reviews, edits, and runs `recall promote <ulid>` to apply,
or `recall proposals discard <ulid>` to dismiss.)
```

### 2.4 Suggested-action heuristic

For each disconfirmed assertion:
- If the live evidence simply *refines* the claim (the memory
  says "X is hardcoded", live shows "X is configurable but
  defaults to that value"): suggest `update` — the memory is
  partially right.
- If the live evidence *contradicts* the claim (the memory says
  "file at /foo/bar exists", live shows it doesn't): suggest
  `supersede` — the memory is wrong in a way that needs a fresh
  record, not a tweak.

Heuristic only; the suggestion is advisory. The proposal always
shows both. The user decides.

### 2.5 No auto-edits

This PRD writes proposals only. It never modifies memory files,
never marks memories as superseded, never deletes anything. The
existing `recall promote` / `recall proposals discard` flow is the
only way drift proposals leave the queue.

### 2.6 Exit codes

- `0` — scan completed, no disconfirmed assertions.
- `1` — scan completed, at least one disconfirmed assertion (so
  `recall doctor --check-claims` can be chained into a CI-like
  gate: "fail if anything is stale").
- `2` — scan errored (filesystem unreadable, etc.).

### 2.7 Performance budget

Reading every memory body + extracting + path-stat + binary-version
fork-exec must complete in ≤ 5s for a 50-memory store, ≤ 15s for a
500-memory store, on this laptop (cold disk, no daemon). If we miss
that, add a `--max-checks N` cap or a parallel mode in Fleet 2.

---

## 3. Acceptance criteria

1. **Extractor catches the two observed stale claims** —
   `recall doctor --check-claims --subject self --dry-run` extracts
   at least one disconfirmed assertion from
   `feedback_delegate_run_300s_cap.md` (the "hardcodes 300s" line)
   AND at least one from a synthetic test memory containing a
   non-existent path. Test memory written to a temp `--root` to
   avoid polluting real store.

2. **Proposal file is well-formed and reviewable** — without
   `--dry-run`, the same invocation produces one proposal per
   memory with disconfirmed assertions under
   `--root/proposals/<ulid>.md`, parseable by `recall proposals`
   list. Frontmatter includes the four NEW metadata fields
   (`source: doctor-claims`, `about_memory`, `about_memory_path`,
   `detected_at`).

3. **No auto-edits** — running `--check-claims` against a store
   leaves all memory files byte-identical (verified by sha256
   before/after). Only `proposals/` gains files.

4. **Filter flags work** — `--subject self` skips non-self memories;
   `--since 30d` skips memories older than 30 days. `--format json`
   produces a JSON array of `{memory_id, file_path,
   disconfirmed_assertions: [...]}` records suitable for piping.

5. **Conservative extraction** — running `--check-claims --dry-run`
   on the full real store produces a false-positive rate ≤ 30% on
   a hand-audited sample of the first 5 disconfirmed-assertion
   reports. ("Hand-audited" = jsy reviews and confirms the
   assertion is genuinely stale vs the extractor mis-parsing an
   example.) If FP rate exceeds 30%, tighten extraction patterns
   in `src/doctor_claims.rs` before merging.

6. **Exit codes** — `0` on clean, `1` when ≥1 disconfirmed
   assertion (verifiable via `echo $?`), `2` on actual error
   (e.g. unreadable `--root`).

7. **Existing `recall doctor` invocation unchanged** —
   `recall doctor` (no flags) and `recall doctor --fix` produce
   byte-identical output to v0.4.2 on the same store. Adding
   `--check-claims` does not alter the default doctor flow.

8. **Performance gate** — `recall doctor --check-claims --dry-run`
   on the live ~33-memory store at `~/.claude/recall/` completes
   in ≤ 5s wall-clock on this laptop.

9. **Version + install + changelog + push** (rust-extend
   mechanical):
   - `~/wintermute/recall/Cargo.toml` bumped to `0.7.0` (or
     next-free minor if v0.7.0 taken when this lands).
   - `~/wintermute/recall/CHANGELOG.md` gets a `## v0.7.0` section
     describing `doctor --check-claims`.
   - `cargo install --path . --force` from `~/wintermute/recall`
     refreshes `~/.local/bin/recall`; `recall --version` reports
     the new version.
   - One commit authored as `Joe Yen <jyen.tech@gmail.com>`
     (per [self_inline_edits_beats_patches] / wintermute identity
     rule); subject `recall v0.7.0: doctor --check-claims`.
   - `git push` to existing `origin/main` (no new repo, no
     force-push).

10. **Verified end-to-end on a real proposal** — after install,
    running `recall doctor --check-claims --subject self` against
    the real store on this laptop produces at least one proposal
    AND the user (jsy) reviews it and either promotes or
    discards. Mark PRD shipped only after that round-trip
    completes.

---

## 4. Out of scope (deferred to Fleet 2 or later)

- Network checks (URL HEAD requests).
- "Hardcoded" / "always" / "never" / "removed" / "shipped" prose
  qualifiers (Fleet 2 — needs grep heuristics).
- README / CHANGELOG / CLAUDE_SELF.md checking (Fleet 2 —
  freshness-doc-sweep PRD).
- Hooked into `recall query` for lazy re-verification (Fleet 2 —
  freshness-on-recall).
- Cross-session disagreement detection (Fleet 2 — composes with
  chord-cross-episode).
- Auto-supersede / auto-edit (intentional; the human-in-the-loop
  is the value).
- Asserting on `pacman -Qi` output, kernel version, systemd unit
  state (Fleet 2 if motivated by real drift episodes).

---

## 5. Notes for /build

- This is `build_target: rust-extend` on the existing
  `~/wintermute/recall` repo. No `gh repo create` needed.
- New module `src/doctor_claims.rs`; wire into existing
  `src/main.rs` clap subcommand for `doctor` as a new flag,
  not a new top-level command.
- Test fixtures live in `tests/doctor_claims/` with a
  temp-`--root` store seeded by `tests/common.rs`.
- Coordinate version with in-flight recall work: at draft time,
  v0.5.0 (recall-daemon), v0.5.1–0.5.3 (recall-outcome-feedback),
  and v0.6.0 (recall-session-stamp) are reserved. v0.7.0 is the
  next clean minor.
- The proposal `source: doctor-claims` tag is new. `recall
  proposals list` should display it; if the existing lister
  doesn't show source tags, add a small `--source <tag>` filter
  as part of this PRD (one extra acceptance criterion in iter-2
  if jsy concurs).

---

## 6. Risks

- **False positives erode trust in the queue.** Mitigated by
  AC5's 30%-FP gate and by `--dry-run` for inspection. If FP rate
  stays high in practice, reduce extraction scope (e.g., drop
  prose-path extraction; only check fenced-code paths) before
  shipping more aggressive Fleet 2 patterns.
- **Self-referential drift**: this PRD itself will go stale (cites
  v0.4.2, references chord vision, etc.). When `recall
  doctor --check-claims` first runs against this file's eventual
  promoted form, it will be a good early test.
- **Binary-version checks invoke `--version` on whitelisted
  binaries** — if any of those binaries hangs or has a side
  effect on first run, the scan blocks. Mitigated by 2s timeout
  per fork and a `--no-binary-checks` escape hatch.
