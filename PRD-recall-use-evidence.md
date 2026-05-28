# PRD: recall-use-evidence — at session end, detect which surfaced memories Claude actually used

**Author:** Claude (Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** [visions/fidelity.md](visions/fidelity.md)
**Depends on:** [PRD-recall-surfaced-tracking.md](PRD-recall-surfaced-tracking.md) shipped (surfaced_count column + surfaced.json hook write)
build_target: rust-extend
build_into: /home/jsy/wintermute/recall
**Version target:** `recall v0.7.2` (minor — new `recall use-detect`
subcommand introduces transcript scanning).

---

## TL;DR

Once surfaced_count exists, the next question is: of the memories the
hook surfaced this session, which ones did Claude *actually use*?
Today there's no signal at all. This PRD adds `recall use-detect
--session <sid>` — a transcript-scanning subcommand that, given a
session id, opens the Claude Code session JSONL at
`~/.claude/projects/-home-jsy/<uuid>.jsonl`, finds the surfaced.json
ids, and detects use evidence per id via two heuristics:

1. **N-gram match**: the memory body's most-distinctive 5+ word n-gram
   appears in any assistant text turn from that session.
2. **API recall**: the memory id appears as an output line of any
   `recall query` or `recall show` invocation captured in the session
   transcript (Bash tool result blocks).

Writes `used.json` alongside `surfaced.json` for the session. **Still
no behavior change in feedback** — the discriminate step is the next
PRD. This is the signal collection slice.

---

## 1. Why this exists

1. **Use-evidence is the missing signal.** Fidelity vision §End-state
   #1-3: surfacing without use must be observable separately from
   surfacing with use. PRD-recall-surfaced-tracking collected the
   surfaced events; this PRD collects the used events.
2. **Claude's transcript already contains the evidence.**
   `~/.claude/projects/-home-jsy/<uuid>.jsonl` is the canonical record
   of every user message, assistant message, and tool result for a
   session. Any time Claude used a memory, the use is visible there:
   either Claude echoed body text from the memory, or Claude called
   `recall query` and the result was pulled back into the context.
3. **No external service needed.** Transcript scan is local, fast
   (sequential JSONL parse), and idempotent. Worst case a long session
   is a few MB; pass-through scan is <200ms.
4. **N-gram match is conservative-on-purpose.** False negatives
   (paraphrase) are fine: the next PRD treats false-negatives as
   `--abstain`, which is a smaller correction than the current
   blanket `--accept`. We'd rather under-credit than over-credit.
5. **The session JSONL UUID maps directly from Stop hook session_id.**
   The Claude Code harness uses the same UUID for the JSONL filename
   and the `.session_id` value passed to hooks. (Verify in AC1.)

---

## 2. What this builds

### 2.1 New subcommand: `recall use-detect`

```
recall use-detect --session <sid> [--transcript-dir <dir>] [--format text|json]
```

- Loads `surfaced.json` from `$RECALL_WEATHER_DIR/<sid>/surfaced.json`
  (falls back to `~/.cache/recall-weather/<sid>/`).
- Resolves transcript path: `<transcript-dir>/<sid>.jsonl` where
  `<transcript-dir>` defaults to `~/.claude/projects/-home-jsy/`.
- For each surfaced id:
  1. Loads the memory body from disk.
  2. Extracts the longest distinctive 5+ word n-gram from the body
     (skip stopword-heavy n-grams; prefer rare-word n-grams).
  3. Scans transcript JSONL for the n-gram in any `assistant` role
     text content.
  4. Scans transcript for `recall query` or `recall show` Bash tool
     results that contain the id.
  5. Marks the id as "used" if either heuristic fires.
- Writes `used.json` to
  `$RECALL_WEATHER_DIR/<sid>/used.json` as a JSON array of used ids.
- Prints summary to stdout (text or json format).

### 2.2 N-gram extraction

`src/use_detect.rs` (new module):

```rust
pub fn distinctive_ngram(body: &str, n: usize) -> Option<String> {
    // 1. Tokenize body (Unicode word boundaries, ASCII lowercase fold).
    // 2. Slide n-window, score each by sum of word rarity (inverse
    //    log frequency from a 100-word stopword list).
    // 3. Return the top-scoring n-gram as a space-joined string.
    // None if body too short.
}
```

Stopword list ships embedded (top 100 English; ~1KB). Score function
favors n-grams without stopwords. n=5 is the default; configurable
via `--ngram-len`.

### 2.3 Transcript scan

`src/use_detect.rs`:

```rust
pub fn scan_transcript(
    path: &Path,
    surfaced: &[SurfacedMemory],
) -> Result<Vec<String>> {
    // 1. Read JSONL line-by-line (BufReader).
    // 2. For each line, parse minimal struct: { role, content, tool_use? }.
    // 3. If role == "assistant", check each surfaced memory's n-gram
    //    against the content text. Mark id as used on first hit.
    // 4. If role == "tool_result" AND the parent tool_use was a Bash
    //    invocation of `recall query|show`, scan the result content
    //    for surfaced ids as literal substring.
    // 5. Return deduped list of used ids.
}
```

### 2.4 Stop hook integration

Stop hook (`~/.claude/scripts/recall-stop.sh`): after the surfaced
increment from PRD #1, invoke `recall use-detect --session "$sid"`.
Best-effort, silent on failure. The Stop hook does NOT yet
read used.json — the next PRD does. This PRD only ensures it gets
written.

### 2.5 Out of scope

- **No semantic matching.** Embedding-based use detection is a v2
  proposal; v1 is n-gram only.
- **No multi-session aggregation.** This subcommand handles one
  session at a time.
- **No behavior change in feedback.** Used.json is written but not
  consumed yet.
- **No retroactive scan.** Surfaced sessions from before the
  surfaced.json hook landed (PRD #1) have no surfaced.json, so
  use-detect returns empty. The 158 existing weather session dirs
  stay as-is.

---

## 3. Acceptance criteria

1. **AC1 — session_id maps to transcript JSONL.** Given a live
   session, the file
   `~/.claude/projects/-home-jsy/<session_id>.jsonl` exists and is
   readable. Smoke test from a real session.
2. **AC2 — n-gram extraction picks distinctive content.** Test:
   `use_detect::tests::ngram_skips_stopwords` — body "the user
   has been working on the recall feedback weather module since
   2026-05-25" should pick a 5-gram from the rare-word region
   ("recall feedback weather module since" or similar) rather than
   "the user has been working".
3. **AC3 — transcript scan detects n-gram in assistant text.**
   Synthetic test: transcript with one assistant turn containing
   "feedback weather module since 2026-05-25"; surfaced memory
   whose body 5-gram is "feedback weather module since 2026"; AC
   passes if scan returns this id.
4. **AC4 — transcript scan detects id in `recall query` result.**
   Synthetic test: transcript with a Bash tool_result containing a
   surfaced id in a `recall query` output; AC passes if scan
   returns it.
5. **AC5 — `used.json` is written even if no ids used.**
   `[]` is a valid result; empty result is distinct from "subcommand
   crashed and wrote nothing." Smoke test: invoke against a session
   with surfaced ids none of which match; assert used.json contents
   are `[]`.
6. **AC6 — graceful fallback on missing transcript.** If the
   transcript file doesn't exist, `recall use-detect` exits 0 with
   no used.json written and prints a one-line note to stderr.
   Smoke test against a fake sid.
7. **AC7 — scan completes under 500ms for a 5MB transcript.**
   Synthetic test: generate a 5MB JSONL with 1000 lines; scan
   completes within budget. (Performance gate, not strict — allows
   2× headroom on cold-cache disk reads.)
8. **AC8 — Stop hook calls use-detect after surfaced step.** Smoke:
   run a session with surfaced ids; after Stop, used.json is present
   in the weather dir.

---

## 4. Implementation notes

### 4.1 Minimal JSONL parsing

The Claude Code session JSONL has a flexible schema; we only need:

```rust
#[derive(Deserialize)]
struct TranscriptLine {
    #[serde(rename = "type")]
    line_type: Option<String>,
    message: Option<Message>,
    tool_use_result: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Message {
    role: Option<String>,
    content: Option<serde_json::Value>,
}
```

Use `serde_json::from_str::<TranscriptLine>` per line; ignore parse
errors (skip malformed lines silently). Extract assistant text by
walking the content blocks looking for `{ "type": "text", "text": ... }`.

### 4.2 N-gram-as-byte-substring search

After lowercasing both the n-gram and the assistant text,
`text.contains(&ngram)` is sufficient. No regex engine; no
allocation per line beyond the lowercase fold of the line.

### 4.3 Idempotency

If `used.json` already exists, `use-detect` overwrites (last writer
wins; Stop hook fires once per session). Don't try to merge.

### 4.4 Logging

Emit a JSON summary to stdout in the format:

```json
{
  "session_id": "<sid>",
  "surfaced": 5,
  "used": 1,
  "ngram_hits": 1,
  "id_hits": 0,
  "transcript_bytes": 423819,
  "scan_ms": 47
}
```

Useful for future doctor surface (PRD #4).

---

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Stop hook latency budget | Scan budget <500ms (AC7); gated by `[fidelity] use_evidence_scan = true` config (default off in v0.7.2; default on after measurement). |
| False negatives from paraphrase | Acknowledged; downstream treats no-match as abstain (smaller penalty than blanket accept). v2 adds semantic match. |
| False positives from short / common phrases | Stopword-skipping n-gram extraction (AC2). N-gram length default 5 raises specificity. |
| Transcript file path differs from session_id | AC1 verifies the mapping; fallback to "no use signal, write []" if mapping fails. |
| Concurrent sessions writing same weather dir | Already not possible — weather dir is per-sid; sids are unique. |

---

## 6. Phasing

- **v0.7.2** (this PRD): new subcommand, n-gram extraction,
  transcript scan, used.json write, Stop hook invocation. Behavior
  unchanged; signal collection only.
- v0.7.3 (next: recall-stop-hook-discriminate): consume used.json
  to discriminate `+accept` from `+abstain`.
