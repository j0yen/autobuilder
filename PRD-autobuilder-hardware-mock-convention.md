# PRD: autobuilder-hardware-mock-convention — satisfy hardware-bound ACs at gate-time via documented mocks

**Status:** Draft v0.1
**build_target:** self-mod
**build_priority:** high
**build_into:** /home/jsy/.claude/skills/autobuilder
**Research:** research/quality-verification-2026-05-28.md §2d, §4 Test 4
**Created:** 2026-05-28
**Author:** Claude (Opus 4.7), for jsy

---

## TL;DR

Replace the blanket `deferred_acs:` escape hatch with a mock
convention: for every hardware-bound AC, the PRD must EITHER (a)
declare a mock test under `tests/mocks/<ac>.rs` that exercises the full
call sequence + signature the real path would, OR (b) declare both
`deferred_acs: [N]` AND `mock_unjustified_for: [N]` with a one-sentence
prose explanation of why a mock isn't tractable.

The verified-completed check #5 accepts either path. ACs that have
neither remain a hard fail. Hardware-conditional reality verification
(via `cargo test --features=real-hardware`) is paired with each mock,
and drift between mock and real device is surfaced in a follow-up
report.

## Why this exists

PRD-build-deferred-acs (shipped 2026-05-27) introduced
`deferred_acs:` so 4 wintermute crates (platform/audio/stt/tts) could
ship past the verified-completed gate. The escape hatch is honest
about the constraint, but it's also the constraint's destination — once
an AC is deferred, no behavioral verification mechanism is wired in at
all. The "I'll verify when the hardware is here" path tends to mean
"I'll never verify."

Mock-then-verify is the discipline used by the rest of the Rust
ecosystem for hardware-adjacent code (linux kernel drivers, embedded
crates, every IO library that has a fake-fs feature). The mock proves
the call sequence + signature + invariant *at the type level*. The
hardware run proves they hold *in the world*. Both, not either.

Research report §2d + §4 Test 4 traces evidence. Also relevant:
`feedback_use_local_toolkit.md` and `feedback_dont_mock_the_database.md`
(spirit, not strict letter — these warn against mocks REPLACING reality,
not against mocks COMPLEMENTING it).

## What this builds

### Artifact 1: PRD frontmatter spec

Augment `~/.claude/skills/autobuilder/SKILL.md` "PRD frontmatter the
skill reads" section. The autobuilder + /build parsers gain:

```
mock_unjustified_for: [3, 7]
    # ACs from `deferred_acs:` that ALSO can't sensibly be mocked.
    # Requires `mock_justifications:` companion field with one
    # sentence per listed AC explaining why.

mock_justifications:
  3: "AC3 verifies hardware fan speed via thermal pressure; no mock
      can simulate the real PWM signal without recreating the firmware."
  7: "AC7 requires the OS scheduler under load; a mock would either be
      a tautology or a different scheduler."
```

The intent: deferring is fine, but deferring without explanation isn't.

### Artifact 2: mock test directory convention

For every AC in `deferred_acs:` but NOT in `mock_unjustified_for:`,
the crate must ship `tests/mocks/ac<N>.rs` containing a test that:
- exercises the same public API surface the real test would,
- against a documented in-crate fake (trait impl, channel pair,
  in-memory device, etc.),
- asserts the same invariant the AC's English text declares.

Mock tests run under `cargo test` by default. They count toward Stage 3
hard gates and the verified-completed checklist.

### Artifact 3: verified-completed gate update

`~/.claude/skills/build/SKILL.md` Phase 4 verified-completed check #5
gains an "OR mock-paired" clause:

> 5. Every acceptance test the PRD declared is paired with EITHER:
>    - a passing `cargo test` name (real test), OR
>    - a passing `cargo test --test mocks::ac<N>` (mock test) AND the
>      AC is listed in PRD frontmatter `deferred_acs:`, OR
>    - the AC is in BOTH `deferred_acs:` AND `mock_unjustified_for:`
>      with a companion `mock_justifications:` entry.
>
> Any AC with none of the three remains a hard fail.

### Artifact 4: hardware-drift report (optional, follow-on)

When the user runs `cargo test --features=real-hardware` against a
crate with mocks, autobuilder's later receipt sweep checks whether
the real-hardware test outcomes match the mock test outcomes. Drift
(mock passes, real fails) emits a `hardware-drift.json` receipt that
the next /self-review surfaces.

This artifact is opt-in for v0.1: the feature flag is documented;
the sweep is a follow-on PRD.

### Artifact 5: backfill the 4-5 hardware-deferred wintermute crates

Within this PRD's iter-N action, for each wintermute crate currently
using `deferred_acs:` without a mock:
- wintermute-platform (systemctl ACs)
- wintermute-audio (live audio device ACs)
- wintermute-stt (whisper-cpp inference ACs)
- wintermute-tts (piper inference ACs)
- wintermute-dialog (barge-in timing ACs)

…either write the missing `tests/mocks/ac<N>.rs` OR add the
`mock_unjustified_for:` + `mock_justifications:` frontmatter. Each
crate is one self-mod tick on /build's queue; the autobuilder run for
this PRD does the parser update + SKILL.md edits + writes a follow-on
PRD per crate for the backfill if it doesn't fit in one tick.

### Out of scope (v0.1)

- Auto-generating mocks from a trait. Hand-written; the mock IS the
  documentation of what the boundary looks like.
- A linter that refuses `deferred_acs:` without companion fields.
  v0.1 catches it at the verified-completed check; a Phase-2 lint
  would catch it at iter-1.
- Cross-crate mock libraries. Each crate's mocks are local.

## Acceptance criteria

- **AC1**: PRD parser (`scripts/scan-prds.sh` for /build,
  autobuilder's intake parser) reads `mock_unjustified_for: [N, M]`
  and `mock_justifications: {N: "...", M: "..."}` from PRD frontmatter
  and exposes them in the manifest JSON.
- **AC2**: PRD with `deferred_acs: [3]` AND NOT in
  `mock_unjustified_for:` AND NOT having `tests/mocks/ac3.rs` fails
  the verified-completed check #5 with a clear diagnostic naming the
  missing mock file.
- **AC3**: PRD with `deferred_acs: [3]` AND
  `mock_unjustified_for: [3]` AND `mock_justifications: {3: "..."}`
  passes verified-completed check #5 (path 3 of the OR-clause).
- **AC4**: PRD with `deferred_acs: [3]` AND a passing
  `cargo test --test mocks::ac3` passes verified-completed check #5
  (path 2 of the OR-clause).
- **AC5**: PRD with `deferred_acs: [3]` AND `mock_unjustified_for: [3]`
  but no companion `mock_justifications: {3: ...}` fails parser with
  a diagnostic; verified-completed check fails by extension.
- **AC6**: Backfill: each of the 5 wintermute crates listed above
  has, after this PRD ships, EITHER `tests/mocks/ac<N>.rs` for each
  deferred AC OR a `mock_unjustified_for:` + justification entry.
  This may take multiple /build ticks; tracked as follow-on PRDs.
- **AC7**: `~/.claude/skills/autobuilder/SKILL.md` PRD frontmatter
  section gains the two new fields; example block updated.
- **AC8**: `~/.claude/skills/build/SKILL.md` Phase 4
  verified-completed checklist #5 gains the OR-clause.
- **AC9**: Existing PRDs (already-shipped) are NOT retroactively
  failed by this PRD — the OR-clause's third path (mock_unjustified +
  justification) is back-compat. Verified by running
  verified-completed against `PRDs-archive/PRD-wintermute-platform.md`
  post-frontmatter-backfill and confirming it still passes.
- **AC10**: Drift-report scaffolding exists but is not invoked
  by default. `cargo test --features=real-hardware` is documented in
  the relevant crate READMEs; the sweep itself is a future PRD.

## Files

```
~/.claude/skills/autobuilder/
└── SKILL.md                          # +frontmatter fields

~/.claude/skills/build/
├── SKILL.md                           # +verified-completed OR-clause
└── scripts/scan-prds.sh               # +mock_unjustified_for parsing

~/wintermute/wintermute-platform/      # backfill (follow-on PRDs)
~/wintermute/wintermute-audio/
~/wintermute/wintermute-stt/
~/wintermute/wintermute-tts/
~/wintermute/wintermute-dialog/
```

## Non-functional

- The mock convention is OPT-IN per AC. Crates whose ACs are all
  hardware-mockable still pass with mock-paired tests; crates with
  truly unmockable ACs use the justification path. No forced
  conversion.
- Mock test files are subject to the same lint discipline as real
  test files (unwrap/expect/panic = deny). The mock is real Rust, not
  a magic affordance.
