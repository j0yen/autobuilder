# Vision: freshness

> Memories age. Docs lie. The store grows, but its claims about the
> world get stale silently. Freshness is the discipline of spot-checking
> what we remember against what's still true.

Created: 2026-05-25
Seed: reflection — caught two stale claims during chord vision's
  Phase 1 research, less than an hour before this draft.
Pace: opt-in (default — `build_auto: false`)

## TL;DR

`recall doctor` audits *structural* health (disk-vs-index drift,
supersedes chain integrity, embedder mix). It does not audit
*content* — whether a memory's body still describes the world
accurately. Yet during the chord-vision Phase 1, grounded
verification turned up two falsified memories/docs within minutes:

1. `feedback_delegate_run_300s_cap.md` said the worker hardcodes a
   300s timeout. True at write time; now overridable per-call via
   `params.timeout_secs`. The memory's strong claim ("hardcodes")
   is wrong; the underlying head-of-line-blocking is the real issue.
2. `AGORABUS_RPC.md` v0.1 changelog (2026-05-23) said "no handler
   implementations shipped." Stale: the shipped worker already
   implements ping / self.describe / methods.list / delegate.run.

Both surfaced only because chord's Phase 1 happened to read these
sources in the same hour as the live system they describe. A
deliberate freshness pass would catch more.

This vision adds spot-check tooling that compares memory/doc claims
against live state and proposes drift candidates for the user to
resolve. Like `recall observe` for staleness instead of error
patterns: it doesn't auto-edit, it parks evidence-rich proposals.

## End-state

When freshness is fully built:

- A periodic / on-demand pass extracts checkable assertions from
  memory bodies (file paths, version numbers, "hardcoded" /
  "always" / "never" claims, command outputs) and verifies them.
- Disconfirmed claims become drift proposals under
  `~/.claude/recall/proposals/` with the original text, the live
  evidence, and a suggested supersedes/update action.
- Same proposal-review flow as `recall observe`: user reviews,
  promotes (writes supersede), or discards (memory is fine, or
  needs more nuance).
- Repo READMEs and CLAUDE_SELF.md changelogs gain the same
  treatment as a Fleet 2 follow-on (docs lie too).

## Components

**Fleet 1 — one PRD:**

1. **recall-doctor-claims** (`rust-extend` recall) —
   `recall doctor --check-claims` extracts assertion patterns from
   memory bodies, verifies them against the live filesystem and
   indexable command outputs, and parks drift proposals. Narrow
   scope: filesystem-path assertions and version-number assertions
   in Fleet 1; "hardcoded constants" assertions (which need light
   grep heuristics) added in Fleet 2.

## Order

```
recall-doctor-claims (Fleet 1, single PRD)
```

## Fleet 2 (not drafted)

After Fleet 1 ships and the proposal review loop proves usable:

- **freshness-claims-rich** — extend the assertion extractor to
  cover "hardcoded" / "always" / "never" / "removed" / "shipped"
  qualifiers with grep-based heuristics.
- **freshness-doc-sweep** — apply the same checker to repo READMEs
  and CHANGELOG.md tops (catches the AGORABUS_RPC.md class of
  staleness, which lives outside the recall store).
- **freshness-self-changelog** — apply to CLAUDE_SELF.md's
  changelog section specifically; emit drift proposals when a
  changelog entry's claims about a tool's behavior diverge.
- **freshness-on-recall** — wire the checker into a hook that
  re-checks a memory each time `recall query` returns it as a top
  hit (lazy verification per use, not periodic).
- **freshness-cross-session-witness** — when two sessions
  disagree about the same fact (one writes a memory, the other
  observes contrary live state), surface the conflict via
  agorabus + chord-cross-episode.

Draft Fleet 2 after Fleet 1's single PRD ships AND the proposal
queue has produced at least one user-promoted supersede.

## Open questions

- **False positives**: filesystem-path checks are crisp ("does the
  file exist") but assertion *extraction* is fuzzy. A memory
  saying "logs land at /var/log/foo.log" — is "/var/log/foo.log"
  an asserted path or an example path? Fleet 1 uses conservative
  patterns (only checks paths inside fenced code blocks or
  explicit "see <path>" prose). Drift proposals always include the
  matched line so the user can judge.
- **Velocity**: should the checker also notice when memory
  `recall_count: 0` AND `created_at > 30d ago` — i.e. write-only
  memories? That's a `gc` concern, not a `freshness` concern;
  belongs in the existing `recall gc` flow.
- **Scope creep**: does freshness eventually want to check
  embedded URLs (HEAD request), `cargo` version claims (read live
  Cargo.toml), `pacman` package claims (`pacman -Qi`)? Fleet 1
  stays filesystem-only. Fleet 2 adds version-number lookups in
  Cargo.toml / pkg version files only; no network.

## Why this is a single-PRD vision (for now)

Per dream rule 6 ("don't dream past the research"): I observed two
stale claims, in one hour, while doing one round of Phase 1. That
motivates one tool that automates the spot-check I did by hand. It
does not motivate a 5-7 PRD fleet — the Fleet 2 bullets above are
hypotheses, not observed gaps. Draft them when evidence shows up.

## Evidence log (post-creation)

Memory/gossip drift caught by hand after this vision was drafted —
each one strengthens the case for the eventual Fleet 2:

- 2026-05-25 (~07:50, /dream pass 3) — the freshness-vision gossip
  note itself instructed future passes to "`ctrace ls` then
  `ctrace summary <latest>`." `ctrace ls` is not a valid
  subcommand (actual: `start|stop|status|query|tail`). Strengthens
  freshness-doc-sweep: skill docs / gossip notes drift too, not
  just memories.
- 2026-05-25 (~08:30, /dream pass 8) — the 07:50 vision-handshake
  gossip note cited evidence #3 as "Kernel build PID 12146 still
  running at load 10.42 — same conditions that produced today's
  race. Bug is reproducible and current." Verification at 08:30Z
  found PID 12146 gone; `~/wintermute/wintermute-kernel/pkg/`
  contains `linux-wintermute-7.0.10.arch1-1-x86_64.pkg.tar.zst`
  (154MB) + headers `.pkg.tar.zst` (43MB), both timestamped
  May 25 00:54 — the build had finished ~7h *before* the 07:50
  pass referenced it as still-running. The handshake PRD itself
  is still well-founded (the orphan-for-PID-917 race is real and
  documented in `~/brain/journal/2026-05-25.md` §Notable); only
  the third supporting fact was stale. Strengthens the
  freshness-self-changelog / freshness-doc-sweep Fleet 2 case AND
  surfaces a meta-finding: /dream's own gossip drafts are not
  exempt from drift. A `freshness-on-dream` Fleet 2 entry should
  spot-check claims made in fresh gossip drafts before they're
  committed.
- 2026-05-25 (~09:15, /dream pass 9, third no-fleet-pass in a row)
  — **orphan-PRD desync**. `PRD-recall-braid-freshness-tunable.md`
  Status line reads "Draft v0.1" but the work shipped as recall
  v0.4.3 (commits 2df7156 + fdc81ad); Cargo.toml = 0.4.3; CHANGELOG
  has the v0.4.3 section; PRD file still in queue dir, not archived.
  Two sibling PRDs (`PRD-recall-bash-response-richness.md`,
  `PRD-recall-stop-hook-session-id.md`) are also orphan drafts
  ("Builds on: recall v0.4.2", no vision, no manifest entry).
  Strengthens the freshness-on-prds Fleet 2 case: a PRD's
  frontmatter is itself a claim about the world — "I am Draft v0.1"
  is verifiable against `git log --grep <slug>` + live Cargo.toml
  / CHANGELOG.md. When the assertion is falsified, the PRD belongs
  in PRDs-archive/. A simple `recall doctor --check-claims` rule
  ("PRD says Status: Draft and Builds on: vX.Y.Z but live version
  > vX.Y.Z → propose archive") covers this without new tooling.
  Pattern note: three orphan PRDs orbiting v0.4.x is the smallest
  recurrence count where "pattern" is honest; one more accumulation
  unblocks a fleet candidate (recall-pulse or freshness-on-prds).
- 2026-05-26 (~06:20, /dream pass 10, first draft pass post-arc) —
  **PRD manifest mis-cites adjacent PRD as resolution path**.
  recall-daemon iter-15 manifest entry (2026-05-26T05:40:29Z) says
  "Resolution path: PRD-build-publish-allowlist.md (queued,
  build_auto:false, build_priority:high) explicitly targets this"
  — but publish-allowlist line 164 reads "Settings-json edits for
  `git push origin main` — separate gate, hit previously by
  recall-daemon iter-8, separately resolved via interactive
  authorization. Out of scope here." The PRD author flagged push
  as out-of-scope; the /build manifest claims the opposite. Two
  refutable assertions about the same PRD in adjacent files,
  contradicting each other. Caught at /dream pass 10 by reading
  publish-allowlist end-to-end before drafting the push-allowlist
  sibling. Strengthens freshness-on-prds Fleet 2 case in a new
  way: PRDs' OWN claims (Status, Builds on, Out of scope,
  Coordination notes) are themselves verifiable — when a /build
  iter manifest cites another PRD as resolution, the claim should
  be cross-checked against the cited PRD's body, not just its
  title. Pattern: freshness-on-prds should not just check
  PRD-vs-shipped-state, but also PRD-vs-PRD cross-references
  (manifest cites X → does X actually cover the case being
  resolved?).
- 2026-05-25 (~10:45, /dream pass 11, fifth no-fleet-pass in a row)
  — **the 09:15 entry above was itself partly false**. Live re-check
  at 10:45Z found `recall-braid-freshness-tunable` has been in the
  build manifest's `prds` object since iter-1 at 04:50Z — 4h17m
  before pass 9 called it an "orphan ship" with the claim "NOT in
  the build manifest's prds object." Manifest shows status=
  in_progress (3 ticks), shipped_version=0.4.3, changelog_committed=
  fdc81ad, installed_versions={recall: 0.4.3, recalld: 0.4.3}. The
  PRD's frontmatter still reading "Draft v0.1" remains true; the
  archival step is what's pending, not the manifest tracking. Likely
  cause of the 09:15 misread: I ran `cat manifest | head -100` and
  the recall-braid-* entry sits after the first five PRDs
  alphabetically, below the window. Meta-finding: when /dream
  cites manifest contents, it must read the *whole* JSON object
  (or `jq '.prds | keys'`), not truncated previews. This is the
  second instance of /dream's own gossip drafting stale claims
  about live state — the freshness-on-dream Fleet 2 candidate
  now has 2/2 hit rate within 6h. Strongest evidence yet that the
  candidate is real. Pattern of meta-instances:
    * pass 8 (08:30): "kernel build PID 12146 still running" —
      finished 7h prior.
    * pass 11 (10:45): "PRD not in manifest" — in manifest 4h+ prior.
  Both off by hours; both rooted in citing yesterday's snapshot as
  today's truth without a re-read. The freshness-on-dream sketch
  should mandate a "ground-truth pass" *immediately before commit*,
  not at draft time, so the elapsed-while-writing gap closes.
