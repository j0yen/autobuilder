# PRD: build — recognize ground-truth-deferred ACs

**Author:** Claude (Opus 4.7) via /dream pass 13
**Status:** Draft v0.1
**Date:** 2026-05-27
**Builds on:** `/build` skill (verified-completed check #5), scan-prds.sh,
PRD frontmatter conventions
**Vision:** `visions/wintermute.md` § Fleet 1.5 — Maturation & validation
build_target: self-mod
build_priority: high
build_version_bump: none
deferred_acs: [7]
deferred_ac_reasons:
  7: "overtaken by events — archive-trailer.sh mechanism shipped iter-3 (commit 3ab4e03) AFTER the 4 named stuck PRDs (platform/audio/stt/tts) had already archived via the older #[ignore]-stub pairing path. Their archive commits are immutable git history and do not carry Deferred: trailers. The mechanism is ready and will fire on the next PRD whose archive uses it; the literal AC7 grep against those 4 specific commits is unreachable by retroactive edit. Future PRDs that use deferred_acs will populate the greppable trailer ledger that AC7 contemplates."

---

## TL;DR

`/build`'s archive gate (verified-completed check #5: "every AC paired
with a passing test") is structurally unsatisfiable for ACs that require
ground-truth validation — real microphones, live `systemctl` state,
8-hour soak runs, AT-SPI buses, IMAP mailboxes. The wintermute fleet
shipped to GitHub today (2026-05-27) with 5 PRDs published; **4 of them
(platform, audio, stt, tts) are now stuck in `in_progress` purgatory,
all citing the same blocker: "AC pairing incomplete — hardware-dependent
ACs need live validation."** Same pattern, 4 instances, single tick log
line each:

- `wintermute-platform` iter-16: "ACs 1-2 (systemctl/cold-reboot) and
  AC8 (init.backoff event) need live-systemd validation"
- `wintermute-audio` iter-18: `next: ac-pairing-hardware-dependent`
- `wintermute-stt` iter-15: `next: ac-pairing-and-whisper-feature-verification`
- `wintermute-tts` iter-19: "hardware-timing ACs 1/3/5/7 remain
  `#[ignore]`-gated; require local audio"

These PRDs will never archive under the current gate because the gate
cannot be honestly satisfied — there is no source change that turns "AC1
fires within 200ms of a real wake event" into a code-level pairing.

This PRD adds **`deferred_acs:`** to PRD frontmatter so a PRD can
declare, at draft time, which ACs require ground-truth attestation
rather than code-level proof. `/build`'s archive gate then treats those
ACs as paired-by-design and writes a `Deferred:` trailer enumerating
them, so the gap is visible in the archive commit rather than hidden.

This is not a way to ship untested code. It is a way to ship PRDs whose
real-world ACs are genuinely outside `/build`'s reach, and to make that
gap explicit and reviewable.

## Why this exists

Evidence from the wintermute Fleet 1 publish flurry (2026-05-27):

1. **4-PRD identical-shape bottleneck.** All four stuck PRDs cite
   hardware/live-system ACs as the only remaining gate. Tick counts:
   platform 14, audio 18, stt 16, tts 20 — a combined 68 ticks of
   build-time invested across four PRDs that cannot mechanically
   advance further. Source: `~/.claude/skills/build/state/manifest.json`,
   per-PRD `iter_log[-1]` and `next` fields.

2. **The "deferred" intent is already implicit.** Each stuck PRD's test
   suite uses `#[ignore]` on hardware ACs (per wintermute-tts iter-19:
   "hardware-timing ACs 1/3/5/7 remain `#[ignore]`-gated"). The PRDs
   know which ACs are ground-truth; the information just isn't readable
   by `/build`'s gate.

3. **The current workaround is human-mediated.** /build's verified-
   completed check #5 fails → PRD stays `in_progress` → user must
   either (a) accept that a "shipped" PRD is forever non-archived, or
   (b) manually pair an attestation note in the manifest. Neither
   scales across the 7+ Fleet 2 PRDs queued behind this, several of
   which have the same shape (e.g., wm-mail needs a real IMAP server,
   wm-screen-narrate needs a live display).

4. **The gap is masking real verification debt.** Without a declarative
   `deferred_acs:` field, every stuck PRD's `iter_log[-1]` is the only
   place the deferral is recorded. The archive commit will eventually
   say "shipped" without naming what was actually attested vs deferred.
   That's invisible to anyone reading the repo history.

5. **A sibling self-mod just shipped today.** `PRD-build-publish-
   allowlist.md` and `PRD-build-push-allowlist.md` both landed on
   2026-05-27 with the same `build_target: self-mod` / `build_priority:
   high` / single-tick shape. The pattern works; the path is durable.

## What this builds

### 1. Frontmatter field: `deferred_acs:`

A PRD declares the ACs it does not expect `/build` to mechanically
verify:

```yaml
build_target: rust-cli
build_priority: high
deferred_acs: [1, 3, 5, 7]
deferred_ac_reasons:
  1: "wake-to-event latency requires real microphone capture"
  3: "AEC quality needs live PipeWire echo-cancel module"
  5: "8h soak requires live system"
  7: "AT-SPI announce needs running desktop session"
```

`deferred_acs` is a YAML list of integers (the AC numbers from §AC of
the PRD). `deferred_ac_reasons` is an optional dict; when present, each
key must match an entry in `deferred_acs`. Both fields are inert under
the current /build skill — they parse as unknown frontmatter and are
ignored. After this PRD ships, they become load-bearing.

### 2. scan-prds.sh parses the new fields

`~/.claude/skills/build/scan-prds.sh` already extracts `build_target`,
`build_auto`, `build_priority`, etc. via the same yq pipeline. Extend
the emit JSON to include:

```json
{
  "slug": "wintermute-tts",
  "deferred_acs": [1, 3, 5, 7],
  "deferred_ac_reasons": {"1": "...", "3": "..."}
}
```

When `deferred_acs` is absent, emit `[]` (no behavior change for any
existing PRD). When `deferred_ac_reasons` is absent, emit `{}`.

### 3. verified-completed check #5 honors the field

`~/.claude/skills/build/verified-completed.sh` (or whichever script
performs the gate — `/build` Phase 5 documents this in its own README)
gains a new rule:

- For each AC N in 1..NumACs: if N appears in `deferred_acs`, mark
  it `paired = deferred`. Otherwise apply the existing "is there a
  passing test referencing AC N?" check.
- Gate passes when every AC is either `paired = test` or `paired =
  deferred`.
- Gate fails when any AC is `paired = none`.

### 4. Archive commit trailer enumerates deferrals

When /build moves a PRD into PRDs-archive/, the archive commit message
already includes a `Verified-completed:` trailer. Extend it with a
`Deferred:` trailer when `deferred_acs` is non-empty:

```
Verified-completed:
  AC2 — paired with tests/wm_tts_synthesis.rs::synthesis_under_300ms
  AC4 — paired with tests/cli.rs::tts_speak_emits_audio_chunks
  AC6 — paired with tests/proptest_invariants.rs::piper_voice_table
  AC8 — paired with tests/cancel.rs::cancel_drains_within_budget

Deferred:
  AC1 — wake-to-event latency requires real microphone capture
  AC3 — AEC quality needs live PipeWire echo-cancel module
  AC5 — 8h soak requires live system
  AC7 — AT-SPI announce needs running desktop session
```

A reader of the archive commit can now tell at a glance which ACs were
proven and which were declared-deferred. The `Deferred:` lines are
greppable: a future audit script can rebuild a deferred-ACs ledger
across all archived PRDs by walking `git log --grep="^Deferred:"`.

### 5. Backfill the four stuck PRDs

Once the new field lands, edit each of the four currently-stuck PRDs to
declare their deferred ACs (per their own iter logs):

- `wintermute-platform`: `deferred_acs: [1, 2, 8]`
- `wintermute-audio`: `deferred_acs: [list per iter-18 hw notes]`
- `wintermute-stt`: `deferred_acs: [list per iter-15 whisper notes]`
- `wintermute-tts`: `deferred_acs: [1, 3, 5, 7]`

This unblocks `/build`'s next archive tick on each. The backfill is a
PRD-frontmatter edit, not a code change; treat it as part of this
PRD's ship (a one-shot batch edit in the install action).

### 6. Optional: doctor-style audit subcommand

`/build` already has a verbose mode that prints check #5 status per AC.
Extend the human-readable output to distinguish:

```
AC1: DEFERRED — wake-to-event latency requires real microphone capture
AC2: PAIRED   — tests/cli.rs::greet_within_15s
AC3: DEFERRED — AEC quality needs live PipeWire echo-cancel module
AC4: PAIRED   — tests/cli.rs::tts_speak_emits_audio_chunks
```

So a reader can see at the same glance that the gate is satisfied AND
which ACs the gate intentionally let through.

## Anti-scope

- **Not an attestation tool.** This PRD does not add a `wm-verify`
  CLI that lets jsy attest "yes, I plugged in a mic." That is a
  separate, larger PRD (`wm-verify` bullet under Fleet 1.5). This PRD
  is the minimal mechanical change to unblock the current 4-PRD
  bottleneck.
- **Not a default-deferred policy.** A PRD with no `deferred_acs:`
  field behaves exactly as today. Authors must explicitly opt in to
  defer an AC, and ideally explain why via `deferred_ac_reasons`.
- **Not a per-AC test runner.** `/build` already runs `cargo test`;
  this PRD only changes the gate's bookkeeping of what those tests
  cover.
- **Not retroactive to already-archived PRDs.** The 11 already-shipped
  PRDs (recall-daemon, wintermute-bootstrap, etc.) stay archived as-is.
  This PRD only affects archives going forward.

## Acceptance criteria

1. **AC1 — Frontmatter parses.** A PRD with `deferred_acs: [1, 3]` in
   its frontmatter is picked up by `scan-prds.sh`, which emits the
   field in its JSON output. PRD without the field emits
   `"deferred_acs": []`. Verified by adding one test fixture PRD to
   `~/.claude/skills/build/tests/fixtures/` and asserting the parsed
   JSON via `jq`.

2. **AC2 — Gate honors deferral.** A PRD with 4 ACs of which `[1, 3]`
   are deferred and `[2, 4]` are paired-by-test passes verified-
   completed check #5. A PRD with 4 ACs of which `[1, 3]` are deferred
   and `[2, 4]` have no passing test fails verified-completed check
   #5 with a clear "AC4: not paired (and not declared deferred)"
   message. Tests: two fixture PRDs in `tests/`.

3. **AC3 — Archive trailer enumerates deferrals.** Running the
   archive action on a PRD with `deferred_acs: [1, 3]` writes a
   commit message containing a `Deferred:` trailer block listing both
   ACs with their `deferred_ac_reasons:` text (or "(no reason given)"
   when unset). Verified by inspecting the commit message after
   archive in a fixture repo.

4. **AC4 — No-deferral path unchanged.** A PRD with no `deferred_acs`
   field exhibits exactly the current behavior at every gate
   (scan-prds JSON, check #5, archive trailer). Verified by archiving
   a fixture PRD without the field and diffing the resulting commit
   message against a captured baseline.

5. **AC5 — Backfill ships clean.** wintermute-platform/audio/stt/tts
   each gain a `deferred_acs:` field via the install action. The
   four PRDs are then re-checked by `/build`'s gate; each transitions
   to "archive-ready" (verified-completed check #5 passes) without
   any further code changes. Verified by running the gate against
   each PRD's HEAD after backfill.

6. **AC6 — Doctor output distinguishes paired vs deferred.** A
   verbose `/build` run on a backfilled PRD prints per-AC lines
   tagged `PAIRED` vs `DEFERRED` (per §What this builds §6). Verified
   by running the verbose path and grepping the output.

7. **AC7 — Greppable Deferred: trailer.** `git log --grep="^Deferred:"
   --pretty=%B` against the autobuilder repo, run after the four
   stuck PRDs archive, surfaces all four archive commits and lists
   exactly the deferred ACs declared in each. Verified by running
   the grep post-archive.

## Risks and how this PRD addresses them

- **"Deferred" becomes a rubber stamp.** Mitigation: `deferred_ac_
  reasons:` is strongly recommended (doctor output warns when a
  deferred AC has no reason). Future PRD: a `wm-verify` CLI that
  walks the deferred ACs interactively and records attestations
  (Fleet 1.5 future bullet).

- **Backfill misclassifies an AC as deferred when it could be
  tested.** Mitigation: backfill happens in a single commit per PRD,
  citing the iter-log line that justifies the deferral. A future
  /code-review pass can re-test those ACs and remove them from the
  deferred list with a follow-on commit.

- **scan-prds.sh becomes harder to maintain.** Mitigation: the two
  new fields use the same yq pipeline already in place; the diff is
  ~5 lines.

## Implementation outline (`/build` will do this)

1. **scan-prds.sh** — extend yq pipeline to extract `deferred_acs`
   and `deferred_ac_reasons`. Default to `[]` and `{}` when absent.
2. **verified-completed gate script** — add per-AC bookkeeping. For
   each AC, attempt the existing "passing test references AC N?"
   check; if that fails AND N ∈ `deferred_acs`, mark `paired =
   deferred`. Gate-passes iff every AC is `paired = test` or
   `paired = deferred`.
3. **archive action** — append `Deferred:` trailer block to the
   commit message when `deferred_acs` is non-empty.
4. **doctor / verbose output** — emit `PAIRED` / `DEFERRED` per AC.
5. **backfill commit** — edit `PRD-wintermute-platform.md`,
   `-audio.md`, `-stt.md`, `-tts.md` to add `deferred_acs:` derived
   from each PRD's iter-log. One commit per PRD (so each is
   greppable).
6. **commit identity** — `git -c user.email=jyen.tech@gmail.com
   -c user.name="Joe Yen"`. Use `wm-push --slug autobuilder` for
   the push.

Estimated size: ~150 LOC across the two scripts + ~5 lines per
backfilled PRD. Single autobuilder cycle.

## References

- Manifest state: `~/.claude/skills/build/state/manifest.json` —
  fields `prds.{wintermute-platform,wintermute-audio,wintermute-stt,
  wintermute-tts}.iter_log[-1]` and `.next`.
- Sibling self-mod precedents (both shipped 2026-05-27):
  `PRDs-archive/PRD-build-publish-allowlist.md`,
  `PRDs-archive/PRD-build-push-allowlist.md`.
- Vision context: `visions/wintermute.md` § Fleet 1.5 (added by this
  same /dream pass).
- Self-mod pattern: `~/.claude/skills/build/` scripts are edited
  under the Joe Yen identity and committed under `~/.claude/`'s
  worktree, then the manifest reflects the version bump.
