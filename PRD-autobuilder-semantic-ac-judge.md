# PRD: autobuilder-semantic-ac-judge — LLM judges whether each test exercises its AC's stated behavior

**Status:** Draft v0.1
**build_target:** mixed
**build_priority:** high
**build_into:** /home/jsy/wintermute/ac-judge
**Research:** research/quality-verification-2026-05-28.md §3, §4 Test 3
**Created:** 2026-05-28
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

A small Rust CLI `ac-judge` takes a PRD path + a crate root. For each
declared AC, it pairs the AC's English text with the test file that
claims to verify it, sends both to Claude (Sonnet 4.6, prompt-cached
system), and asks two strict questions: (1) does the test exercise the
behavior the AC describes? (2) is the test asserting the AC's stated
invariant, or merely re-running the impl and confirming its return?

Verdicts land in a new 9th receipt at
`target/autobuilder/ac-semantic-judge.json`. Stage 4 blocks if any AC
has `behavior_match: no` OR `assertion_kind: restates-impl` with
confidence ≥ 0.7.

Mixed target: rust-cli builds the judge binary; self-mod step wires
it into the autobuilder pipeline.

## Why this exists

The Stage 3 adversarial sub-agent already tries to falsify ACs by
writing adversarial tests — but it operates inside the same /autobuilder
session as the edit-agent. A separate judge call, with a different
prompt and a clean context window, catches a different failure mode:
**AC-text-↔-impl-semantics mismatch**. The AC says X; the impl does
X-but-with-a-quiet-Y; the test asserts X. cargo test green, mutation
test green, adversarial test green, semantic check fails.

Research report §3 catalogs this failure mode; §4 Test 3 details the
design.

The complementary check to mutation testing: mutation asks "would the
test catch a broken impl?" The judge asks "does the test even check the
*right thing*?"

## What this builds

### Artifact 1: Rust CLI

A new crate at `~/wintermute/ac-judge/` (LOC budget ≤500 src, ≤300
tests). Single binary `ac-judge`.

Dependencies: `clap`, `serde`, `serde_json`, `ureq` (sync HTTP),
`sha2`, `regex`.

CLI:
```
ac-judge run --prd <path> --crate-root <path> [--model <id>]
  → emits target/autobuilder/ac-semantic-judge.json
  → exit 0 if all ACs pass the judge
  → exit 4 if any AC fails (behavior_match:no OR assertion_kind:restates-impl, conf>=0.7)
ac-judge calibrate --golden-set <dir>
  → runs the judge against hand-curated AC-↔-test pairs labeled good/bad
  → reports false-positive + false-negative rates
ac-judge show --slug <ac>   # pretty-print one verdict from the most recent run
```

### Artifact 2: pair detection

Given a PRD with numbered ACs (`**AC1**: ...`, `**AC2**: ...`), pair
each AC to its test by these heuristics (first match wins):
1. `tests/ac<N>_*.rs` filename match (matches today's autobuilder
   convention).
2. `tests/acceptance_ac<N>.rs` match (the older convention seen in
   agorabus / episodic-observer).
3. A `#[test]` function whose name starts with `ac<N>_` in any
   test file.
4. Falls through to `unpaired` if no match → recorded with
   `behavior_match: no, reason: "no paired test found"`.

### Artifact 3: judge prompt

System block (cached, ~1500 input tokens):

> You are an independent reviewer judging whether a Rust test
> exercises the behavior its acceptance criterion describes. You will
> receive (1) the AC's English text from a PRD, and (2) the full
> source of the test that claims to verify it.
>
> Answer in strict JSON only:
>
> ```json
> {
>   "behavior_match": "yes" | "no" | "partial",
>   "assertion_kind": "asserts-invariant" | "restates-impl" | "mixed",
>   "confidence": 0.0..1.0,
>   "reasoning": "<1-2 sentences>"
> }
> ```
>
> "asserts-invariant" means the test asserts a property the AC's
> English describes (e.g., "output ends with cut bytes" → asserts the
> last bytes are `0x1D 0x56 0x42 0x00`). "restates-impl" means the
> test calls the function and asserts the function returned what the
> function returned (tautological).

Ephemeral block (per AC): AC text + test source + crate name + AC index.

Cached system + few-shot drives cost to ~$0.005/AC at Sonnet rates.
Per-crate cost (10 ACs avg): ~$0.05. Annual cost across ~365 crates:
~$20.

### Artifact 4: calibration golden set

Ship `~/wintermute/ac-judge/golden/` with 20 hand-curated AC-↔-test
pairs:
- 10 known-good (`behavior_match: yes, assertion_kind: asserts-invariant`)
- 5 known-bad (`assertion_kind: restates-impl`)
- 5 partial (`behavior_match: partial`)

`ac-judge calibrate --golden-set golden/` runs the judge against
all 20 and reports the confusion matrix. CI gate: false-positive rate
< 0.10, false-negative rate < 0.20.

### Artifact 5: autobuilder integration (self-mod step)

After Stage 3 hard gates pass, autobuilder runs `ac-judge run`. New
Stage 4 row added to the receipt table:

| `ac-semantic-judge` | new ac-judge binary | every AC verdict has `behavior_match != no` AND not (`assertion_kind == restates-impl` AND `confidence >= 0.7`) |

Stage 4 blocks (with a clear per-AC diagnostic) if the gate fails.

## Acceptance criteria

- **AC1**: `ac-judge run` against the agorabus crate produces a
  verdict for each declared AC (5 verdicts). Verdicts are JSON-valid
  and conform to the schema.
- **AC2**: Against a synthetic crate where AC1 is paired to a test
  that just calls the function and asserts its return value,
  `ac-judge run` emits `assertion_kind: restates-impl` for AC1 with
  `confidence >= 0.7` and exits 4.
- **AC3**: Against a synthetic crate where AC1 is paired to a test
  that asserts the PRD's English invariant on the result,
  `ac-judge run` emits `assertion_kind: asserts-invariant` and
  exits 0.
- **AC4**: Pair detection finds tests via `tests/ac1_*.rs` AND
  `tests/acceptance_ac1.rs` AND `#[test] fn ac1_*` patterns.
  Tested with 3 fixture crates each using one convention.
- **AC5**: Missing test for AC2 (`unpaired`) emits a verdict with
  `behavior_match: no, reason: "no paired test found"` and exits 4.
- **AC6**: Calibration: `ac-judge calibrate --golden-set golden/`
  against the 20 hand-curated pairs yields false-positive < 0.10
  AND false-negative < 0.20. (Re-run this gate every time the
  judge prompt or model changes.)
- **AC7**: System prompt block carries
  `cache_control: {"type": "ephemeral"}` (verified by inspecting
  request bytes via a mock server).
- **AC8**: Missing `$ANTHROPIC_API_KEY` exits 6 immediately, no
  network call attempted.
- **AC9**: Receipt JSON validates against
  `schemas/ac-semantic-judge.schema.json` (ship the schema with
  this PRD).
- **AC10**: Per-AC wall-clock <5s at Sonnet 4.6 default latency;
  measured over 10 fixture ACs.
- **AC11**: Backfill: running the judge against the 3 most-recently-
  shipped crates (agorabus, episodic-observer, day-haiku) produces
  verdicts for every AC and the verdicts are committed alongside this
  PRD's archive commit for calibration.
- **AC12**: Self-mod: `~/.claude/skills/autobuilder/SKILL.md` Stage 4
  table gains the `ac-semantic-judge` row; Stage 3 step 11 calls
  `ac-judge run` after hard gates pass.

## Files

```
~/wintermute/ac-judge/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── install.sh
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── pair.rs            # AC ↔ test pair detection
│   ├── prompt.rs          # system + few-shot assembly
│   ├── api.rs             # Anthropic client (sync ureq)
│   ├── schema.rs          # verdict + receipt JSON
│   └── calibrate.rs       # golden-set runner
├── golden/                # 20 hand-curated pairs (5 dirs × 4 pairs each)
└── tests/
    ├── ac1_basic.rs
    ├── ac2_restates_impl.rs
    ├── ac3_asserts_invariant.rs
    ├── ac4_pair_detection.rs
    ├── ac5_unpaired.rs
    ├── ac6_calibrate.rs
    ├── ac7_cache_control.rs
    ├── ac8_no_api_key.rs
    ├── ac9_schema.rs
    └── ac10_perf.rs

~/.claude/skills/autobuilder/
├── schemas/ac-semantic-judge.schema.json   # new
└── SKILL.md                                 # +Stage 3 step + Stage 4 row
```

## Non-functional

- **Default model: `claude-sonnet-4-6`** — *intentionally different
  family from /autobuilder's pipeline default* (Opus 4.7; inherited
  via `claude -p "/build"` with no `--model` flag in
  `~/.local/bin/claude-build-headless.sh`). The independence is
  load-bearing: the same model that wrote the test should not also
  judge whether the test verifies its AC. Decision confirmed
  2026-05-28 by jsy.
- Privacy: only AC text + test source sent to API; no other PRD body,
  no journal, no env.
- No streaming. Sync, blocking, one-shot per AC.
- Re-running on identical AC + test (same SHA) returns cached verdict
  from `target/autobuilder/ac-judge-cache/`. Cache key:
  `sha256(ac_text + test_source + model + prompt_version)`.

## Open question (calibration only — model choice settled)

Calibration data (AC6) will tell us if Sonnet 4.6 is *good enough* at
the judge task. If false-positive rate exceeds 0.10, escalate to
Opus 4.7 (matches pipeline, costs ~10× more); if cost matters more
than verdict quality, drop to Haiku 4.5. v0.1 ships with Sonnet.
