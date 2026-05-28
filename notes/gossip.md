# dream ↔ build gossip

Append-only shared notebook between `/dream` and `/build`.

Format: one section per entry, fenced with `## <ISO-ts>  /<skill>  <subject>`.
Body is free-form markdown — order hints, blockers, questions, observations,
links between PRDs. Never rewrite earlier entries; correct by appending a
new entry that references the older one.

Read the tail before acting. Append after acting.

---

## 2026-05-24T18:50  /dream  bootstrap
Channel opened. `/dream` skill created at `~/.claude/skills/dream/`. First
real entries will follow when the user invokes `/dream <topic>`.

---

## 2026-05-24T19:30  /dream  vision-wintermute
Drafted: PRD-wintermute-bootstrap.md, PRD-wintermute-platform.md,
  PRD-wintermute-audio.md, PRD-wintermute-stt.md, PRD-wintermute-tts.md,
  PRD-wintermute-dialog.md, PRD-wintermute-brain.md (7 PRDs, Fleet 1).
Vision: visions/wintermute.md (voice-first AI laptop for a
  computer-illiterate user; prototyping on this laptop under the
  wintermute name; she will name her own laptop later).

**User override — ATTENTION /build:** all 7 PRDs carry `build_auto: true`.
  Default `/dream` rule is `build_auto: false`; user explicitly
  authorized aggressive pace this session. Treat as opt-in even though
  the flag is set — the user wants you to start ticking immediately.

**Sequencing for /build:**
  - PRD-wintermute-bootstrap and PRD-wintermute-platform are entry
    gates (no upstream deps). Either can start first.
  - PRD-wintermute-audio gates wm-stt, wm-dialog, and the barge-in
    cancel path for wm-tts.
  - PRD-wintermute-tts can land in parallel with audio (only needs
    a sink); useful early so platform's greeting works on first boot.
  - PRD-wintermute-dialog and PRD-wintermute-brain develop in parallel;
    brain has a real dependency on PRD-recall-daemon.md shipping
    (recall daemon mode for sub-10ms PostToolUse retrieval).
  - Recommended order: bootstrap → platform → audio + tts (parallel) →
    stt → dialog → brain.

**Cross-PRD collaboration to coordinate:**
  - PRD-wintermute-tts and `peon-ping/docs/prds/PRD-003-tts-spoken-feedback.md`
    both want a Linux TTS engine. Contract documented in wm-tts §2.5:
    whoever ships first defines the `wm-voicepack` resolver crate; the
    other adopts it. Don't double-build the Piper backend.
  - PRD-wintermute-brain depends on PRD-recall-daemon.md (sub-10ms
    retrieval path). If recall-daemon isn't live when brain implementation
    starts, brain falls back to in-process recall (loud warning; 500ms
    instead of 10ms — functional but degraded).

**Plan-agent's flagged risks** (worth surfacing during /build iterations):
  - **AEC3 build flag on Arch's `pipewire` package** — `wm-audio` detects
    at startup and falls back to webrtc classic; PRD documents the
    rebuild path if AEC3 is missing.
  - **microWakeWord pretrained-only in v1** — custom wake-word training
    is too finicky for a non-literate user's setup. Wake word is
    selected from a pretrained set ("Hey Jarvis" / "Okay Nabu" /
    "Hey Mycroft") during bootstrap. Do NOT promise "Hey Wintermute"
    custom training; that's Fleet 3 if it ever happens.
  - **Sonnet vs Opus default for brain** — Sonnet 4.6 by default; Opus
    4.7 opt-in via `wmd --model opus` for the next turn only. Opus on
    every chatty turn would burn cost and latency.

**Open question for /build:**
  - PRD-wintermute-platform §2.3 leaves the supervisor choice open:
    reuse `pevent` (Option A, recommended) or hand-roll standalone
    (Option B). Pick during platform's iter-1; document the call in
    the platform repo's README. Leaning A — `pevent` is already
    battle-tested on this laptop and the dep is local.

**Library picks summary** (full table in visions/wintermute.md):
  microWakeWord (wake) · Silero VAD · whisper.cpp + whisper-rs (STT,
  distil-small.en default) · Piper (TTS) · PipeWire module-echo-cancel
  (AEC) · NoiseTorch-ng (NS) · Claude API Sonnet 4.6 (brain).
  Reference architecture: Home Assistant Voice PE pipeline at
  300-700ms end-to-end on a Pi5 — proves the latency budget.

**Fleet 2 and Fleet 3** are captured as bullets in visions/wintermute.md
  but NOT drafted as PRD files in this pass. User will invoke
  `/dream extend wintermute` after Fleet 1 has shipped enough to
  learn from (≥3 of 7 components). Examples in vision doc:
  wintermute-browser, wintermute-desktop, wintermute-mail,
  wintermute-music, wintermute-screen-narrate, wintermute-emergency,
  wintermute-voice-profile, wintermute-glow (state indicator —
  dropped from Fleet 1 because non-blocking for first-usable).

**Notes for next /dream tick:**
  - When /build ships #1 bootstrap and #2 platform, /dream should
    consider drafting wintermute-glow (visual state indicator) so the
    caregiver / her sighted helpers have an at-a-glance view of what
    the laptop is doing.
  - Worth a /dream pass on `wintermute-offline-persona` once the brain
    is up — the offline behavior in v1 is a polite apology; richer
    offline (cached news, music, time-telling, small local LLM chat)
    is a vision in its own right.

---

## 2026-05-25T04:30  /dream  vision-continuity
Drafted: PRD-agentns-claude.md, PRD-provq.md, PRD-memlog-witness.md,
  PRD-recall-session-stamp.md, PRD-session-postmortem.md (5 PRDs,
  Fleet 1). Vision: visions/continuity.md (kernel→userspace bridge —
  consume memlog + provfs LSM + agent namespaces from userspace so
  every Claude session has a primary-source 128-bit id and downstream
  tools can attribute work to it).

**Default rule applied — ATTENTION /build:** all 5 PRDs carry
  `build_auto: false`. Unlike the wintermute fleet, this one is opt-in
  per PRD — the user reviews each before /build advances it. Don't tick
  these until the user flips the flag or explicitly authorizes.

**Sequencing for /build (once authorized):**
  - PRD-agentns-claude is the entry gate (every other PRD benefits
    from a stable session_id at session start, and #4 + #5 require it
    in non-mock mode).
  - PRD-provq and PRD-memlog-witness are independent of #1 and of each
    other — develop in parallel. Both can scaffold pre-boot; their live
    ACs gate on `linux-wintermute` being booted.
  - PRD-recall-session-stamp depends on #1's session_id resolution
    contract. Target version: recall v0.6.0 (intentional gap above the
    in-flight v0.5.x band held by recall-daemon and rebased
    recall-outcome-feedback).
  - PRD-session-postmortem depends on all four. Can scaffold against
    fixtures pre-#1–#4 ship; AC9 gates on all four shipping.

**Boot gating (important):**
  - The wintermute kernel package is BUILT (per 2026-05-24 changelog)
    but AWAITS BOOT VALIDATION. Until the user boots into
    `linux-wintermute`, all `[boot]`-marked ACs cannot pass. Mock
    interface contracts are documented in each PRD (AGENTNS_SESSION_ID
    env override, `/tmp/agentns-mock` file, fixture replay for
    memlog-witness).
  - /build should NOT mark these PRDs verified-completed before boot
    validation lands, even if mechanical ACs pass. Annotate manifest
    entries with `boot_validation_pending: true` until then.

**Cross-PRD coordination with in-flight work:**
  - recall-session-stamp targets v0.6.0 to avoid colliding with
    recall-daemon's v0.5.0 and recall-outcome-feedback's v0.5.1–0.5.3
    rebased band. v0.6.0 is past both; safe.
  - memlog-witness is a `rust-extend` to ~/wintermute/memlog/ — adds a
    new binary and a `persistence.rs` module. The existing kernel
    module, `libmemlog`, and `cli/memlog` (python show tool) are
    untouched.
  - session-postmortem composes by shelling out — does not re-implement
    ctrace parsing, recall queries, or xattr reads. Reduces blast
    radius if any of #1–#4 change their CLI surface; the postmortem
    just sees stdout.

**Notes for next /dream tick:**
  - Fleet 2 (vision continuity §"Fleet 2 — Hook into introspection")
    is bullets only: mirror-kernel, episode-from-memlog,
    letter-from-snapshot, /postmortem skill, agentns-budget-policy.
    Draft after ≥3 of 5 Fleet 1 PRDs ship.
  - Still pending from earlier /dream notes: wintermute-glow once
    bootstrap+platform ship; wintermute-offline-persona once brain is
    up. Not yet ready.

**Open question for /build:**
  - When /build encounters a PRD with `build_auto: false`, behavior is
    "skip until explicit user opt-in." Confirm that's still how the
    scan-prds path treats it (vs. queued-but-not-ticking). The
    continuity fleet's whole shape depends on the user reviewing each
    PRD before /build advances — please don't auto-flip the flag.

---

## 2026-05-24T22:30  /dream  vision-cadence
Drafted: PRD-cadence-substrate.md, PRD-cadence-bind-daily-receipt.md,
  PRD-cadence-bind-confidant.md, PRD-cadence-bind-letters.md,
  PRD-cadence-bind-zine.md, PRD-cadence-bind-reliquary.md,
  PRD-cadence-pulse.md (7 PRDs, Fleet 1).
Vision: visions/cadence.md (composes the existing reflective time-pyramid
  — daily-receipt/confidant/letters-we-never-sent/conversations-zine/
  memory-reliquary — by adding one shared substrate at
  ~/.claude/cadence/ + one CLI + six thin bind-extensions so each tier
  records what it produces and reads what the tier below produced).

**Default rule applied — ATTENTION /build:** all 7 PRDs carry
  `build_auto: false`. User reviews each before /build advances.

**Sequencing for /build (once authorized):**
  - PRD-cadence-substrate MUST ship first (foundational; nothing else
    can use the substrate's CLI until it exists). It's a new repo at
    ~/wintermute/cadence/, rust-cli, not rust-extend.
  - The five bind PRDs (daily-receipt → confidant → letters → zine →
    reliquary) are mutually independent and can ship in any order
    or in parallel. They tolerate a half-bound pyramid — each bind's
    intake gracefully falls back to existing behavior when its
    upstream tier hasn't shipped yet.
  - PRD-cadence-pulse depends ONLY on substrate, not on binds. Worth
    shipping right after substrate even before any bind lands; the
    empty-substrate output ("everything overdue: never") is itself
    honest signal.

**Cross-fleet coordination:**
  - No collision with wintermute fleet (different repo set entirely).
  - No collision with continuity fleet (continuity is per-session /
    kernel-boot-gated; cadence is per-day-and-up / userspace).
  - Recall fleet (recall-daemon, recall-outcome-feedback,
    recall-session-stamp, recall-bash-response-richness,
    recall-braid-freshness-tunable, recall-stop-hook-session-id):
    cadence doesn't touch recall directly. memory-reliquary already
    reads recall; cadence-bind-reliquary leaves that intake path
    intact and only adds the quarterly section.
  - build-rust-extend (shipped, v0.4.1+) is the rail every bind PRD
    uses. All six rust-extend PRDs honor the existing extend pattern:
    no re-init, version bump, install via `cargo install --path .`,
    CHANGELOG.md, push to existing remote.

**Notes for /build:**
  - PRD-cadence-substrate is the only new-repo PRD in this fleet
    (rust-cli, build_into not set — uses the standard `gh repo create
    j0yen/cadence` path). The other six are rust-extend.
  - Each bind PRD's `build_into` points to an existing repo; the
    binary names are mixed (`daily-receipt`, `confidant`,
    `letter-curate`, `zine`, `reliquary`) — verify the crate Cargo.toml
    name before bumping (verified during /dream Phase 1).
  - The shell-out approach (binds call `cadence record/list` as
    subprocesses, not as a Rust lib dep) is intentional. Keeps the
    binds decoupled and lets cadence evolve without churning five
    Cargo.toml files.

**Open questions for /build:**
  - Is the empty-substrate pulse output ("overdue: never") good UX or
    too noisy? The PRD bets on honest > polite. Confirm during pulse
    review.
  - Should the substrate auto-register tools the first time `cadence
    record` is called by a previously-unknown tool? PRD says no
    (explicit `register` only). User may want the convenience; leave
    for v0.3 once feedback exists.

**Fleet 2 bullets (not drafted this pass):**
  - cadence thread <topic> — cross-tier topic trace
  - cadence deck — printable wall-calendar PDF
  - cadence share — encrypted publish
  - ambient integration — tonal shift on missed-tier signal
  - cadence prune — substrate cleanup (after idempotency settled)
  - SessionStart hook auto-install (depends on cadence-pulse landing)
  - Possible per-session tier (only if continuity's session-postmortem
    demands it)

**Still pending from earlier /dream notes:**
  - wintermute-glow once bootstrap+platform ship (no change)
  - wintermute-offline-persona once brain is up (no change)
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship (no change)
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship (no change)

---

## 2026-05-25T05:25  /dream  vision-chord
Drafted: PRD-chord-intent-rich.md, PRD-chord-claim.md,
  PRD-chord-async-delegate.md, PRD-chord-cross-episode.md (4 PRDs, Fleet 1).
Vision: visions/chord.md (cross-session orchestration — the four-PRD
  thin coordination layer atop agorabus that turns 3 concurrent sessions
  from noise into a chord. Three white-space gaps identified during
  Phase 1 research: synchronous head-of-line-blocking delegate.run,
  empty `intent` field on heartbeats, no soft-lock primitive, and
  episodic-observer being single-session by design).

**Default rule applied — ATTENTION /build:** all 4 PRDs carry
  `build_auto: false`. User reviews each before /build advances.

**Sequencing for /build (once authorized):**
  - PRD-chord-intent-rich is foundational; ship first. Adds `skill`,
    `prd_slug`, `working_paths[]` to heartbeat envelope; minor agorabus
    version bump (0.1 → 0.2).
  - PRD-chord-claim depends on intent's `working_paths` schema but
    only loosely; can ship in either order with intent. Both are
    rust-extend on the same repo (~/wintermute/agorabus); bundle
    versioning if shipped close together.
  - PRD-chord-async-delegate is build_target:shell (extends
    ~/.claude/scripts/agorabus-worker.sh + adds a new
    agorabus-delegate-runner.sh helper). Independent of the agorabus
    rust-extend pair. Plus a follow-up commit to AGORABUS_RPC.md (the
    doc, not code) updating it from v0.1 → v0.2.
  - PRD-chord-cross-episode is rust-extend on episodic-observer.
    Soft-depends on chord-intent-rich for high-fidelity
    `working_paths` filtering, and on chord-claim for AC5
    suppression. Degrades gracefully if either upstream isn't live —
    explicitly named in the Risks section.

**Cross-fleet coordination:**
  - No collision with wintermute fleet (different repo set; voice
    laptop is orthogonal to developer-tooling layer).
  - No collision with continuity fleet — they compose: an agentns
    session id (continuity Fleet 1) is exactly the kind of stable
    handle chord-intent-rich wants to broadcast. Once the kernel
    boots and agentns is live, the two visions reinforce each other.
  - No collision with cadence — cadence is per-day-and-up reflection;
    chord is per-second-and-up coordination. A cross-session episode
    feeding into cadence's daily-receipt is a Fleet-2-or-later
    composition idea, not a coupling.
  - Recall fleet: chord-cross-episode reads agorabus events alongside
    transcripts; recall isn't touched directly. Once recall-daemon
    (in flight, iter-2) ships and provides a sub-10ms query path,
    chord-cross-episode could optionally query recall to enrich
    candidates with prior memories. Not in this fleet.

**Important corrections discovered during Phase 1:**
  - `AGORABUS_RPC.md` v0.1 changelog (2026-05-23) says "no handler
    implementations shipped" — this is **stale**. The shipped worker
    at `~/.claude/scripts/agorabus-worker.sh` already implements
    ping/self.describe/methods.list/delegate.run. The chord PRDs
    update the convention doc to v0.2 alongside the async-delegate
    code changes.
  - Feedback memory `feedback_delegate_run_300s_cap.md` says
    "hardcodes timeout 300s" — partially stale: the timeout is
    per-call overridable via `params.timeout_secs`, but the
    *head-of-line blocking* is the real underlying problem. The
    chord-async-delegate PRD addresses both (configurable ttl + non-
    blocking ticket pattern).

**Notes for /build:**
  - PRD-chord-async-delegate is the only `build_target: shell` PRD in
    this fleet (no Cargo.toml, no /autobuilder cycle). It edits two
    files in ~/.claude/scripts/ and adds one new helper script.
    /build's shell path should handle this; if it doesn't, defer
    until /dream extends /build for it.
  - Both rust-extend agorabus PRDs target a v0.2 minor bump.
    Coordinate so they don't both try to claim v0.2.0 simultaneously
    — recall-daemon vs recall-outcome-feedback had exactly this
    collision (resolved by rebasing one to v0.5.1+).
    Recommendation: intent-rich is v0.2.0, claim is v0.2.1 (patch
    bump on second). Or, if both ship in one PR, intent-rich is
    v0.2.0 and claim folds in as part of the same minor.
  - episodic-observer is currently v0.1.0; cross-episode bumps to
    v0.2.0.

**Open questions for /build:**
  - chord-async-delegate's worker-restart story for in-flight tickets:
    runner uses `setsid` so survives, but ticket state file may be
    stale-marked on next worker start. The AC names "scan tickets dir
    on boot, mark stale-running as failed:'worker_restart'" — confirm
    that lands in /build's first iter, not deferred.
  - Naming: `chord` was chosen during /dream because it's short,
    evocative (concurrent notes coordinating into one sound), and not
    taken in ~/wintermute/. If user prefers another name (consort,
    council, concert, concord), rename the vision file + all four
    PRDs together. Captured in visions/chord.md §Open questions.

**Fleet 2 bullets (not drafted this pass, captured in visions/chord.md):**
  - chord-peek — `agorabus peek <peer-sid>` wrapping an RPC
    `self.recent_tools` method
  - chord-peer-review — PostToolUse hook publishes
    `code.commit.<repo>`; peers can offer review
  - chord-method-discovery — `methods.list` returns capability
    table + custom registration
  - chord-quorum — multi-session agreement for risky shared actions
  - chord-handoff — exiting session hands off intent + claims to
    a peer

Draft Fleet 2 after ≥2 of 4 Fleet 1 PRDs ship.

**Still pending from earlier /dream notes:**
  - wintermute-glow once bootstrap+platform ship (no change)
  - wintermute-offline-persona once brain is up (no change)
  - continuity Fleet 2 after ≥3 of 5 Fleet 1 PRDs ship (no change)
  - wintermute Fleet 2 after ≥3 of 7 Fleet 1 PRDs ship (no change)
  - cadence Fleet 2 after ≥1 of 7 Fleet 1 PRDs ship (no change)

## 2026-05-25T06:30  /dream  vision-freshness
Drafted: PRD-recall-doctor-claims.md (1 PRD, Fleet 1).
Vision: visions/freshness.md (single-PRD vision; Fleet 2 bullets
  captured but explicitly NOT drafted per dream rule 6 — "don't
  dream past the research").

**Why this pass was narrow.** Bare /dream invocation arriving ~50min
after vision-chord drop (5th /dream in <30h, 4 active visions, 28
PRDs queued, only 1 shipped today). Saturation is real. The honest
move was either (a) ask the user, or (b) pick the smallest genuinely
new gap and stop. User declined the AskUserQuestion offer; I went
with (b).

**The gap:** during chord-vision Phase 1 grounded research I caught
two stale claims in one hour — `feedback_delegate_run_300s_cap`
asserts "hardcodes" but the timeout is per-call overridable;
`AGORABUS_RPC.md` v0.1 changelog says "no handlers shipped" but
ping/self.describe/methods.list/delegate.run all ship today. These
were caught only because Phase 1 happened to read both the
memory/doc AND the live source in the same hour. A deliberate
sweep would catch more. That's the entire motivation. One PRD,
one tool, parks proposals for the user to review.

**Default rule applied — ATTENTION /build:** PRD-recall-doctor-claims
carries `build_auto: false`. User reviews before /build advances.

**Sequencing for /build (once authorized):**
  - Single PRD; no internal ordering. Independent of every other
    in-flight recall PRD because v0.7.0 reserves clean space
    (v0.5.0 recall-daemon, v0.5.1-0.5.3 recall-outcome-feedback,
    v0.6.0 recall-session-stamp). If another recall PRD jumps the
    line, rebase to next free minor — explicitly named in PRD §Notes.
  - rust-extend on ~/wintermute/recall. Same mechanical path as
    recall-observer-correlation v0.4.2 (shipped today).
  - One new module src/doctor_claims.rs + clap flag on existing
    `doctor` subcommand. No new top-level command.

**Cross-fleet coordination:**
  - Composes with continuity Fleet 2 (freshness-doc-sweep in F2
    would consume provfs xattrs to know which session wrote which
    doc; useful once kernel boots).
  - Composes with chord-cross-episode (freshness-cross-session-witness
    in F2 would surface when two sessions disagree about a fact).
  - Composes with cadence (a quarterly "freshness audit" tier
    entry would feed memory-reliquary).
  - Composes with itself: this very PRD will go stale. When
    `recall doctor --check-claims` first runs against the
    eventually-promoted version of this PRD, it'll be a good early
    self-test.

**Notes for /build:**
  - AC5 sets a 30% false-positive ceiling on the extractor. If
    iter-1 trips this, tighten extraction patterns (drop
    prose-path matching; only check fenced-code paths) before
    iter-2; don't ship a noisy tool.
  - AC10 requires a real round-trip — jsy reviews at least one
    actual proposal generated against the live store. Don't
    mark shipped on synthetic proofs alone. The proposal review
    flow already exists (recall observe / proposals / promote);
    this just adds a new source tag (`source: doctor-claims`).

**Fleet 2 bullets (NOT drafted, captured in visions/freshness.md):**
  - freshness-claims-rich (extend extractor for hardcoded/always/never
    qualifiers)
  - freshness-doc-sweep (apply checker to READMEs + CHANGELOGs +
    CLAUDE_SELF.md)
  - freshness-self-changelog (CLAUDE_SELF.md changelog specifically)
  - freshness-on-recall (lazy re-check on `recall query` hit)
  - freshness-cross-session-witness (compose with chord-cross-episode)

Draft Fleet 2 after Fleet 1 ships AND the proposal queue produces
at least one user-promoted supersede.

**Still pending from earlier /dream notes:**
  - wintermute-glow once bootstrap+platform ship (no change)
  - wintermute-offline-persona once brain is up (no change)
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship (no change)
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship (no change)
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship (no change)
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship (no change)

## 2026-05-25T07:50  /dream  no-fleet-pass
Bare /dream invocation, 80min after vision-freshness. **No new fleet
drafted this pass** — per dream rule 6 ("don't dream past the
research"). Three reasons:

  1. Saturation: 5 active visions, 28 PRDs queued, 1 shipped today.
     Bottleneck is /build throughput, not /dream supply.
  2. No new evidence since 06:30 (no PRD shipped, kernel still on
     7.0.9 stock, no journal entry for today).
  3. User declined the AskUserQuestion offering 4 directions
     (specific topic / extend existing / status only / dream anyway).
     Honest move was option (b) from the freshness pass — go narrow
     or don't go — and this time "don't go" is the truth.

**One micro-finding this pass DID surface,** worth capturing here so
it isn't lost: the 06:30 freshness-gossip note cites `ctrace ls` as
a Phase 1 command. `ctrace ls` is not a valid subcommand — actual
subcommands are `start|stop|status|query|tail`. This is exactly the
class of staleness that `PRD-recall-doctor-claims` was drafted to
catch (extended in Fleet 2's freshness-doc-sweep to cover gossip /
skill docs). Logged as evidence in `visions/freshness.md` §Evidence
log; strengthens the Fleet 2 case once Fleet 1 ships.

**Notes for /build:** nothing new. Continue with current in_progress
PRDs (build-rust-extend AC-10 verification, recall-daemon iter-3,
wintermute-bootstrap/platform/tts in-flight). No new dependencies
introduced by this /dream pass.

**Notes for next /dream:** real new evidence the laptop could
produce that would unblock a grounded vision:
  - kernel boots into linux-wintermute → continuity Fleet 1 boot-
    gated ACs become testable → grounded basis for continuity
    Fleet 2.
  - any single in_progress PRD ships → gossip-trigger to draft its
    Fleet-2 follow-on per the existing rules in earlier gossip
    notes (chord ≥2, continuity ≥3, wintermute ≥3, cadence ≥1).
  - a real new user-articulated topic.
  Without one of those, the next bare /dream should also be a
  no-fleet-pass. Two in a row is not a problem — it's the system
  working correctly.

**Pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede

## 2026-05-25T07:50  /dream  vision-handshake
Drafted: PRD-agorabus-boot-handshake.md (1 PRD, Fleet 1).
Vision: visions/handshake.md (single-PRD vision; Fleet 2 bullets
  captured but explicitly NOT drafted per dream rule 6).

**Why this pass wasn't a no-fleet-pass.** Last note (~07:50) said
the next bare /dream should also be no-fleet unless real new
evidence arrived. Three new pieces of evidence DID arrive in the
intervening hours:
  1. **recall v0.4.3 shipped** (`2df7156 recall v0.4.3: braid
     freshness tunable, default 60s → 300s`) — 2nd ship today
     after v0.4.2 observer-correlation. /build throughput is
     better than yesterday.
  2. **2026-05-25 self-review entry exists** at
     `~/brain/journal/2026-05-25.md` (was empty at 07:50 pass).
     §Notable identifies a new bug class: post-reboot startup
     race in `agorabus-session-start.sh` produces orphan
     subscribers when daemon-not-ready races the hook's 0.5s
     socket-wait under boot load. PID 917 hit it today.
     Journal explicitly proposes the fix.
  3. **Kernel build PID 12146 still running** at load 10.42 —
     same conditions that produced today's race. Bug is
     reproducible and current.

This satisfies the "narrow gap with grounded new evidence"
heuristic from the 06:30 freshness pass. One small shell-target
PRD, single-PRD vision (same pattern as `freshness`).

**Default rule applied — ATTENTION /build:**
PRD-agorabus-boot-handshake carries `build_auto: false`. User
reviews before /build advances.

**Sequencing for /build (once authorized):**
  - Single PRD, no internal ordering.
  - **build_target: shell.** /build's shell-target path may not
    yet be hardened (chord-async-delegate is the only other
    in-flight shell PRD; it hasn't shipped). If /build can't run
    shell yet, defer this PRD until /dream extends /build for
    shell. The handshake bug is real but not blocking; current
    self-review playbook (escalate to user) is workable in the
    meantime.
  - One file edited: `~/.claude/scripts/agorabus-session-start.sh`.
    No new files. No version bump (script has no version).
  - AC8 and AC10 require manual verification (synthetic load /
    reboot); /build should ship to AC7 + AC9 mechanically and
    mark AC8/AC10 as user-verify checkpoints, same pattern as
    recall-observer-correlation.

**Cross-fleet coordination:**
  - **chord-async-delegate** is the only other shell-target PRD
    in flight; both touch `~/.claude/scripts/`. No file collision
    (chord-async-delegate adds new files; this PRD edits one
    existing file). Sequence so the handshake fix lands first —
    chord-async-delegate assumes a reliable bus.
  - **continuity Fleet 1** — once agentns boots, subscribe will
    use 128-bit agentns session ids instead of PID-derived ones.
    Handshake verification logic is sid-agnostic, so no rework.
  - **freshness / cadence / wintermute** — none.

**Notes for /build:**
  - The retry parameters (10 × 0.3s socket-wait; 10 × 0.3s peer-
    record poll with one re-spawn after 10 attempts) are tuned
    for today's boot conditions. Surface any tuning learnings
    back to PRD's §Open questions, not as a code comment.
  - Handshake log directory `~/.cache/agorabus/handshake/` does
    not exist yet; first run creates it. AC7 (log rotation, 14
    days) is the only place the script needs `find` — keep that
    invocation explicit and bounded (no `-exec rm -rf`).
  - Do not auto-merge / install to live `~/.claude/scripts/`
    without jsy testing one session first. This file is on the
    Claude startup path.

**Fleet 2 bullets (NOT drafted, captured in visions/handshake.md):**
  - handshake-reannounce-on-watch-loss (daemon-side `peer.lost`
    broadcast)
  - handshake-daemon-ready-fd (marker file instead of socket
    poll)
  - handshake-reattach-cli (`agorabus reattach <sid>` for
    orphan recovery)
  - handshake-startup-race-pevent (supervised daemon launch)
  - handshake-prom-counters (race/orphan/recover counters)

Draft Fleet 2 after Fleet 1 ships AND at least one race is
observed + recovered in `~/.cache/agorabus/handshake/` logs.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede

## 2026-05-25T08:30  /dream  no-fleet-pass
Bare `/dream`, 40min after vision-handshake. **No new fleet drafted
this pass** — second consecutive no-fleet-pass, exactly the cadence
the 07:50 no-fleet note predicted ("two in a row is not a problem").

**State delta since vision-handshake (07:50):**
  1. **linux-wintermute kernel pkgs are now sitting on disk**:
     `~/wintermute/wintermute-kernel/pkg/linux-wintermute-7.0.10.arch1-1-x86_64.pkg.tar.zst`
     (154MB) + headers (43MB), both timestamped May 25 00:54.
     The build finished ~7h *before* the 07:50 handshake pass
     cited "PID 12146 still running" as live evidence. PID 12146
     is gone now. The handshake PRD's *core* premise (the orphan-
     for-PID-917 race is real and current; see journal §Notable)
     remains valid; only its third supporting fact was stale.
  2. **recall-daemon iter-3 committed** at 08:26 (4min ago,
     e51b9a2): query/embed/touch wired against open Index +
     built Embedder, lib 31/31 + integration 4/4 green. Still
     pre-ship; v0.5.0 lands after iter-4 (CLI auto-forward +
     systemd-user unit).
  3. **stock 7.0.9 still booted.** linux-wintermute is install-
     ready but not installed. User decision: `sudo pacman -U`
     the two pkg.tar.zst files (alongside the 29-pkg queue still
     blocked on protected substrings) and reboot to unlock
     memlog / provfs / agentns.

**Why no fleet was drafted:**
  - Triggers from the 07:50 no-fleet rules:
      * any in-flight PRD ships → recall-daemon hasn't (iter-3 of 4)
      * kernel boots → not yet (build done, not booted)
      * new user articulation → none
  - Saturation unchanged: 5 active visions, 29 PRDs queued, 0
    additional shipped since handshake. /build remains the
    bottleneck, not /dream supply.

**One micro-finding logged this pass** (added to
`visions/freshness.md` §Evidence log): the 07:50 handshake gossip
note cited a fact that was already 7h stale at the time of writing.
This is exactly the freshness-on-gossip class. Surfaces a new Fleet 2
candidate beyond the original five: `freshness-on-dream` — spot-check
load-bearing claims in fresh gossip drafts before they commit. Not
drafted; bullet captured in vision file.

**Notes for /build:** unchanged. Continue with current in_progress
PRDs (recall-daemon iter-4 is the visible next ship; wintermute-
bootstrap/platform/tts still in-flight; build-rust-extend AC-10 done).
No new dependencies introduced.

**Notes for next /dream:** evidence that would unblock a real fleet:
  - **recall-daemon ships to v0.5.0** → recall fleet can extend with
    daemon-aware follow-ons (already partly covered by recall-outcome-
    feedback's rebase to v0.5.1-v0.5.3, but check before drafting).
  - **kernel boots into linux-wintermute** → continuity Fleet 1
    boot-gated ACs become testable → grounded basis for continuity
    Fleet 2. This is the single biggest unblock available; one
    user action (`pacman -U` + reboot) shifts the entire substrate.
  - **any other single in-flight PRD ships** → its Fleet-2 trigger
    fires (chord ≥2, continuity ≥3, wintermute ≥3, cadence ≥1).
  - **a real new user-articulated topic.**

If none of those land before the next bare /dream, a third
no-fleet-pass is the right call. The discipline holds.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede

## 2026-05-25T09:15  /dream  no-fleet-pass
Bare `/dream`, 45min after the 08:30 no-fleet-pass. **Third
consecutive no-fleet-pass** — exactly the cadence the 08:30 note
predicted ("a third no-fleet-pass is the right call. The discipline
holds.").

**State delta since 08:30:**
  1. **recall v0.4.3 shipped twice**: commit 2df7156 (braid
     freshness tunable, default 60s → 300s) + commit fdc81ad
     (CHANGELOG backfill). This is **PRD-recall-braid-freshness-
     tunable.md** landing in code. Cargo.toml now reports 0.4.3;
     CHANGELOG.md has the v0.4.3 section.
  2. recall-daemon: still iter-3 (no iter-4, no v0.5.0). No new
     commits past e51b9a2.
  3. kernel: still stock 7.0.9 booted. `~/wintermute/wintermute-
     kernel/pkg/linux-wintermute-7.0.10.arch1-1-x86_64.pkg.tar.zst`
     unchanged from 00:54.
  4. No new user articulation (bare /dream).

**Why no fleet drafted:**
  - The recall-braid-freshness-tunable ship is an **orphan-PRD ship**:
    NOT in any active vision (the freshness vision is about
    `recall doctor --check-claims` for memory bodies; this PRD is
    about the *braid correlator's* fresh-tool-result window — a
    different "freshness"). NOT in the build manifest's `prds`
    object (manifest read confirms only build-rust-extend,
    agentic-memory, recall-daemon, recall-observer-correlation,
    recall-outcome-feedback, plus notebooks/wintermute-audio).
    Therefore the ship fires NONE of the six Fleet 2 triggers
    (chord ≥2, continuity ≥3, wintermute ≥3, cadence ≥1, freshness
    ≥1, handshake ≥1 — all still at 0).
  - Triggers from 08:30's no-fleet rules:
      * recall-daemon to v0.5.0 → no (still iter-3)
      * kernel boots → no
      * any in-flight **Fleet 1** PRD ships → no (orphan ship
        doesn't count)
      * new user articulation → no
  - Saturation unchanged: 6 active visions, ~28 PRDs queued, /build
    remains the bottleneck.

**One micro-finding logged this pass — orphan-PRD desync:**
  Three recall-* PRDs target v0.4.x extension work but live as
  orphan drafts (no vision, no manifest tracking):
    - **PRD-recall-braid-freshness-tunable.md** — Status line still
      reads "Draft v0.1" but the work is in main at v0.4.3 (commit
      2df7156). PRD file is still in the queue dir, NOT archived.
      Self-referential drift: the PRD's frontmatter is a stale
      claim about its own implementation state.
    - **PRD-recall-bash-response-richness.md** — "Draft v0.1",
      "Builds on recall v0.4.2", not yet shipped.
    - **PRD-recall-stop-hook-session-id.md** — "Draft v0.1",
      "Builds on recall v0.4.2", not yet shipped.
  This is on-theme for the freshness vision (a document body whose
  claims about live state are wrong). Logged into
  `visions/freshness.md` §Evidence log; not drafted as a new PRD
  per dream rule 6.

**Notes for /build:** when next ticking
PRD-recall-braid-freshness-tunable.md: the work is already in
main. Reconcile — either archive the PRD as shipped (preferred) or
detect the version-already-bumped state in `scan-prds.sh` and skip.
A natural rule: "if PRD `Builds on: recall vX.Y.Z` and live
Cargo.toml version > X.Y.Z, flag for archival." Other orphan PRDs
unchanged; recall-daemon iter-4 remains the visible next ship.

**Notes for next /dream:** same unblock conditions as 08:30,
plus one new sign-post:
  - recall-daemon to v0.5.0 → recall fleet gets daemon-aware
    follow-ons.
  - kernel boots into linux-wintermute → continuity Fleet 1
    boot-gated ACs become testable; single biggest unblock.
  - any **Fleet 1** PRD ships (six fleets, six triggers).
  - a real new user-articulated topic.
  - **NEW: if a 4th orphan-PRD ship lands without reconciliation**,
    consider a small "recall-pulse" vision that retroactively folds
    the orphan recall-* PRDs under one umbrella. Holding back per
    dream rule 6 until at least one more orphan ship occurs.

If none of these land before the next bare /dream, a fourth
no-fleet-pass is the right call. The discipline still holds.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede
  - handshake Fleet 2 after Fleet 1 ships + 1 observed race

## 2026-05-25T10:00  /dream  no-fleet-pass
Bare `/dream`, 45min after 09:15. **Fourth consecutive no-fleet-pass**
— the call 09:15 predicted ("a fourth no-fleet-pass is the right
call").

**State delta since 09:15:**
  1. /build hit a sustained throughput stall between 02:17–02:58 PT
     (09:17Z–09:58Z): 9 cron fires, 0 successful ticks. Six fires
     bounced on `tick.lock` held by a sibling timer-fired tick;
     three bounced on classifier-unavailable. PID 481429 finally
     took the lock at 02:58 (currently in-flight, 3min elapsed).
     Net result: no commits from 09:15Z onward across Fleet 1.
  2. recall-daemon: still iter-3, last_action 08:26Z. No v0.5.0.
  3. recall tip: still `fdc81ad` (v0.4.3 CHANGELOG backfill). No
     new ships, no new orphan PRDs.
  4. wintermute-bootstrap manifest now `last_action: 09:15Z` —
     iter-5 (AC4 OnceStart, commit 796852b at 09:08Z) re-touched
     during the 09:15 dream window; still in_progress (AC1+AC3
     deferred). Not a Fleet-2-trigger ship.
  5. wintermute-platform manifest `last_action: 09:02Z` — iter-9
     clippy fix (5b10fb6 at 08:57Z); still in_progress.
  6. Kernel: stock 7.0.9-arch1-1, pkgs unchanged at 00:54.
  7. No new user articulation; bare /dream.

**Why no fleet drafted:**
  - None of the six unblock conditions fired (recall-daemon→v0.5.0,
    kernel boot, any Fleet 1 ship, new orphan ship for recall-pulse
    hook, user articulation, …). The build-tick contention pattern
    is a /build hygiene issue, not vision-shaped: it surfaces a
    candidate fix (timer interval > expected wall time, or
    self-contention exit-fast), but the fix author is /build, not
    /dream, and a PRD would be misshaped here.

**One micro-finding logged this pass — /build self-contention:**
  systemd-user timer interval is 5min; observed wall time between
  09:17Z and 09:58Z had multiple concurrent `claude-build-headless.sh`
  processes (PIDs 474583 / 476041 / 476669 / 478094 / 479993 / 481429
  in sequence, several overlapping for >5min). When classifier
  flaps add 60–90s retries on top of an already-long tick, the
  cron cadence is shorter than steady-state wall time and ticks
  race themselves. Phase 0's lock-guard absorbs the contention
  (every loser exits cleanly without mutating state — that part
  works), but the *cost* is that 9 of 9 fires in 41min did zero
  PRD-advance work; the wall-clock advance rate during the stall
  was 0 commits/41min vs the baseline of ~1 commit per cron-fire.
  This is not new from a steady-state perspective (the lock guard
  is doing its job) but it IS a new visible bottleneck shape
  that wasn't present at 09:15. Logged to gossip; future /dream
  should NOT draft a PRD for this — it's /build's domain to widen
  the cron interval, add a self-detect-running early-exit, or
  collapse classifier retries.

**Notes for /build:** when the in-flight PID 481429 tick completes,
consider:
  - widening the systemd-user timer to 10min (matches observed
    p95 wall time including classifier flap retries), OR
  - early-exit if a sibling has been running >2min (the lock
    age is detectable via `ls -la tick.lock` mtime).
  This is a /build-skill self-mod, not a /dream PRD; surfacing as
  a finding for next /self-review or /build's own Phase 6
  follow-on draft. recall-daemon iter-4 (CLI auto-forward +
  systemd-user unit toward v0.5.0) is still the visible next ship
  if the throughput stall clears.

**Notes for next /dream:** unblock conditions unchanged from 09:15.
A fifth no-fleet-pass is appropriate if none have fired by the
next bare /dream. The discipline still holds — it explicitly
predicts itself one step out.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede
  - handshake Fleet 2 after Fleet 1 ships + 1 observed race

## 2026-05-25T10:45  /dream  no-fleet-pass
Bare `/dream`, 45min after 10:00. **Fifth consecutive no-fleet-pass** —
the call 10:00 predicted ("a fifth no-fleet-pass is appropriate if
none have fired by the next bare /dream").

**State delta since 10:00:**
  1. /build throughput: the in-flight PID 481429 tick that 10:00
     described as "3min elapsed" eventually completed without
     advancing any Fleet 1 PRD ship. Wintermute-bootstrap last_action
     unchanged from 09:15Z; wintermute-platform 09:02Z; wintermute-tts
     06:09Z; recall-daemon 08:26Z. No Fleet 1 ships in the 45min
     window.
  2. recall tip: still `fdc81ad` (v0.4.3 CHANGELOG backfill).
  3. recall-daemon: still iter-3, no v0.5.0.
  4. Kernel: still booted on stock 7.0.9-arch1-1. `linux-wintermute-
     7.0.10.arch1-1` pkg unchanged from 00:54.
  5. No new user articulation.

**Why no fleet drafted:**
  - None of the six unblock conditions fired (recall-daemon→v0.5.0,
    kernel boot, any Fleet 1 PRD ship, new orphan-PRD ship for
    recall-pulse hook, user articulation, …).

**Self-correction logged this pass — meta-freshness:**
  The 09:15 gossip note about recall-braid-freshness-tunable as an
  "orphan-PRD ship" had a stale claim of its own. Quoted:
    > "NOT in the build manifest's prds object (manifest read
    >  confirms only build-rust-extend, agentic-memory,
    >  recall-daemon, recall-observer-correlation,
    >  recall-outcome-feedback, plus notebooks/wintermute-audio)."
  Live re-check at 10:45Z: the PRD has been in the manifest since
  iter-1 at 04:50Z — 4h17m before the 09:15 dream pass called it an
  orphan. Manifest now shows status=in_progress (3 ticks),
  shipped_version=0.4.3, changelog_committed=fdc81ad,
  installed_versions={recall: 0.4.3, recalld: 0.4.3}. What's
  actually pending: archival (move PRD to PRDs-archive/, flip
  status to shipped). The 09:15 narrative confused
  "manifest doesn't track this PRD" with "PRD frontmatter still
  reads Draft v0.1" — a different fact, and the only true one.
  Likely cause: I read manifest at 09:15 but truncated mid-object
  via `head -100`; the recall-braid-* entry sits after the first
  five PRDs alphabetically and was below my window.

  This is on-theme for vision-freshness with extra force: the
  artifact whose claims drifted is gossip.md itself, drafted by
  the same skill that exists to *write* drift-free notes about
  ground truth. The freshness-on-dream Fleet 2 candidate (logged
  08:30) is reinforced: a spot-check pass over dream's own
  outputs before commit would have caught this.

  Logged into visions/freshness.md §Evidence log; not drafted
  per dream rule 6. Pattern now has FOUR instances:
    - feedback_delegate_run_300s_cap.md "hardcodes" wording
    - AGORABUS_RPC.md v0.1 changelog "no handler shipped"
    - 06:30 gossip's `ctrace ls` invocation (invalid subcommand)
    - 09:15 gossip's orphan-PRD-manifest claim (false)
  Last two were authored by /dream itself within the last 6h.

**Notes for /build:** the actionable hint from 09:15 is still valid
in spirit but more specific in shape — recall-braid-freshness-tunable
needs an **archival tick** (status:in_progress → status:shipped, mv
PRD to PRDs-archive/), not version-bump work. The
build_stale_blockers playbook may not currently detect "all version
work is done, only archival remains" as a discrete state — worth a
/build-side check on its own scan logic. recall-daemon iter-4 (CLI
auto-forward + systemd-user unit toward v0.5.0) remains the visible
next ship.

**Notes for next /dream:** unblock conditions unchanged. A sixth
no-fleet-pass is appropriate if none have fired by the next bare
/dream. If a sixth pass happens, the discipline-test pattern itself
becomes evidence — proposing it as a Fleet 2 entry for
vision-cadence ("dream rest-pace heuristic: bare /dream within Nmin
of last no-fleet-pass and unchanged state → respond with vision
list + one paragraph instead of a fresh research pass") may be
warranted. Holding back per dream rule 6 until pass six.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede
  - handshake Fleet 2 after Fleet 1 ships + 1 observed race

## 2026-05-25T11:30  /dream  no-fleet-pass
Bare `/dream`, 45min after 10:45. **Sixth consecutive no-fleet-pass** —
the call 10:45 predicted ("a sixth no-fleet-pass is appropriate if
none have fired by the next bare /dream").

**State delta since 10:45 (real, but sub-threshold):**
  1. `recall-daemon` advanced from iter-3 to **iter-4** at 11:13Z
     (commit `8949d32`, pushed to `origin/main`). New surface:
     `recall daemon status` subcommand (text+json, exit-code on
     liveness) + `contrib/systemd/recalld.service` user-unit +
     `recall where` now reports socket liveness. Lib 31/31 +
     daemon_ping 4/4 unchanged; no version bump (v0.5.0 still
     gated on iter-5 CLI auto-forward).
  2. /build cron has fired ~3x since (11:16/11:21/11:26Z) — all
     bounced on `tick.lock` held by the headless-cron sibling. The
     10:00 self-contention finding persists.
  3. recall tip: `fdc81ad` → `8949d32`.
  4. Kernel: still stock 7.0.9-arch1-1.
  5. No new user articulation.

**Why no fleet drafted:**
  - The iter-4 commit is real motion on `recall-daemon`, but it is
    a within-PRD iteration, not a Fleet 1 ship. The explicit
    trigger from 10:45 (`recall-daemon→v0.5.0`) hasn't fired.
  - None of the other five unblock conditions fired either.

**Fleet 2 candidate landed (per 10:45's tee-up):** added
`dream rest-pace heuristic` as a bullet in `visions/cadence.md`
§Fleet 2. Six consecutive passes is enough evidence the pattern
is real and recurring; cadence is the natural vision-home
(pulse-like signal applied to /dream's own invocation cadence).
This is a vision-doc bullet only, not a drafted PRD — rule 6
still holds; drafting waits for `cadence-pulse` to ship so the
heuristic has a stable substrate to read.

**Discipline-test summary (07:50Z → 11:30Z, 3h40m, 6 passes):**
  - Pass 7 (07:50): saturation + no new evidence
  - Pass 8 (08:30): predicted pass 9; logged stale `ctrace ls`
    + first /dream meta-stale claim (PID 12146 "still running")
  - Pass 9 (09:15): orphan-PRD misread (later self-corrected)
  - Pass 10 (10:00): /build self-contention micro-finding
  - Pass 11 (10:45): self-correction of pass 9's manifest misread;
    teed up the Fleet 2 entry for pass 12
  - Pass 12 (11:30): consumed the tee-up; cadence Fleet 2 bullet
    added; pattern documented as complete.
Every pass logged a state delta. Every pass predicted the next
correctly. Two passes drafted stale claims in their own gossip
(passes 8 and 9), both self-corrected within 2h. Zero PRDs
drafted past evidence. Net writes per pass: 1 gossip entry +
1 manifest append + 0-1 vision-evidence-log lines.

**Notes for /build:** recall-daemon iter-5 (CLI query auto-forward)
is the visible next ship to unlock the v0.5.0 boundary. Once that
lands, the recall-outcome-feedback + recall-session-stamp PRDs
(both pinned behind v0.5.0) become eligible. Tick.lock contention
remains — 10:00 hint (widen timer interval or sibling-age early-
exit) still applicable.

**Notes for next /dream:** unblock conditions unchanged. A seventh
no-fleet-pass would be **excessive**, not disciplined. If state is
still unchanged at the next bare invocation, /dream should follow
its own newly-bulleted rest-pace heuristic spirit: reply with a
one-paragraph state delta + this pass's unblock list, and skip
even the manifest/gossip writes (logging "I did nothing" six times
is itself a noise floor). The bullet exists now; living the bullet
before the PRD ships is acceptable since the change is to /dream's
own behavior, not to a downstream tool.

**Still pending from earlier /dream notes (unchanged):**
  - wintermute-glow once bootstrap+platform ship
  - wintermute-offline-persona once brain is up
  - continuity Fleet 2 after >=3 of 5 Fleet 1 PRDs ship
  - wintermute Fleet 2 after >=3 of 7 Fleet 1 PRDs ship
  - cadence Fleet 2 after >=1 of 7 Fleet 1 PRDs ship
  - chord Fleet 2 after >=2 of 4 Fleet 1 PRDs ship
  - freshness Fleet 2 after Fleet 1 ships + 1 promoted supersede
  - handshake Fleet 2 after Fleet 1 ships + 1 observed race

## 2026-05-25T13:45  /dream  no-fleet-pass (trigger-fired, scope mismatch)
Bare `/dream` 2h15min after 11:30. **Seventh /dream-pass in the
3h40m arc + 2h15m gap; first since-then.** Discipline still holds.

**State delta since 11:30 (real, but vision-orthogonal):**
  1. `recall-daemon` shipped **v0.5.0** at commit `4333b18` somewhere
     between 12:26Z and 13:45Z (manifest snapshot at 12:26Z still
     called iter-5 pre-bump). This is the exact trigger 10:45 and
     11:30 named as the visible next ship to unlock the v0.5.0
     boundary. ✅ FIRED.
  2. Kernel still stock 7.0.9-arch1-1.
  3. No user articulation; bare `/dream`.

**Vision Fleet 1 counts: unchanged.** recall-daemon is a build-manifest
PRD, not in any vision's Fleet 1. None of the six Fleet 2 triggers
(continuity ≥3/5, wintermute ≥3/7, cadence ≥1/7, chord ≥2/4, freshness
≥1/1, handshake ≥1/1) fire. The ship is real motion but
vision-orthogonal.

**Actual downstream unblocks (for /build, not /dream):**
  - `recall-outcome-feedback`: pinned behind v0.5.0, rebases to
    v0.5.1/0.5.2/0.5.3 per its PRD §6 plan. Already drafted; /build
    can pick it up.
  - `recall-session-stamp` (continuity Fleet 1, targets v0.6.0): the
    v0.5.0 collision risk that the continuity vision-doc called out
    is now resolved. Already drafted; /build can pick it up.

**Why no fleet drafted this pass:**
  - The new substrate (UDS socket exposing ping/query-no-filter/embed/
    touch) is minimal. Streaming/subscribe and filter-rich daemon
    queries are NOT exposed; daemon-aware hook PRDs would be real but
    are "dream past the research" — the daemon shipped ~80min ago
    with zero downstream consumers. Wait for at least one consumer to
    tick through /build before imagining the next layer.
  - The 30-PRD queue across 6 visions is the saturating constraint,
    not idea generation.

**Self-application of the cadence-rest-pace heuristic** (from
visions/cadence.md §Fleet 2, banked at 11:30): this pass is the first
since the heuristic was banked. State *did* change — but the change
was build-side (PRD shipped from queue), not dream-side (new
vision-shaped white space). The heuristic was written for "unchanged
state"; this pass tests a refinement — "build-side change with no
fleet-trigger fire." Treating it identically (log state delta + skip
new drafts) is the right read. The PRD `cadence-pulse` should
generalize to "/dream rests when /build advances within an already-
drafted vision; only wakes when a Fleet 2 trigger fires OR new user
articulation arrives OR new kernel/substrate surface lands."

**Notes for /build:** with v0.5.0 shipped, you have two immediately-
eligible PRDs:
  1. recall-outcome-feedback (rebase to v0.5.1/0.5.2/0.5.3)
  2. recall-session-stamp (continuity Fleet 1, v0.6.0)
Pick by priority/heat. Also: the tick.lock self-contention (10:00
finding) probably worth addressing before either, since both are
multi-iter PRDs that will benefit from a less self-racing cron.

**Notes for next /dream:** unblock conditions narrow further now that
recall-daemon→v0.5.0 has cleared:
  - any continuity Fleet 1 ship (5 candidates; recall-session-stamp
    is now the most-eligible)
  - any cadence Fleet 1 ship (7 candidates; cadence-substrate is
    foundational)
  - any chord Fleet 1 ship (4 candidates; chord-intent-rich is
    foundational)
  - any wintermute Fleet 1 ship (7 candidates; bootstrap+platform
    closest)
  - freshness or handshake Fleet 1 ship (1 each; both shell-target,
    fastest)
  - kernel boot (linux-wintermute pkg → grub default)
  - new user articulation OR new substrate landing in main
Without one of those, the next bare /dream should rest-pace harder:
state delta one-liner, no new gossip entry, no manifest write.

## 2026-05-26T04:00  /dream  no-fleet-pass (rest-pace, ninth)
Bare `/dream` ~14h after 2026-05-25T13:45. **Ninth /dream pass in the
arc, first post-rest.** Followed the cadence rest-pace heuristic
(banked 11:30, refined 13:45): real motion since last pass but
vision-orthogonal → log state delta + add evidence to existing
vision + skip new drafts + skip manifest write.

**State delta since 13:45 (substantive, vision-orthogonal):**
  1. `recall-daemon` v0.5.0 fully shipped end-to-end: iter-7 changelog
     2781c70 (16:25Z), iter-8 push blocked (23:36Z), iter-9 push landed
     01:05Z (range 8949d32..2781c70 — three commits f231524 + 4333b18
     + 2781c70 now on origin/main).
  2. iter-10 AC verification (01:22Z) caught a partial ship — AC1/4/6
     failed: `recall daemon start/stop/restart` subcommands never
     landed (help text claims iter-5 ships them; v0.5.0 only has
     `status`), and `recall doctor --format json` does not expose
     `daemon_active` or `daemon_uptime_s`. PRD NOT archivable until
     iter-11 closes the gap.
  3. iter-11 WIP is real but entangled: `~/wintermute/recall` working
     tree carries 4 modified files + 1 new test (`hooks/stop.sh`,
     `src/bin/recalld.rs`, `src/daemon.rs`, `src/main.rs`,
     `tests/hook_stop_session_id.rs`) — mix of recall-daemon iter-11
     scope AND sibling PRD-recall-stop-hook-session-id scope. /build
     deferred per Hard Safety Rule #5.
  4. Self-review run 8 (20:03 PT 2026-05-25) was the quietest run
     yet — confirms steady-state on most signals, escalated
     iter-11 entanglement to a /build blocker (blocker count 2→4).
  5. Kernel still stock 7.0.9. No new user articulation.

**Vision Fleet 1 counts: unchanged.** recall-daemon is build-manifest,
not vision. None of the six Fleet 2 triggers fire (continuity ≥3/5,
wintermute ≥3/7, cadence ≥1/7, chord ≥2/4, freshness ≥1/1,
handshake ≥1/1).

**One evidence-log line added (visions/chord.md §Evidence):** the
iter-10/iter-11 entanglement is the first real-world motivation for
PRD-chord-claim's soft-lock primitive. Had iter-11's originating
session claimed `repo:recall` on agorabus before editing, the
sibling stop-hook-session-id work would have backed off or queued.
Strengthens AC1+AC4 of PRD-chord-claim.md (lock acquisition +
visible holders). Not promoted to a new PRD — chord-claim already
exists, this just sharpens its motivation.

**Why no fleet drafted (rule 6):**
  - 30 PRDs across 6 visions queued; 0 Fleet 1 ships. Saturation
    is the constraint, not idea generation.
  - The new substrate (recall-daemon v0.5.0 UDS) is minimal and has
    zero downstream consumers; daemon-aware-hook PRDs would be
    "dream past the research."
  - The iter-11 entanglement motivates an *existing* PRD; no new
    PRD is warranted.

**Notes for /build:** the iter-11 working-tree state is the immediate
unblock target. Options: (a) attribute the diff to one or the other
PRD and commit the rest separately, (b) reset to HEAD and re-pick
one PRD to advance, (c) ask user. With recall-daemon's v0.5.0
shipped (commits public) but PRD un-archivable due to AC gap, the
work isn't lost — but archive is gated on iter-11 closing AC1/4/6.

Other eligible /build picks (per 13:45 list, still valid):
  - recall-outcome-feedback (rebase to v0.5.1/0.5.2/0.5.3) — does
    NOT touch the entangled files, can advance now.
  - recall-session-stamp (continuity Fleet 1, v0.6.0) — does NOT
    touch the entangled files, can advance now and would fire
    continuity Fleet 1's first ship.

**Notes for next /dream:** unblock conditions narrow again:
  - any Fleet 1 ship from any of the six visions
  - kernel boot (linux-wintermute pkg → grub default)
  - new user articulation OR new substrate landing with at least
    one downstream consumer
  - iter-11 entanglement resolution becomes a second chord-claim
    evidence line if /build records the resolution path chosen
Without one of those, next bare /dream should rest harder: terse
state delta in chat, no gossip entry, no manifest write. The
cadence-pulse PRD when it ships will codify this.

## 2026-05-26T06:20  /dream  vision-release-gate (tenth pass, FIRST DRAFT since pass 6)
Bare `/dream` ~2h20min after 2026-05-26T04:00 (ninth pass,
rest-pace). Tenth /dream pass in the arc. **First pass to draft
new artifacts since pass 6 (2026-05-25T05:25 vision-chord, 25h ago).**

**Why this pass drafted (broke the rest-pace streak):**
The cadence-rest-pace heuristic (banked 11:30 2026-05-25, refined
13:45) says rest when /build advances within an already-drafted
vision. But pass 10 saw evidence of a NEW failure mode unaddressed
by any existing PRD: the `git push origin main` gate fired AGAIN
against recall-daemon iter-15 at 2026-05-26T05:40Z — second firing
on the same PRD (first was iter-8 at 2026-05-25T23:36Z). The
publish-allowlist PRD (drafted by /build Phase 6 on 2026-05-26)
explicitly puts this case OUT OF SCOPE on line 164. The /build
iter-15 manifest entry mis-cites publish-allowlist as the fix
("Resolution path: PRD-build-publish-allowlist.md ... explicitly
targets this"), which means /build's own Phase 6 may not draft a
sibling PRD because /build believes the case is already covered.
This is the rest-pace heuristic's "new substrate with consumer"
exception inverted: NEW failure mode + WRONG resolution-path
citation in /build manifest = /dream is the closer author.

**Drafted (1 vision, 1 PRD):**
  - **visions/release-gate.md** — small vision (2 PRDs Fleet 1,
    one already queued). Frames the publish-vs-push gate pattern
    as a single class with symmetric solutions. Fleet 2 captured
    as bullets (release-gate-repos-md-sync, -prerelease, -revert)
    but explicitly NOT drafted per rule 6.
  - **PRD-build-push-allowlist.md** — `self-mod`, `build_auto:false`,
    `build_priority: high`, sibling to PRD-build-publish-allowlist.md.
    Adds `~/.local/bin/wm-push` wrapper + `Bash(wm-push:*)` allow
    rule + /build Phase 4 patch. Wrapper checks: slug regex + hard-
    coded allow-list + origin URL ends in `j0yen/<slug>` + current
    branch matches target + fast-forward only (no force-push) +
    refuses no-op. 10 ACs mirroring publish-allowlist's structure.
    Single-tick phasing, <20min estimated. Can be authorized in
    same user review pass as publish-allowlist.

**Vision Fleet 1 counts:** release-gate = 1 drafted + 1 already-
queued (publish-allowlist) = 2/2. Other six visions unchanged.
None of the prior Fleet 2 triggers fire from this pass.

**State delta since 2026-05-26T04:00 /dream pass:**
  1. recall-daemon advanced iter-11 → iter-15 (commits 36cb6ea,
     aa0922c, 3abdf7b for v0.5.2 daemon lifecycle + doctor
     liveness + changelog; all 12 ACs PASS per iter-12 smoke).
  2. recall v0.5.2 fully built + installed locally; running
     daemon still on v0.5.0 binary until restart (cosmetic).
  3. iter-15 push BLOCKED — same shape as iter-8 23:36Z. Three
     commits queued local. Archive gated on push landing
     (verified-completed Check #2 for rust-extend).
  4. iter-11 entanglement (10:00 finding on chord-vision) resolved
     trivially — the WIP attributed cleanly to recall-daemon;
     recall-stop-hook-session-id landed separately at 32590f2.
     This is the SECOND chord-claim evidence line predicted by
     the 04:00 pass. NOT promoted to PRD draft (chord-claim
     already exists, this just sharpens motivation in passing).
  5. Kernel still 7.0.10-arch1-3-wintermute (only agentns
     registered; memlog + provfs still absent — same as last
     /self-review run 9).
  6. No new user articulation.

**Freshness evidence line added:** visions/freshness.md §Evidence
log gets a new entry for the publish-vs-push mis-citation pattern.
freshness-on-prds Fleet 2 case now covers PRD-vs-PRD
cross-reference checking (not just PRD-vs-shipped-state).

**Notes for /build:**
  - **immediate**: recall-daemon iter-15 is blocked on push. Two
    paths to unblock: (a) user manually `git push origin main` from
    `~/wintermute/recall`, (b) user authorizes
    PRD-build-push-allowlist.md and PRD-build-publish-allowlist.md
    together (single review pass, both `build_priority: high`,
    single-tick phasing each, <20min combined). Path (b) is
    durable; (a) is a one-shot.
  - **don't draft a third publish-gate PRD in /build's Phase 6** —
    /dream has now drafted the sibling. release-gate vision has
    Fleet 1 complete (publish + push = 2 PRDs).
  - **update /build's iter-15 manifest entry** at next tick to
    correctly cite PRD-build-push-allowlist.md as the resolution
    path (currently cites publish-allowlist incorrectly).
  - other eligible /build picks (unchanged from 04:00 list, still
    blocked by same push-gate at their version-bump step):
    recall-outcome-feedback (v0.5.1/0.5.2/0.5.3),
    recall-session-stamp (v0.6.0). Both will hit the push gate at
    Phase 4 unless push-allowlist ships first.

**Notes for next /dream:** unblock conditions:
  - any Fleet 1 ship from any of the seven visions (now 7 with
    release-gate)
  - kernel boot WITH memlog + provfs registering (currently only
    agentns made it in; user pending investigate
    `~/wintermute/wintermute-kernel/pkg/`)
  - new user articulation OR new substrate landing with at least
    one downstream consumer
  - the chord-claim evidence count reaching 3 instances (currently
    2: iter-10/iter-11 entanglement + iter-11 resolution path)
Without one of those, next bare /dream should rest harder: terse
state delta in chat, no gossip entry, no manifest write.

**Self-application:** this pass IS a draft pass, not a rest-pace
pass, because evidence demanded it. The discipline arc holds —
nine rest-paces preceded one draft, not the other way around.

## 2026-05-27T05:30  /dream  vision-onramp (eleventh pass, second draft pass)
Bare `/dream` ~23h after 2026-05-26T06:20 (tenth pass, release-gate
draft). Eleventh /dream pass in the arc. **Second draft pass since
the discipline arc closed.**

**Why this pass drafted (Fleet 2 trigger fired):**
The rest-pace heuristic banked 13:45 names exactly this case:
"new kernel/substrate surface lands with at least one consumer."
`linux-wintermute 7.0.10-arch1-5` is BOOTED — confirmed live:
  - `uname -r` = `7.0.10-arch1-5-wintermute`
  - `cat /sys/kernel/security/lsm` includes `provfs`
  - `/dev/memlog` is a live char device (`crw-rw---- root:root 660`)
  - `/proc/self/ns/agent` exists
Consumers drafted in [continuity Fleet 1][continuity] (5 PRDs)
are the gating consumers; this is a "substrate landed with drafted
consumers" Fleet 2 fire.

Plus three empirically-observed gaps that NO existing PRD covers:
  1. `getent group memlog` returns empty (no group exists; udev
     never resolves /dev/memlog ownership)
  2. `cat /proc/self/agent_session` reads 32 zeros (no Claude
     session enters agentns; hook-time unshare is structurally
     impossible)
  3. `getfattr -d ~/wintermute/recall/Cargo.toml` returns
     `comm:awk:pid:76630:uid:1000` — provfs comm-fallback names
     the transient utility (`awk` in autobuilder pipeline), not
     the originating tool

The self-review runs 13/14/15 (2026-05-26) flagged #2 three runs
running with the wrong proposed fix ("edit agorabus-session-start.sh
to unshare"). The structural reality is that unshare is per-process
and self-only — a SessionStart hook can't enter the launched
process into a namespace post-hoc. Articulating the actual fix
(wrap the launch itself) is /dream's closer-author work.

**Drafted (1 vision, 3 PRDs):**
  - **visions/onramp.md** — small 3-PRD vision; the bridge between
    "kernel booted" and "tools consume it." Fleet 2 (5 bullets:
    agentns-launcher-hardening, memlog-readable-by-default,
    provfs-attribution-test-suite, onramp-doctor,
    kernel-pkg-postinstall-tests) explicitly NOT drafted per rule 6.
  - **PRD-kernel-pkg-postinstall.md** — `kernel-extend`, edits
    `~/wintermute/wintermute-kernel/pkg/PKGBUILD` (pkgrel 5→6),
    adds `.install` hook + sysusers.d for `memlog` group +
    udev rule for `/dev/memlog` ownership. 10 ACs (live AC10 needs
    pacman -U + reboot + login). No dependencies, ships first.
  - **PRD-claude-agentns-wrap.md** — `mixed`, edits `~/.zshrc` +
    `~/.config/systemd/user/*.service` + `agorabus-session-start.sh`.
    Three integration paths (interactive shell function, headless
    units, kernel-id-aware hook). Depends on PRD-agentns-claude.md
    being shipped + installed. 10 ACs (live AC10 needs in-session
    `/proc/self/agent_session` non-zero observation).
  - **PRD-provfs-comm-richer.md** — `kernel-extend`, pairs with
    PRD-provfs-deferred-stamp.md (shared hook-time capture buffer).
    Enriches fallback xattr value: comm chain (3 levels) + env
    signal (`CLAUDE_TOOL`, `AGORABUS_SID`) + cwd. 256-byte cap.
    agentns-present path unchanged. 10 ACs (live AC10 needs
    rebooted kernel + real workload).

**Vision Fleet 1 counts:**
  - continuity: 5 PRDs, 0 shipped (BUT 4 of 5 transitively
    blocked on PRD-claude-agentns-wrap)
  - chord: 4 PRDs, 0 shipped
  - cadence: 7 PRDs, 0 shipped
  - wintermute: 7 PRDs, 0 shipped
  - freshness: 1 PRD, 0 shipped
  - handshake: 1 PRD, 0 shipped
  - release-gate: 2 PRDs (1 drafted + 1 already-queued), 0 shipped
  - **onramp: 3 PRDs (NEW), 0 shipped**
Total: 8 visions, 30 queued PRDs (was 29 last pass + 3 new − 2 if
either of the release-gate or build-publish-allowlist PRDs ships
on a /build tick today). None of the seven Fleet 2 triggers from
prior visions fire from this pass.

**Notes for /build:**
  - **onramp-PRD-kernel-pkg-postinstall is the smallest unblock**
    (single PKGBUILD edit + 3 install assets; <30min not counting
    the kernel rebuild). User authorization needed (build_auto:false).
    Unblocks /dev/memlog usability for everyone going forward.
  - **PRD-claude-agentns-wrap depends on PRD-agentns-claude shipping
    first** ([continuity Fleet 1][continuity]); if /build picks
    continuity, PRD-agentns-claude is the natural starting point.
  - **PRD-provfs-comm-richer pairs with PRD-provfs-deferred-stamp**
    — if both ship in the same patch cycle, the kernel pkgrel
    bumps consolidate (5→6→7 becomes 5→7 in one rebuild).
  - **Eligible /build picks unchanged from yesterday's analysis:**
    recall-outcome-feedback push gate now resolved (origin/main at
    3abdf7b per iter-18); but 9 unpushed recall commits accumulated
    overnight (4 → 7 → 9) suggesting more shipped PRDs need
    archival ticks.

**Notes for next /dream:** unblock conditions narrow further now
that onramp is on the queue:
  - any Fleet 1 ship from any of the eight visions (now 8 with
    onramp; smallest path is onramp's kernel-pkg-postinstall or
    handshake's single shell-target PRD)
  - new kernel/substrate surface beyond the current 7.0.10-arch1-5
    (e.g. a memlog API extension; an agentns-counters new field)
  - new user articulation OR a NEW failure mode not covered by any
    queued PRD
  - any of the eight visions' Fleet 2 triggers firing
Without one of those, next bare /dream should rest-pace per the
discipline arc. The kernel-boot trigger is now consumed; treat
subsequent passes as steady-state until something genuinely new
arrives.

**Self-application:** this is the second draft pass since the
nine-rest-pace arc closed. The pattern holds: rest-pace nine times,
draft once when evidence demands. Discipline arc not violated;
extended.

[continuity]: ../visions/continuity.md

## 2026-05-28T01:40  /dream  vision-wintermute Fleet 2 (twelfth pass, third draft pass)
Bare `/dream` ~20h after 2026-05-27T05:30 onramp draft. Twelfth /dream
pass in the arc. **Third draft pass.**

**Why this pass drafted (clear Fleet 1 ship trigger):**
The wintermute vision doc explicitly authorized Fleet 2 extend at
">=3 of 7 Fleet 1 shipped." Current shipped count per CLAUDE_SELF
changelog: 5/7 (bootstrap archived; platform/tts/stt/dialog all have
"shipped" entries; only audio + brain remain queued). The 5/7 ship
count and the explicit vision-doc trigger together fire cleanly.

Plus a new user articulation (2026-05-27): the
[always-commit-push-prds-and-work][feedback] feedback memory lands
new rules — every PRD is auto-built (build_auto stripped from new
drafts), commits + pushes are inline (no batching), no daily caps.
Dream skill instructions updated to match; this is the first draft
pass under the new defaults.

**Drafted (1 vision update, 6 PRDs):**
  - **visions/wintermute.md** updated: Fleet 1 section gains shipped
    count (5/7); Fleet 2 section converted from bullets to drafted
    table with sequencing + bumped-to-Fleet-3 list (news + glow).
  - **PRD-wintermute-browser.md** — rust-cli, `wm-browser`,
    chromiumoxide-based. Tools: open/read/click/type/back/find/
    screenshot over `wm.browser.cmd`. A11y snapshot is canonical;
    image-mode fallback via wm-screen-narrate. 10 ACs (AC10 live).
    Calls out: no Rust Playwright binding exists — vision doc's
    "Playwright" label was shorthand.
  - **PRD-wintermute-desktop.md** — rust-cli, `wm-desktop`,
    atspi-rs + xdotool via baton. Tools: apps/focus/read_window/
    click/type/key/find. Reuses j0yen/baton (shipped 2026-05-24).
    AT-SPI bus auto-enable in install.sh. 10 ACs (AC10 live).
  - **PRD-wintermute-screen-narrate.md** — rust-cli,
    `wm-screen-narrate`, scrot/grim + Claude messages API vision.
    Tools: describe/read_text/find_in_image/screenshot. Defaults
    to focused window (privacy); per-day soft budget; logs cost
    to recall. 10 ACs (AC10 live).
  - **PRD-wintermute-mail.md** — rust-cli, `wm-mail`, async-imap +
    lettre + freedesktop SecretService. Tools: inbox/read/send/
    search/mark_read/delete/folders. send + delete through
    wm-dialog verbal confirm. IMAP IDLE for new-mail signal.
    wm-bootstrap extended with /mail credential page. 10 ACs.
  - **PRD-wintermute-calendar.md** — rust-cli, `wm-cal`, minicaldav
    + ical. Tools: today/range/add/find/delete/calendars/
    set_calendar. add + delete through verbal confirm. Reminders
    via `wm.cal.event.upcoming` 5min before. wm-bootstrap /cal
    page. 10 ACs.
  - **PRD-wintermute-music.md** — rust-cli, `wm-music`, mpris-rs
    over zbus. Tools: players/play/pause/toggle/next/prev/
    now_playing/set_volume. Control-only; provider catalog/launch
    explicitly out of scope. Smallest ship in Fleet 2. 10 ACs.

**Vision Fleet 1 counts (state delta since 2026-05-27T05:30):**
  - continuity: 5 PRDs, 0 archived; PRD-agentns-claude unchanged
  - chord: 4 PRDs, 0 shipped
  - cadence: 7 PRDs, 0 shipped
  - **wintermute: 7 PRDs, 1 archived + 4 binary-shipped per
    CLAUDE_SELF → 5/7 effective. Fleet 2 NOW 6 PRDs drafted**
  - freshness: 1 PRD, 0 shipped
  - handshake: 1 PRD, 0 shipped
  - release-gate: 2 PRDs, **BOTH SHIPPED → vision fulfilled**
    (publish-allowlist + push-allowlist both in PRDs-archive/;
    also PRD-build-changelog-prepend-fix shipped as adjacent)
  - onramp: 3 PRDs, 0 shipped (substrate state unchanged:
    agent_session still zeros; memlog group still missing;
    provfs still names comm:awk for autobuilder writes)
Total: 8 visions, **36 queued PRDs** (was 30+3 onramp + 5 backlog
adds + 6 Fleet 2 new − 3 release-gate shipped = ~36; precise count
in `ls PRD-*.md` = 35 after this pass since wintermute-platform/
tts/stt/dialog still in queue dir pending archive).

**Notes for /build:**
  - **release-gate is closed** — both wrappers shipped. push +
    publish path is now durable, no longer the structural blocker
    described in 2026-05-26 gossip.
  - **wintermute Fleet 2 is six fresh queued PRDs.** Sequencing
    hint per vision update: browser/desktop are the big two
    (~1 autobuilder cycle each); music is the cheapest first ship
    (~30 min). Mail/calendar require the wm-bootstrap extension
    arm — if those are picked first, factor in the bootstrap
    side-edit. screen-narrate uses Claude vision API + cost
    budget; touch the claude-api skill on first build.
  - **Two wintermute Fleet 1 PRDs still queued + likely unshipped
    in code:** wintermute-audio + wintermute-brain. Brain is the
    capstone; audio is the perception gate for the rest of Fleet 1.
    Worth knowing if /build is choosing between Fleet 1 completion
    vs Fleet 2 starts.
  - **substrate gaps unchanged**: /dev/memlog group/udev still
    missing; agentns wrap still missing; provfs comm-fallback still
    coarse. Onramp Fleet 1 (3 PRDs from prior pass) all still
    queued.

**Notes for next /dream:** unblock conditions:
  - Any Fleet 1 ship from continuity/chord/cadence/freshness/
    handshake/onramp/wintermute (audio or brain) — five of these
    have NEVER shipped a Fleet 1 PRD; first ship for any of them
    would warrant a refresh pass.
  - Any wintermute Fleet 2 ship — would close one of the new ones
    and rebalance the queue.
  - New user articulation OR new substrate landing (kernel rebuild
    with memlog ownership rule; agentns wrap landing in a Claude
    launcher; etc.)
  - >=2 wintermute Fleet 2 ships → consider drafting Fleet 3
    (voice-profile, voice-clone, emergency, quiet-hours,
    multi-user, undo, offline-persona).

[feedback]: ~/.claude/projects/-home-jsy/memory/feedback_always_commit_push.md

## 2026-05-27T21:35  /dream  pass 13 — Fleet 1.5
Drafted: PRD-build-deferred-acs.md
Vision: visions/wintermute.md updated with new "Fleet 1.5 — Maturation
  & validation" section (1 PRD drafted, 2 bullets for future passes:
  wm-verify, build-maturation-log).

**Trigger:** 4 wintermute Fleet 1 PRDs (platform, audio, stt, tts) all
stuck in_progress on identically-shaped hardware-dependent AC pairing
after today's publish flurry. 68 combined ticks invested across the 4
without the verified-completed check #5 gate satisfiable. Same pattern
across 4 instances = structural problem, not effortful.

**What this PRD does:** adds `deferred_acs: [N, M]` to PRD frontmatter
+ teaches /build's check #5 to honor it + writes a `Deferred:` trailer
in archive commits + backfills the 4 stuck PRDs as part of its install
action. Single tick. Sibling shape to build-publish-allowlist and
build-push-allowlist (both shipped today). `build_target: self-mod`,
`build_priority: high`.

**Notes for /build:** PRD-wintermute-dialog is one tick from archive
(iter log says "Next tick: archive PRD-wintermute-dialog.md") — that
one doesn't need this PRD. The other 4 stuck PRDs are blocked until
deferred-acs lands. After this PRD ships and backfills, expect 4
archive actions to fire in rapid succession.

**Notes for next /dream:** unblock conditions:
- build-deferred-acs ships → check that backfill actually freed the 4
  stuck PRDs; if not, the gap is more subtle than the iter logs admit
- any wintermute Fleet 2 ship → Fleet 3 draft trigger (>=2 ships)
- new user articulation
- the wm-verify Fleet 1.5 bullet reaches its own trigger (declared-
  deferred ACs exist in PRD frontmatter, motivating an attestation
  walker)


## 2026-05-27T22:00  /dream  vision-daily-receipt (pass 14, NEW vision)
Drafted: PRD-daily-receipt-summarize.md, PRD-daily-receipt-haiku.md,
  PRD-daily-receipt-stamps.md, PRD-daily-receipt-archive.md,
  PRD-daily-receipt-yearend-letter.md
Vision: visions/daily-receipt.md (new)

**Trigger:** user articulation this session — MASUNG IP1000 thermal
printer arrived 2026-05-27, PRD-daily-receipt-printer just queued
(58mm, /dev/usb/lp0 live, paper en route). User: "Articulate the
haiku-composition + year-end-scroll arc downstream of it." Vision-
worthy: 4 distinct named components + a capstone, all motivated by
the 2026-05-22 archived daily-receipt PRD's never-built §4 pipeline
and §9 open questions.

**Order (critical path):**
  1. daily-receipt-printer (THIS SESSION, queued) — bytes meet paper
  2. daily-receipt-summarize — gathers ctrace+git+recall+journal into
     summary.json; the upstream the original PRD §4 named but
     never built
  3. daily-receipt-haiku — Claude API call producing 3-line content
     from summary.json; cached system+few-shot, <$4/year
  4. daily-receipt-stamps — special-day catalog (sibling, ships any-
     time after #1; seeds itself with "printer-arrives" 2026-05-27)
  5. daily-receipt-archive — annual PDF from cadence's `daily`
     records; depends on cadence-substrate + cadence-bind-daily-
     receipt landing first
  6. daily-receipt-yearend-letter — Dec-31-23:55 long thermal strip
     + PNG twin for the archive PDF cover; depends on #5 and #3

**Notes for /build:**
  - Summarize → haiku gets workdays printing real content within
    ~3 ship cycles after the printer wrapper lands.
  - Stamps is the cheapest first ship of the 5 here (no API, no
    PDF, just JSON + render). Good "warm up" candidate.
  - Archive + yearend-letter both depend on the cadence fleet
    (substrate + bind-daily-receipt). If cadence-substrate is
    still queued when /build gets here, defer these two until it
    ships. They're well-formed PRDs regardless.
  - yearend-letter has a small rust-extend side-edit on daily-
    receipt itself (`render_long_text`). Build that crate first,
    then yearend-letter consumes it via path dep.
  - Cost ceiling for the whole arc (haiku + yearend-letter)
    estimated at <$5/year. Don't over-engineer cost controls.

**Vision doc Fleet 2 bullets (next /dream pass material):**
  - daily-receipt-photo (monthly scan-prompt ritual)
  - daily-receipt-redo (reprint a past day from cache)
  - daily-receipt-status-board (web view of the year's grid)
  - glyph vocabulary v2 (bigram-shaped, not noise — after ~30
    quiet-day strips give a feel for what's missing)
  - build-shipped milestones as automatic stamps (gossip hook)
  - K and M strips (audience-shaped haikus; multi-printer mirror)

**Notes for next /dream:** unblock conditions:
  - daily-receipt-printer ships → fix any device-quirk PRDs that
    surface (IP1000 might surprise us — paper-out detection, cut
    behavior, CP437 codepage edges, etc.)
  - Paper arrives + first real strip prints → smoke-test PRD shape
    might need revision based on real-world physical output
  - Any of summarize/haiku/stamps ships → archive + yearend-letter
    PRDs become unblock-ready (assuming cadence fleet keeps moving)
  - >=30 days of real strips accumulated → revisit glyph vocabulary
    v2, re-roll budget, and the K/M strip question with real data

## 2026-05-28T01:50  /dream  pass 14 — vision-drift
Drafted: PRD-drift-fix-self-review-dream.md, PRD-tool-manifest.md, PRD-skill-doctor.md
Vision: visions/drift.md
Order: drift-fix-self-review-dream (independent, ship first) || tool-manifest
  (no deps, ship parallel) -> skill-doctor (depends on tool-manifest)

**Seed:** Bare /dream invocation. No unblock condition from pass 13's
list was met (build-deferred-acs not shipped yet, no Fleet 2 ships, no
new user articulation, no wm-verify trigger). Looked for fresh white
space; found it: 6+ consecutive self-review ticks have flagged the same
three tool-skill drift instances in their journal entries without
resolution. Pattern is structural, not effortful — nobody owns "fix
the drifting flag." Plus a 4th instance surfaced during Phase 1
(ctrace ls in dream/SKILL.md:86, already in freshness evidence log).
All four verified live via direct probe.

**Live evidence:**
  1. `pevent gc --older-than 7d --dry-run` cited at
     `self-review/SKILL.md:74,170,389`. Installed: only `[-h]
     [--older-than OLDER_THAN]`. `--dry-run` doesn't exist; `7d`
     errors as "invalid float value: '7d'".
  2. `bpolicy status --format json` cited at `self-review/SKILL.md:77`.
     Installed `bpolicy status` accepts `[-h]` only.
  3. Bootstrap-symlinks 13-tool list at `self-review/SKILL.md:93`.
     7 of 13 missing from `~/.local/bin/`: skill, episode, apipe,
     recall-ops, recall-doctor, recall-io, mirror. Yields 7
     false-positive DANGLING lines every self-review tick.
  4. `ctrace ls` cited at `dream/SKILL.md:86`. Installed: subcommands
     are start|stop|status|query|tail. Already flagged in
     `visions/freshness.md` evidence log.

**Vision shape:** Sibling to freshness/recall-doctor-claims —
freshness catches drift in memory bodies; drift catches drift in skill
text. Same proposal-queue idiom (`~/.claude/<tool>/proposals/<ULID>.md`,
no auto-edit, user-review-gated), different data source.

**Notes for /build:**
  - drift-fix-self-review-dream is a single-tick shell-extend edit
    over two SKILL.md files. Smallest ship in vision-drift; closes
    the noise loop immediately. Verify each replacement invocation
    against the live binary BEFORE writing it.
  - tool-manifest is a fresh rust-cli, new repo at
    `~/wintermute/tool-manifest/`, new GitHub repo j0yen/tool-manifest.
    Foundational — skill-doctor reads its JSON output.
  - skill-doctor is a fresh rust-cli, new repo at
    `~/wintermute/skill-doctor/`. AC11 is the verified-completed
    gate (one user-promoted proposal must land as an actual skill
    edit), mirroring freshness/recall-doctor-claims AC10.
  - All 3 PRDs `build_auto: false` per default /dream rule (vision
    is opt-in until user articulates).

**Notes for next /dream:** unblock conditions:
  - drift-fix-self-review-dream ships → next self-review tick should
    log zero of the four flagged instances; verify in journal.
  - tool-manifest ships → skill-doctor unblocks.
  - skill-doctor ships + first user-promoted proposal lands →
    Fleet 2 draftable (drift-self-review-integration,
    drift-cli-help-snapshot, drift-changelog-witness,
    drift-config-files, drift-bootstrap-truth).
  - Any wintermute Fleet 2 ship → Fleet 3 trigger (>=2 ships).
  - build-deferred-acs ships → check Fleet 1 unblock effect.
  - New user articulation.

**Cross-fleet notes:**
  - freshness vision composes naturally: a future Fleet 2 unified-
    proposals skill could merge `recall doctor --check-claims`'s
    proposals with `skill-doctor`'s.
  - No collision with chord, cadence, continuity, handshake,
    onramp, release-gate, wintermute fleets.

## 2026-05-28T01:55  /dream  pass 14 — postscript (accidental sweep)
Commit `ac38446` (the drift-vision commit) accidentally also included
7 daily-receipt artifacts (PRD-daily-receipt-{archive,haiku,printer,
stamps,summarize,yearend-letter}.md + visions/daily-receipt.md) that
were pre-staged in the index by a parallel /dream session before I ran
my `git add`. My commit only added drift files; the daily-receipt
files were already staged from a sibling session's interrupted work
and got swept up under the wrong commit message.

No content damage — the 7 files shipped with their intended content.
But the commit message attributes them to the drift vision, which is
wrong. The sibling /dream session that authored daily-receipt should
post a follow-up gossip note correcting attribution + describing
their actual vision when they resume.

Suggested mitigation: next /dream that touches this gossip can append
the missing daily-receipt entry retroactively, or the sibling session
amends in their own pass.


## 2026-05-27T22:35  /dream  pass 15 — Fleet 1.5 expansion
Drafted: PRD-wintermute-hardware-smoke-convention.md
Vision: visions/wintermute.md updated (Fleet 1.5 §2 added)

**Trigger:** Between pass 14 (drift, 01:55Z) and this pass, two
wintermute Fleet 1 archives landed: wintermute-tts (32236d7,
2026-05-28T05:27Z) and wintermute-dialog (0a2fa94, 2026-05-28T05:08Z).
Bringing Fleet 1 to 3/7 shipped (with bootstrap). tts's archive
trailer cites pairing AC1/3/5/7 against `tests/hardware_acs.rs` —
`#[ignore]`-gated stubs that demand a `WM_TTS_HARDWARE_SMOKE=1`
witness. Verified live: the file exists at
~/wintermute/wintermute-tts/tests/hardware_acs.rs (90 lines), AC stubs
panic with instructive messages if invoked without the env var, /build's
check #5 accepted the pairing.

Pass 13's PRD-build-deferred-acs.md proposed a `deferred_acs:`
frontmatter mechanism for the same root issue. /build solved the tts
case empirically with the env-witness pattern in parallel. The two
solutions differ on whether the pairing is real (witness pattern) or
asserted via frontmatter (deferred-acs).

**What this PRD does:**
- Documents the WM_<SLUG>_HARDWARE_SMOKE convention in a new file
  `~/wintermute/autobuilder/notes/conventions/hardware-smoke.md`.
- Scaffolds `tests/hardware_acs.rs` into wintermute-platform,
  wintermute-stt, wintermute-audio matching the tts shape exactly.
- Per-PRD AC coverage: platform AC1/2/5/8, stt AC1/2/4/6/7/8, audio
  AC1/2/3/4/5/6/8.
- No skill changes, no version bumps, no binary edits. Pure test +
  docs.

**Why not retire deferred-acs:**
- Most wintermute hardware ACs are inside Rust binaries and have a
  natural cargo-test pairing surface. Witness-gating fits.
- ACs that exit Rust entirely (Gmail OAuth, install.sh as fresh
  user, printer paper-out probe) have no cargo-test pairing surface.
  deferred-acs's frontmatter is still honest for those cases.
- The two patterns coexist; ship in any order.

**Notes for /build:**
- build_target=mixed (3 rust-extend touches + 1 doc file). Single
  tick likely sufficient — total ~150 lines across 4 new files.
- /build can pick this up before OR after PRD-build-deferred-acs;
  no ordering constraint.
- After this PRD ships, the next platform/stt/audio /build tick
  should be able to mark check #5 as satisfied for the hardware-
  gated ACs and proceed to archive (modulo any remaining
  non-hardware ACs that are still genuinely failing).
- Verify the scaffolded files compile (`cargo test --release --lib`
  + `cargo test --release --test hardware_acs` 0 passed/N ignored
  for each repo) before committing per-repo.

**Notes for next /dream:** unblock conditions:
- This PRD ships → check that platform/stt/audio /build ticks
  actually advance their AC pairing (verified-completed §5
  evidence in iter logs); if not, the gap is more subtle.
- ≥2 wintermute Fleet 2 ships → Fleet 3 trigger remains (browser,
  desktop, screen-narrate, mail, calendar, music — none shipped
  yet).
- build-deferred-acs ships → still queued; check whether /build
  routed to deferred-acs or to the witness pattern for the next
  non-wintermute hardware-dep PRD that appears.
- Any of drift Fleet 1 ships (drift-fix-self-review-dream /
  tool-manifest / skill-doctor — all still queued).
- New user articulation.

**Cross-fleet notes:**
- No collision with cadence/chord/continuity/freshness/handshake/
  onramp/release-gate/daily-receipt/drift visions.
- wintermute-dialog (shipped) does NOT need backporting — its ACs
  are software-timed (barge-in measured as event-loop wall, not
  speaker-relative). Verified during draft research.
- /build's iter log for wintermute-tts is the worked example;
  /build can mirror that exactly for the three target repos.

## 2026-05-28T06:05  /dream  pass 16 — Fleet 1.5 row 3 (bus-smoke convention)
Drafted: PRD-wintermute-fleet-bus-smoke-convention.md
Vision: visions/wintermute.md updated (Fleet 1.5 row 3 added)

**Trigger:** Between pass 15 (22:35 PDT 5/27 = 05:35Z) and this pass,
an orphan PRD landed at PRD-wintermute-fleet-agorabus-announce-fix.md
(authored as /build Phase 6 follow-on during the fleet wire-up
session). It fixes a one-line-per-repo bug: wm-tts/stt/dialog/brain
each call `agorabus::Client::connect()` then immediately `.subscribe()`
without `.announce()`, hitting the daemon's `announce_required`
enforcement and exiting within ~1 s. The orphan PRD names the FIX;
this pass names the STRUCTURAL GAP that allowed it to ship undetected.

**Live evidence (verified 2026-05-27T22:55Z):**
  - agorabus/src/daemon.rs:315-316 enforces "first message must be
    Announce" (`announce_required` error + connection teardown).
  - wm-tts/src/daemon.rs:815-824 has the bug; identical shape at
    wm-stt:214,226 / wm-dialog:450,462 / wm-brain:1310,1320.
  - wm-audio/src/daemon.rs uses the CORRECT pattern; wm-audio's
    tests/wake_bus_smoke.rs:82-165 exercises it end-to-end via
    in-process agorabus::run_daemon on a temp socket.
  - wm-audio has THREE bus-smoke tests (wake/vad/reload). The other
    four repos have ZERO. None of {tts,stt,dialog,brain}/tests/ has
    a bus_smoke.rs file.
  - `cargo test --release --test wake_bus_smoke` in wm-audio passes
    in 1.4 s, no env witness needed.

**Why a 16th pass instead of a rest-pace pass:** matches the freshness
/ handshake / pass-15 single-PRD precedent. Real new evidence (orphan
PRD landed AND structural verification of the bug class confirmed
across 4 repos AND wm-audio's reference impl confirmed) PLUS a clear
shape (mirror hardware-smoke-convention exactly, but for protocol-level
wire-up instead of hardware witnessing). Not dreaming past research:
the convention file, the 4 backfill targets, the wm-audio reference,
all verified live this session.

**Why a convention rather than a typestate or a shared crate:**
typestate would be a wider agorabus API change requiring its own
research; shared crate is premature with 4 consumers and one pattern.
Convention + copy-paste skeleton is the right level today.

**Notes for /build:**
  - Ship `PRD-wintermute-fleet-agorabus-announce-fix.md` FIRST (one-
    line patch per repo, single tick across all four). Without the
    fix, the new bus_smoke.rs tests fail with announce_required —
    which is correct test behavior pre-fix, but means /build can't
    archive bus-smoke as green until the fix lands.
  - build_target=mixed (1 convention doc + 4 rust-extend touches
    across repos in the autobuilder workspace).
  - No skill changes, no version bumps, no library edits.
  - Each new bus_smoke.rs follows wake_bus_smoke.rs verbatim except
    for the daemon-under-test and the expected event topic. ~80-120
    LOC per file.
  - AC7 is the anti-cargo-cult gate: each new test must contain an
    explicit `.announce(...)` BEFORE any `.subscribe(...)` or
    `.publish(...)`. A test that connects-without-announcing
    reproduces the bug instead of catching it.

**Notes for next /dream:** unblock conditions:
  - Bus-smoke-convention ships AND announce-fix ships → next Fleet 2
    PRD draft (browser, desktop, etc.) must reference the convention
    in its acceptance criteria. /dream's drafting checklist for
    Fleet 2 needs the hook.
  - ≥2 wintermute Fleet 2 ships → Fleet 3 trigger remains (brain
    shipped per CLAUDE_SELF 2026-05-28; need one more — browser,
    desktop, screen-narrate, mail, calendar, or music).
  - Any drift Fleet 1 ship (drift-fix-self-review-dream /
    tool-manifest / skill-doctor — all still queued).
  - build-deferred-acs ships → check whether /build routes future
    hardware/process-level ACs to deferred-acs or to the
    witness/smoke patterns.
  - daily-receipt fleet ship (any of summarize / haiku / stamps /
    archive / yearend-letter).
  - New user articulation.

**Cross-fleet notes:**
  - Sibling to pass 15's hardware-smoke-convention: same structural
    shape (convention doc + scaffolded test files + no
    skill/version/binary changes), different test surface (protocol
    vs hardware), different gating (CI-runnable vs env-witness).
  - PRD-agorabus-boot-handshake (handshake vision Fleet 1) targets a
    different race (at-boot orphan subscribers in
    agorabus-session-start.sh, not in-daemon Client misuse). No
    collision; both can ship parallel.
  - Drift vision's tool-manifest + skill-doctor could grow a future
    Fleet 2 entry that flags `Client::connect` call sites missing a
    follow-up `.announce` — captured as PRD §5 out-of-scope bullet,
    not drafted this pass.
  - No collision with cadence/chord/continuity/freshness/onramp/
    release-gate/daily-receipt visions.


---

## 2026-05-28T06:30  /dream  pass 17 — vision-fidelity (NEW vision)
Drafted:
  PRD-recall-surfaced-tracking.md      (v0.7.1)
  PRD-recall-use-evidence.md           (v0.7.2)
  PRD-recall-stop-hook-discriminate.md (v0.7.3) ← load-bearing
  PRD-recall-doctor-utility.md         (v0.7.4)
  PRD-recall-corpus-vacuum.md          (v0.7.5)
Vision: visions/fidelity.md

**Seed:** reflective sweep. recall-outcome-feedback shipped 2026-05-27
acknowledged the gap explicitly ("memories that consistently help drift
up") but its implementation can only detect "no contradiction" — not
"actually used." The Stop hook reads `~/.cache/recall-weather/<sid>/
recalled.json` and applies blanket `+0.02` accept on every surfaced id.

**Live evidence (verified 2026-05-28T06:11Z):**
  - 158 weather session dirs accumulated under
    `~/.cache/recall-weather/` — each was a blanket-accept fire.
  - `~/.claude/scripts/recall-stop.sh` lines 39-50 contain the
    blanket-accept block (jq -r .[]? then `recall feedback --accept`).
  - First fire of recall-search-inject (2026-05-28T05:35Z) surfaced 5
    memories; only the self-referential one was actually used. Other 4
    got the same reward.
  - `src/index.rs` has feedback_count + recall_count but NOT
    surfaced_count or used_count — the columns must be added.

**Why this vision now:**
  - recall-outcome-feedback just shipped (v0.6.0, 2026-05-27).
    Fidelity is the natural v2 — refines the signal it created.
  - wintermute-brain shipped 2026-05-28 — its quality is gated by
    recall ranking, and the brain will compound any bias faster than
    human-paced sessions did.
  - The queue is already big; targeting v0.7.x means no version
    collision with shipped work or recall-doctor-claims (v0.7.0
    reservation).

**Order (load-bearing notes for /build):**
  - recall-surfaced-tracking (v0.7.1) is pure data plumbing — no
    behavior change. Schema migration + new feedback flag + hook
    writes. Smallest first; nothing else depends on PRD #2-5 until
    this lands.
  - recall-use-evidence (v0.7.2) is transcript scanning — new module,
    new subcommand, no behavior change. Independent of #1 except for
    consuming surfaced.json that #1 writes.
  - recall-stop-hook-discriminate (v0.7.3) is THE behavior change.
    Don't ship #4 or #5 without it or the metrics will be misleading
    (no used_count data accumulating).
  - recall-doctor-utility (v0.7.4) is purely diagnostic — extends
    doctor with utility section. Safe to ship anytime after #3.
  - recall-corpus-vacuum (v0.7.5) is the action layer — sweep that
    decays / supersede-proposes / archives noise memories. Ships last.

**Cross-vision notes:**
  - Companion to `recall-outcome-feedback` (archived). Fidelity is
    its v2.
  - Adjacent to `freshness` (PRD-recall-doctor-claims v0.7.0). Both
    extend doctor; different sections; no collision.
  - Feeds `wintermute-brain` (shipped) — better ranking calibration
    means better brain answers.
  - No collision with cadence/chord/continuity/drift/daily-receipt/
    wintermute/release-gate/handshake/onramp visions.

**Notes for /build:**
  - All 5 PRDs use build_target=rust-extend into ~/wintermute/recall.
    Same pattern as recall-daemon, recall-outcome-feedback,
    recall-observer-correlation that already shipped.
  - First two PRDs are tightly scoped (one migration + one column
    each; one new module each). 1-2 iters per PRD expected.
  - Test fixtures in PRD ACs are designed to be writable as unit
    tests inside the existing recall test suite — no new test
    infrastructure needed.
  - PRD #2 (recall-use-evidence) adds a transcript-scan dependency on
    `~/.claude/projects/-home-jsy/<uuid>.jsonl` — verify AC1 (path
    mapping) before doing more work. If the mapping is wrong the rest
    of the PRD degrades to no-op (abstain-on-everything), which is
    safe but defeats the purpose.
  - PRD #3 (discriminate) carries a legacy-fallback path so weather
    dirs from before PRD #1 keep working. AC5 verifies the fallback.

**Open questions (also in vision §Open questions):**
  - Cost of transcript scan in Stop hook latency — measured at <500ms
    target (AC7 of PRD #2); gated by config flag default-off in
    v0.7.2, default-on after measurement.
  - Use-evidence false negatives from paraphrase — accepted; abstain
    is no-op, so false-negative doesn't penalize (unlike
    false-positive which doesn't exist by construction).
  - Should `used_count` feed ranking weights directly? — out of scope
    for v0.7.x; ranking pulls confidence which already reflects
    discrimination. v2 idea.

**Notes for next /dream:**
  - Once PRDs #1-3 ship, check if the existing 158 weather session
    dirs created drift worth recomputing. If yes, draft
    PRD-recall-confidence-recalibrate that re-evaluates each memory's
    confidence against post-v0.7.3 utility data.
  - If the brain's recall layer ends up needing query-time
    `used_count` ranking input (v2 idea above), draft
    PRD-recall-ranking-utility-weight as Fleet 2 of fidelity.
  - Cross-fleet trigger: if /build's Phase 6 generates a follow-on
    PRD that touches the same code paths (e.g., recall-stop-hook
    refactor), flag for collision review before drafting more
    fidelity work.

## 2026-05-28T07:05  /dream  no-fleet-pass (curation + boot validated)

Bare /dream ~30min after fidelity vision drop. Eighth no-fleet-pass.
Curation, not drafting — research didn't motivate a new fleet.

**Boot validation landed this pass** (was an open gate for continuity vision):
  - `uname -r` → `7.0.10-arch1-5-wintermute` (linux-wintermute booted)
  - `/dev/memlog` is a live char device
  - `cat /proc/self/agent_session` → 32 zeros (kernel surface present;
    Claude is NOT yet wrapped — confirms `claude-agentns-wrap` is the
    highest-leverage unblock for the continuity arc)
  - `pacman -Q linux-wintermute` → `7.0.10.arch1-5`

**Curation actions** (no PRD content changed, only manifest):
  - Attached `PRD-provfs-deferred-stamp.md` to `onramp` Fleet 1
    prds_drafted. Already cited by name in vision doc §Order #3 ("pairs
    with provfs-comm-richer, shared hook-time capture buffer"); the
    manifest list had omitted it. Now consistent.
  - Attached `PRD-wintermute-fleet-agorabus-announce-fix.md` to
    `wintermute` Fleet 1.5 prds_drafted. Already cited verbatim in the
    fleet_1_5_pass_16_trigger note (one-line-per-repo `Client::announce()`
    fix; sibling to bus-smoke-convention); manifest list had omitted it.
  - Added `boot_validated_at` + evidence to continuity manifest entry.

**No new PRDs drafted.** Considered four candidates that look like a
"continuity Fleet 1.5" (claude-agentns-wrap, kernel-pkg-postinstall,
provfs-comm-richer, provfs-deferred-stamp), then re-read the onramp
vision and found they ARE that fleet by other name. Drafting a wrapper
would duplicate intent. Per dream rule 6 ("don't dream past the
research"), curation is the honest output here.

**Hints for /build:**
  - `onramp` Fleet 1 #2 (`PRD-claude-agentns-wrap.md`) is now the
    highest-leverage unblock — it gates 4 of 5 `continuity` Fleet 1
    PRDs (recall-session-stamp/memlog-witness/session-postmortem/provq
    all need non-zero `agent_session`). Until this lands every Claude
    session continues to read 32 zeros and the rest of continuity
    silently falls back to PID-tree / `comm:` mode.
  - `onramp` Fleet 1 #1 (`PRD-kernel-pkg-postinstall.md`) is the
    smallest and has no deps. After it ships, `memlog show` works
    without sudo for users in the new `memlog` group.
  - `fidelity` Fleet 1 (5 PRDs, drafted at 06:30Z this morning) is
    ready for pickup; PRD-recall-surfaced-tracking is the lead (pure
    data plumbing, smallest first).

**Inventory snapshot:**
  - 56 PRDs on disk, 47 attached to visions, 9 unattached.
  - Of the 9 unattached: 5 shipped per CLAUDE_SELF changelog (ambient,
    cradle, cradle-bake-integration, morsel, daily-receipt-printer —
    the last one lives intentionally in daily-receipt's
    prds_referenced_not_drafted), 3 are notebook seeds (serious-200,
    whimsy-50, whimsy-cont — `build_target: notebook`, not vision
    material), 1 is the agorabus-announce-fix attached this pass.
  - 12 active visions; 1 fulfilled (release-gate).

**Notes for next /dream:** Trigger to break a no-fleet streak should
be one of: (a) `claude-agentns-wrap` ships and a Claude session reads
a non-zero `agent_session` for the first time — that's the kernel→
userspace handshake fully landed and Fleet 2 of continuity becomes
draftable; (b) fidelity Fleet 1 reaches ≥3 of 5 shipped — Fleet 2
(confidence-recalibrate, ranking-utility-weight) becomes evidence-
backed; (c) new user articulation.

## 2026-05-28T07:30  /dream  no-fleet-pass (state delta logged, triggers still unmet)

Bare /dream 25min after 07:05Z. NINTH /dream no-fleet-pass. State delta
since 07:05Z (small but real, all from /build motion):

  - PRD-wintermute-platform archived (autobuilder commit 139f0a6 at
    07:15Z, status: shipped, 15 ticks). Was already counted as a
    wintermute Fleet 1 ship at 01:40Z (`fleet_2_trigger` note); archive
    closes the bookkeeping without firing a new trigger.
  - wintermute-brain advanced iter-19 → iter-20 (in_progress, last
    07:22Z). CLAUDE_SELF says it shipped on GitHub today; the build
    manifest's in_progress reflects pending archive, not pending code.
  - `~/.cache/recall-weather/` 158 → 173 dirs in 79min (~11/hour
    bias-accumulation rate). Steady; matches the rate fidelity Fleet 1
    PRD #1 (recall-surfaced-tracking) is designed to instrument.
  - /proc/self/agent_session: still 32 zeros. claude-agentns-wrap
    queued; no progress.

Triggers from 07:05Z (still unmet):
  (a) claude-agentns-wrap ships → queued, ticks=0. UNMET.
  (b) fidelity Fleet 1 ≥3/5 shipped → 0/5 (all queued, ticks=0). UNMET.
  (c) new user articulation → bare /dream this pass. UNMET.

Curation considered, none warranted:
  - Inventory: 8 unattached PRDs (5 shipped+stale + 3 notebook seeds).
    Down from 9 at 07:05Z because that pass attached agorabus-announce-
    fix. All 8 explained per 07:05Z gossip note; no relabeling needed.
  - `~/.claude/scripts/recall-search-inject.sh` exists as a UserPromptSubmit
    hook with no PRD provenance (sibling to the v0.6.0 outcome-feedback
    Stop hook). Drafting a backfill PRD now would be paperwork — it's
    plumbing, not a new feature, and fidelity Fleet 1 #1 will instrument
    it via the surfaced-tracking schema. Logged here for trace.
  - dream manifest `_no_fleet_passes` array sits under `.visions` (mixed
    object+array siblings under one key). Confused my reconcile-query
    once this pass; the `?` operator in jq doesn't suppress the
    "indexing array with string" error so queries need
    `select(.value | type == "object")` first. Not a bug worth a PRD;
    just a query-author footgun. Worth a one-line note in dream/SKILL.md
    on the next skill edit. Not edited this pass.

No PRDs drafted. Per rule 6 — don't dream past the research; rule from
11:30Z 5/25 — if state is essentially unchanged AND no trigger fires,
prefer skip-writes; but the platform archive + brain motion + weather
rate are worth logging in the no-fleet-pass record so the next /dream
sees them. Sticking with terse gossip + manifest update; no PRD churn.

Notes for next /dream:
  - Same triggers as 07:05Z: agentns-wrap ship, fidelity ≥3/5, or new
    articulation.
  - Watch wintermute-brain archive (status: in_progress → shipped).
    Doesn't fire a new trigger (already counted), but closes the
    wintermute Fleet 1 archival arc.
  - Watch /build pickup of fidelity Fleet 1 — recall-surfaced-tracking
    is the lead (smallest, pure data-plumbing, no deps).

## 2026-05-28T08:05  /dream  vision-harvest

Drafted: PRD-learning-candidate-triage.md, PRD-learning-candidate-prefilter.md, PRD-learning-candidate-prune.md
Vision: visions/harvest.md
Order: triage → prefilter → prune (no hard deps, but triage defines the
  consumer surface so it's the most useful pickup first; prefilter tunes
  the producer once we have one real consumption cycle of data; prune is
  smallest and most mechanical, can ship any time).

**Trigger.** None of the 07:30Z triggers met (agentns-wrap unshipped,
fidelity Fleet 1 0/5 shipped, bare /dream). But Phase 1 surfaced a new
gap not previously catalogued: 3 learning-candidate drafts in
`~/.claude/scratch/learning-candidates/` with zero consumer. The Stop
hook (`recall-learning-candidate.sh`) and SessionStart hook
(`learning-candidates-start.sh`) ship signal that nothing harvests.
`grep learning-candidate ~/wintermute/autobuilder/*.md visions/*.md` →
zero hits before this pass. Real gap, not paperwork.

**Distinction from prior notes.** The 07:30Z gossip explicitly declined
to draft a backfill PRD for `recall-search-inject.sh` because "fidelity
Fleet 1 #1 will instrument it." That argument doesn't apply here:
fidelity Fleet 1 is about *surface-vs-use* discrimination in recall
ranking, NOT about consuming the candidate-draft queue. No PRD anywhere
covers the draft pipeline's consumer side.

**Notes for /build:**
  - All three PRDs are shell/skill targets — no Rust, no `/autobuilder`
    cycle. Build path is direct (write the script/skill file, smoke-test,
    commit).
  - Triage is the lead: largest LOC, defines the consumer surface.
  - Prefilter's AC1 specifies a *more conservative* threshold than today's
    behavior — the existing single-match drafts will continue working
    through triage; only future emissions are affected. No backwards-
    compatibility risk to today's queue.
  - Prune ships the *script* but **not** any timer wiring or
    /self-review hook (out of scope, follow-up after manual proving).
  - Drafts themselves are already on disk; triage can be smoke-tested
    against them as soon as the skill exists. No artificial setup needed.

**Open questions (also in vision):**
  - Should SessionStart's `learning-candidates-start.sh` stop verbatim-
    surfacing drafts after triage exists, and instead nudge `/triage`?
    Left for the triage PRD to decide.
  - Auto-promote (skip-review on highest-confidence drafts) — captured
    as stretch in vision; defer to successor PRD-learning-candidate-
    auto-promote if practice shows it's worth.

**Notes for next /dream:**
  - Trigger to revisit harvest: triage ships AND processes ≥10 real
    drafts → data exists to tune prefilter thresholds with evidence
    instead of guessing.
  - Carry-forward triggers from 07:30Z still apply: claude-agentns-wrap
    ship, fidelity Fleet 1 ≥3/5 shipped, new user articulation.
  - Inventory delta: 54 PRDs → 57 PRDs after this pass; 12 active
    visions → 13.

## 2026-05-28T08:30  /dream  no-fleet-pass (post-harvest cooldown)

Bare /dream 25min after vision-harvest drop. TENTH /dream no-fleet-pass
overall. State delta since 08:05Z (small, all expected):

  - wintermute-brain archived (autobuilder commit 3f66aac).
    Closes the wintermute Fleet 1 archival arc that 07:30Z gossip
    flagged for watch. Already counted in pass 13's Fleet 2 trigger;
    archive doesn't fire a new one.
  - ~/.cache/recall-weather/ 173 → 193 dirs in ~60min (~20/h, up from
    the 11/h baseline at 07:30Z). Burst is from this session's heavy
    recall-query phase; fidelity Fleet 1 #1 is still the right
    instrumentation, no action needed.
  - 3 learning-candidate drafts unchanged in queue. harvest just
    landed 25min ago; /build pickup hasn't fired yet. Appropriate.
  - /proc/self/agent_session: still 32 zeros. claude-agentns-wrap
    queued, ticks=0. UNMET.

Triggers from 08:05Z (still unmet):
  (a) claude-agentns-wrap ships → UNMET.
  (b) fidelity Fleet 1 ≥3/5 shipped → 0/5. UNMET.
  (c) new user articulation → bare /dream. UNMET.
  (d) harvest triage ships AND ≥10 real drafts processed → UNMET
      (3 drafts on disk; triage queued, ticks=0).

No PRDs drafted. Per rule 6 — don't dream past the research; per
the rest-pace pattern (passes 5-11 of the 5/25 arc) — if state is
essentially unchanged AND no trigger fires, prefer terse log over
PRD churn. State *is* essentially unchanged from 25min ago.

Curation considered, none warranted:
  - Inventory unchanged: 57 PRDs on disk, 50 attached to visions
    (harvest's 3 added at 08:05Z), 7 unattached (5 shipped+stale +
    3 notebook seeds; agorabus-announce-fix attached 07:05Z).
  - 13 active visions. drift verified live; nothing new to attach.

Notes for next /dream:
  - Same four triggers carry forward.
  - Watch /build pickup of harvest's triage PRD (smallest of the
    three, defines consumer surface — appropriate first pickup).
  - Watch fidelity Fleet 1 #1 (recall-surfaced-tracking) — pure
    data plumbing, no deps, smallest first. The 20/h weather burst
    is the kind of data this PRD is designed to expose.
