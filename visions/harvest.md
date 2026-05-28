# Vision: harvest

> The Stop hook plants candidates. Nobody comes back to gather them.
> Harvest is the discipline of closing the loop on signals we already
> collected — either they become memories, or they decay with a note.

**Status:** Active
**Created:** 2026-05-28
**Seed:** Phase 1 of this dream — `/home/jsy/.claude/scratch/learning-candidates/`
  has 3 drafts as of 2026-05-28T08:00Z, all from session
  6554d28b-de1c-4e15-9d23-ff4f2073d45d, all written by today's
  `recall-learning-candidate.sh` Stop hook. The SessionStart hook
  surfaces them at every fresh session (visible in the
  startup banner of *this* session). No consumer skill exists.
  `grep learning-candidate` across `~/wintermute/autobuilder/*.md` +
  `visions/*.md` → zero hits. The producer was wired without the
  consumer.
**Pace:** opt-in (default — `build_auto` omitted; pickup explicit)

## TL;DR

`recall-learning-candidate.sh` (Stop hook) writes one markdown draft
per session that matched any of ~15 patterns ("save as feedback",
"turns out", "always use", …). `learning-candidates-start.sh`
(SessionStart hook) surfaces them at startup with the message *"Review
and either `recall write` a memory or delete the draft."* But there is
no `/triage` skill, no `recall triage` subcommand, no scheduled
sweeper. The drafts accumulate. The signal that the hook went to the
trouble of capturing rots unconsumed.

Harvest closes that loop with three small shell/skill PRDs:

1. **Triage** — the consumer. Walk the queue, classify each draft as
   save/discard with reasoning, act.
2. **Prefilter** — sharpen the producer so the queue is smaller and
   higher-signal. Today three drafts from one session with 1–2 hits
   each is already noise; left unfixed it scales linearly.
3. **Prune** — bound the inbox. Drafts not acted on within 7d get
   dropped with a one-line journal note recording the lost signal.

## End-state

When harvest ships:

- The `learning-candidates/` directory tends toward empty: every draft
  is either promoted into recall (via `recall write`) or pruned, never
  silently rotting.
- The producer hook emits fewer, higher-quality drafts (today's 3-per-
  session rate drops to ≤1-per-session on average; sessions that didn't
  meaningfully learn produce zero drafts).
- `recall query` results improve because the durable learnings the user
  flagged ("save as feedback", "always use", …) actually land in the
  store instead of dying on disk.
- The SessionStart banner stops showing the same 3 stale drafts on every
  fresh session — backlog clears, the banner becomes a useful nudge again
  instead of background noise.
- A user invocation `/triage` runs the queue interactively; an
  unattended automated path (timer or /self-review Phase D action)
  handles obviously-save and obviously-discard drafts; the ambiguous
  middle still asks for human judgement.

## Components

- **PRD-learning-candidate-triage.md** (Fleet 1, this pass) — the core
  consumer. A `/triage` slash command at `~/.claude/skills/triage/`
  that walks `~/.claude/scratch/learning-candidates/*.md`, presents
  each draft with full session context, and lets Claude classify as
  save / discard / defer with reasoning. On save: `recall write` with
  inferred kind/subject/confidence + delete the draft. On discard:
  `rm` + append a one-line journal note. On defer: skip until the
  next pass. One draft at a time; interactive by default.

- **PRD-learning-candidate-prefilter.md** (Fleet 1, this pass) — the
  producer-side sharpening. Replaces `recall-learning-candidate.sh`'s
  single-match-creates-draft heuristic with a scored threshold that
  weights explicit imperatives ("save as feedback") higher than
  observational phrases ("turns out"), and requires either ≥2 distinct
  patterns OR ≥1 imperative-pattern before emitting a draft. Goal:
  today's 3-drafts-per-session from common-word matches drops to
  ≤1-per-session on average; the false-positive rate goes down without
  the false-negative rate going up.

- **PRD-learning-candidate-prune.md** (Fleet 1, this pass) — the
  bounded-inbox guarantee. A small shell script invoked by either
  /self-review Phase D or a daily systemd-user timer that deletes any
  draft older than 7d (configurable), appending a one-line note to
  `~/brain/journal/YYYY-MM-DD.md` recording the slug + the lost
  signal's matched patterns. No draft accumulates forever.

## Order

1. **triage** first — it defines the consumer surface; without it
   prefilter and prune are filing-the-papers without a destination.
2. **prefilter** second — once triage exists and we know what
   ambiguity looks like in practice, the prefilter thresholds can be
   tuned with real data instead of guessing.
3. **prune** third — last because we want at least one human-paced
   triage cycle to establish what "7d untouched" actually means before
   automating deletion.

No hard deps between PRDs (each can ship independently), but the
dependency ordering above is the recommended pickup order so /build
has the most context when it gets to the harder ones.

## Open questions

- Should the SessionStart `learning-candidates-start.sh` hook stop
  surfacing drafts after triage exists, and instead recommend
  `/triage`? Today it lists them verbatim; that's noisy once the
  consumer exists. Leave for the triage PRD to decide.
- Auto-promote — for drafts above a high-confidence threshold (e.g.,
  pattern includes "save as feedback" AND the user's prompt itself
  contained a direct preference statement), should triage skip review
  and just `recall write` immediately? Captured as a stretch goal in
  the triage PRD; if practice shows the ambiguous-middle is most of
  the volume, defer to a successor PRD-learning-candidate-auto-promote.
- Categorization labels — when `recall write`-ing from triage, what
  determines kind/subject/confidence? The triage PRD will spec a
  conservative default (kind=feedback when imperative-pattern matched;
  kind=reflective otherwise; confidence=0.6 for save-with-reasoning,
  0.4 for save-on-thin-evidence). Open to revision.

## Notes for /build

- All three PRDs are shell/skill targets; no Rust extends, no new
  repos. Build target is `shell` for triage and prune (new skill +
  new script under `~/.claude/scripts/`); `shell` for prefilter (edits
  to existing `~/.claude/scripts/recall-learning-candidate.sh`).
- No collisions with active recall work (daemon, observer-correlation,
  outcome-feedback, fidelity Fleet 1) — harvest touches only the
  *candidate draft pipeline*, not the recall corpus or schema.
- Triage is the lead; prefilter has the smallest LOC; prune is the
  most mechanical. Pick whichever fits the next tick.
