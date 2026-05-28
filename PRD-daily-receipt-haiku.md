# PRD: daily-receipt-haiku — Claude composes the workday haiku from today's signals

**Status:** Draft v0.1
**build_target:** rust
**build_into:** /home/jsy/wintermute/day-haiku
**build_version_bump:** N/A (new crate)
**Vision:** visions/daily-receipt.md
**Depends on:** PRD-daily-receipt-summarize.md (consumes summary.json)
**Synergistic with:** PRD-daily-receipt-printer.md (produces what `receipt today --content` wants)
**Created:** 2026-05-27
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

A small Rust binary `day-haiku` reads `summary.json` (from
day-summarize), calls Claude via the Anthropic API with a system
prompt + past-Claude voice few-shot, and emits a three-line haiku
into `content.json` (the shape `daily-receipt render --content`
expects). Includes prompt caching of the system + few-shot blocks,
a `--re-roll` flag for the original PRD's veto path, and a strict
schema guard so a malformed model response can never crash the
nightly print.

This is the "art" half of the daily receipt arc. day-summarize is
the dispassionate signal gather; day-haiku is the composition.

## Why this exists

Phase 1 research, 2026-05-27:

- `~/wintermute/daily-receipt/PRDs-archive/PRD-daily-receipt.md` §4
  named the box: "Claude composes; you can veto and re-roll once."
  Section 9.1 noted "Haiku-as-AI-output is a cliché. Mitigation:
  discipline the prompt; iterate; allow you to redraw."
- The `letter` CLI at `~/wintermute/letter/` (used by confidant) has
  the past-Claude / future-Claude voice convention. Borrow that
  voice for haiku composition so the daily strip and the weekly
  letter and the year-end strip all sound like the same agent.
- `~/.claude/skills/claude-api/` exists and prescribes the
  Anthropic SDK conventions on this laptop. Reuse: prompt caching
  (system + few-shots cached; daily summary is the only ephemeral
  block, so the cache hits every day after the first one).
- Cost ceiling: at Sonnet 4.6 input rates with ~2K cached tokens +
  ~500 ephemeral tokens + ~50 output tokens, the daily call lands
  at well under $0.01/day. Annual cost <$4. Not a budget concern.

## What this builds

### Crate shape

- Name: `day-haiku`
- Binary: `day-haiku`
- LOC budget: ≤400 src, ≤300 tests.
- Dependencies: `clap`, `chrono`, `serde`, `serde_json`,
  `anthropic-sdk` (or `reqwest` + hand-rolled — pick one in v0.1).
  Plus `tokio` if async, else `ureq` for sync simplicity (preferred).

### CLI

```
day-haiku today                # read today's summary, write today's content
day-haiku today --re-roll      # discard cached haiku, ask Claude again
day-haiku today --dry-run      # print the prompt + would-be content; no API call
day-haiku today --summary <path>
day-haiku today --out <path>
day-haiku today --voice <name> # default: past-claude; future: jsy-haiku, etc.
day-haiku show                 # print today's content.json
```

### Default paths

- Input: `$DAILY_RECEIPT_SUMMARY_DIR/<today>.json`
  (default `~/.local/state/daily-receipt/summaries/`)
- Output: `$DAILY_RECEIPT_CONTENT_DIR/<today>.json`
  (default `~/.local/state/daily-receipt/contents/`)

Matches PRD-daily-receipt-printer's lookup convention.

### `content.json` schema

```json
{
  "date": "2026-05-27",
  "day_type": "workday",
  "haiku": [
    "Tear bar, no cutter —",
    "the IP1000 hums on,",
    "paper not yet here."
  ],
  "model": "claude-sonnet-4-6",
  "produced_by": "day-haiku",
  "produced_at": "2026-05-27T21:30:00-07:00",
  "re_roll_count": 0,
  "input_summary_sha256": "abc123…"
}
```

The renderer reads `haiku` (exactly three lines, each 1..=40 visible
chars per daily-receipt's AC5). Everything else is provenance.

### Prompt structure

System block (cached, never changes day-to-day):

> You are Claude, on jsy's laptop, composing one short haiku per day
> for a thermal-strip ritual. The form: exactly three lines, each
> ≤40 characters of visible text. The voice: past-Claude addressing
> future-Claude (and future-jsy reading the scroll years later).
> Concrete over abstract. Specific signals over generic mood.
> No emoji, no quotes, no commentary, no titles. Output strictly:
>
> ```
> line one
> line two
> line three
> ```

Few-shot block (cached, updates roughly monthly when we have better
exemplars):

- 4–6 hand-curated example pairs: `(summary excerpt, haiku)`.
  Seed exemplars derived from the original PRD's tone notes; replace
  with real past haikus once the system has run for a month.

Ephemeral block (changes daily):

- A compact rendering of `summary.json`: commit count, distinct
  repos, top ctrace prefixes, journal-first-heading. ~300 tokens.

### Voice file

`~/.claude/daily-receipt/voices/past-claude.toml`:

```toml
name = "past-claude"
system = """
…multi-line system prompt…
"""

[[examples]]
summary = "12 commits across recall and autobuilder; ctrace shows heavy /tmp writes; journal: kernel pkg rebuild"
haiku = [
  "Twelve commits and",
  "ten thousand /tmp writes —",
  "the kernel rebuilds itself."
]

[[examples]]
…
```

Multiple voices ship later (jsy-style, etc.); v0.1 ships one.

### Re-roll cache

When `day-haiku today --re-roll` runs, the previous content is
preserved at `<contents>/<today>.json.rolled-<N>` (atomic rename
before re-ask). The user can `day-haiku show --rolled 0` to see the
original, e.g. for an "I should have kept the first one" recovery.

### Schema guard

Before writing `content.json`, validate:
- `haiku` is an array of exactly 3 strings.
- Each string is 1..=40 visible chars (count after stripping leading/
  trailing whitespace; visible = grapheme cluster count, not bytes).
- No string contains newlines or ESC bytes.

On failure: do NOT write content.json. Exit 3 (matching
daily-receipt's render-error code). Stderr message names the
violation and includes the raw API response for debugging.

### Network failure handling

- Timeout: 30s.
- 4xx: exit 4, stderr explains the API error. Do not retry.
- 5xx / network: one retry with 5s backoff. Then exit 5.
- No API key (`$ANTHROPIC_API_KEY` missing): exit 6 with a hint.

In all failure paths, content.json is not written; the printer
falls back to the deterministic glyph for that day. The ritual
never breaks; some days just get glyphs.

## Acceptance criteria

- **AC1**: `day-haiku today --dry-run` with a fixture summary prints
  the assembled prompt to stdout (system + few-shot + ephemeral) and
  exits 0. No API call.
- **AC2**: With a mock HTTP server returning a well-formed haiku
  response, `day-haiku today` writes `content.json` with `haiku`
  array length exactly 3, each line ≤40 graphemes. Test uses a
  loopback `httpmock`-equivalent.
- **AC3**: Same mock returns an ill-formed response (4-line haiku).
  `day-haiku today` exits 3, writes no content.json, stderr names
  the violation.
- **AC4**: `--re-roll` after a prior successful run renames the old
  file to `<today>.json.rolled-0` (atomic; verified via stat ino),
  then writes a new `<today>.json`. Subsequent re-roll → `rolled-1`.
- **AC5**: System + few-shot blocks are sent with
  `cache_control: {"type": "ephemeral"}` (verified by intercepting
  the request body in the mock). Daily summary block is uncached.
- **AC6**: Missing `$ANTHROPIC_API_KEY` exits 6 immediately, no
  network call attempted. Stderr names the env var.
- **AC7**: `input_summary_sha256` in the output matches
  `sha256(summary.json bytes)`. Tested via fixture summary with
  known hash.
- **AC8**: `--summary <path>` overrides the default lookup; works
  even when no file exists at the default path.
- **AC9**: `day-haiku show` prints the current `<today>.json` to
  stdout pretty-formatted; `show --rolled 0` prints the prior roll.
  Missing target file exits 0 with empty stdout — not an error.

## Files this will create

```
~/wintermute/day-haiku/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── api.rs          # Anthropic API client (sync ureq)
│   ├── prompt.rs       # system + few-shot assembly
│   ├── schema.rs       # content.json + voice TOML
│   ├── validate.rs     # 3-line, 40-char guards
│   └── reroll.rs       # atomic rename cache
├── voices/
│   └── past-claude.toml
└── tests/
    ├── ac1_dryrun.rs
    ├── ac2_happy_path.rs
    ├── ac3_malformed_response.rs
    ├── ac4_reroll.rs
    ├── ac5_cache_control.rs
    ├── ac6_missing_key.rs
    ├── ac7_input_hash.rs
    ├── ac8_override_summary.rs
    └── ac9_show.rs
```

## Non-functional

- **Model**: default to `claude-sonnet-4-6` (per claude-api skill's
  defaults). Override via `--model` for testing against haiku/opus.
- **Privacy**: never send `~/.claude/CLAUDE_SELF.md` or full journal
  bodies. Only the structured summary fields. The ephemeral block is
  small enough to read and audit in `--dry-run`.
- **Determinism**: API responses are not deterministic. The hash in
  `content.json` is of the *input*, not the output, so we can detect
  "Claude was asked the same thing twice" cases.
- **Re-roll budget**: open question — v0.1 allows unbounded; if it
  becomes a problem, cap at 3/day.

## Out of scope (v0.1)

- Multi-voice rotation. `--voice past-claude` is the only voice.
- Image / glyph generation via API. Quiet days remain
  deterministic-glyph-only (daily-receipt v0.1 already does this).
- Streaming responses. v0.1 is sync, blocking, one-shot.
- Tool use. No tools, just a prompt + response.

## After this lands

PRD-daily-receipt-stamps adds the third leg (special days). All
three day-types are now end-to-end. PRD-daily-receipt-archive can
start consuming a year's worth of `content.json` plus
`summary.json` files as the rich form of the year's record.
