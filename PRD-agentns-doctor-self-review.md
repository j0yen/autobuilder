# PRD: agentns-doctor-self-review — stop self-review mis-flagging a healthy kernel

**Author:** Claude (Opus 4.8), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** [visions/signet.md](visions/signet.md)
**Depends on:** [PRD-agentns-doctor.md](PRD-agentns-doctor.md) shipped + installed at `~/.local/bin/agentns-doctor`
**build_target:** shell
**build_into:** `/home/jsy/.claude/skills/self-review`

---

## TL;DR

`/self-review` has reported `agentns` `/proc/self/agent_session`
all-zeros as a broken kernel asset for ~20 consecutive runs. The
check in `self-review/SKILL.md:123-124` only recognizes two states
("present" or "empty / file missing → registration failed") and has
no branch for *present-but-all-zeros = init namespace, unwrapped,
expected*. This PRD rewrites that B.5 block to call
`agentns-doctor status --format json` and journal the tri-state
verdict, so a healthy unwrapped kernel reads as `init (expected)`
instead of "broken," and only a genuinely `absent` surface on a
`-wintermute` kernel or a `malformed` reading becomes a Pending line.

Because editing a skill file is classifier-gated self-modification,
this ships as `proposals/self-review-agentns-block.draft.md` (a drop-in
replacement for the B.5 agentns bullet) plus a one-line wiring note,
**not** a live edit of `SKILL.md`. Same precedent as the
agorabus-boot-handshake and ctrace-session-end-resilient drafts.

---

## 1. Why this exists

### 1.1 The recurring misdiagnosis is the bug

Twenty self-review runs have flagged the same non-problem. Evidence:

- recall reflective `01KSS21WFN5H6V42JF723Z8K2J` (run 19, 2026-05-28):
  *"agentns all-zeros ~20th run."*
- recall reflective `01KSRV7R4FERPP40HQGV5RGZNT` (run 18): *"agentns
  agent_session all-zeros (19th)."*
- journal 2026-05-28 run 19 Pending: *"agentns: `/proc/self/agent_session`
  all-zeros again (18th run). Kernel-side, outside skill scope."*
- runs 13-15 (journal 2026-05-26) proposed editing
  `agorabus-session-start.sh` to unshare — structurally impossible per
  `PRD-claude-agentns-wrap.md` §1.2.

The kernel is healthy (see `PRD-agentns-doctor.md` §1.1 live probe).
The skill keeps mis-reading it because the check can't name the
`init` state.

### 1.2 The exact line to replace

`~/.claude/skills/self-review/SKILL.md:123-124` (verbatim):

> **agentns**: `[ -f /proc/self/agent_session ]` and `cat
> /proc/self/agent_session`. If empty / file missing, the namespace
> registration failed. Recent reaper kills: `dmesg -t | grep …`

The `[ -f ]`-and-`cat` test produces `00000000000000000000000000000000`,
which is neither empty nor missing, so the prose verdict ("registration
failed") is reached *by the human reading zeros*, not by the test.
`agentns-doctor status` replaces the ambiguous reading with a named
state.

---

## 2. What this builds

### 2.1 The replacement B.5 block (drafted as a proposal)

`proposals/self-review-agentns-block.draft.md` — a drop-in replacement
for the agentns bullet in SKILL.md Phase B / B.5. New logic:

```bash
# agentns — tri-state via agentns-doctor (signet vision)
if command -v agentns-doctor >/dev/null 2>&1; then
  ans=$(agentns-doctor status --format json 2>/dev/null)
  state=$(printf '%s' "$ans" | jq -r '.state')
  verdict=$(printf '%s' "$ans" | jq -r '.verdict')
  case "$state" in
    init)  : ;;  # unwrapped, EXPECTED — do NOT flag; one calm line in journal
    live)  : ;;  # wrapped — record sid + intent_tag, healthy
    absent)
      # only a fault on a -wintermute kernel
      uname -r | grep -q wintermute && echo "PENDING: agentns surface absent on -wintermute kernel"
      ;;
    malformed) echo "PENDING: agentns surface malformed: $ans" ;;
  esac
else
  # doctor not installed yet: cat, but interpret correctly
  s=$(cat /proc/self/agent_session 2>/dev/null)
  if [ -z "$s" ]; then echo "agentns surface absent (stock kernel or pre-boot)"
  elif printf '%s' "$s" | grep -qE '^0+$'; then
    echo "agentns: init ns (unwrapped) — EXPECTED until claude-agentns-wrap routes launches; NOT a kernel fault"
  else echo "agentns: live session $s"; fi
fi
```

The key behavioral change: **an `init` reading is never a Pending
line.** It is, at most, one calm journal line ("agentns: init ns,
unwrapped — expected"). Only `absent` on a `-wintermute` kernel or a
`malformed` surface escalates.

### 2.2 Journal phrasing fix

The draft also supplies the replacement Notable/Pending phrasing so
future reflective notes stop carrying "lone broken kernel asset."
Suggested standing line for the `init` case:

> agentns: init ns (unwrapped), expected — non-zero session id arrives
> when `claude-agentns-wrap` routes launches through `agentns-claude`.
> Not a fault; not carried as Pending.

### 2.3 What this does NOT do

- Does not live-edit `SKILL.md` (classifier-gated; user swaps the
  draft in when satisfied).
- Does not build the doctor (`PRD-agentns-doctor.md`).
- Does not route launches through the wrapper
  (`PRD-claude-agentns-wrap.md`).
- Does not change the memlog/provfs/dmesg checks in the same B.5
  region — only the agentns bullet.

---

## 3. Acceptance criteria

1. **Draft exists and is shell-valid.** `proposals/self-review-agentns-block.draft.md`
   contains a fenced bash block that passes `bash -n`.
2. **`init` is not flagged.** Running the draft's logic in this session
   (init ns) produces no `PENDING:` line; at most one calm
   "init ns (unwrapped) — expected" line.
3. **`absent` on `-wintermute` flags; on stock does not.** Simulated by
   stubbing `agentns-doctor` to emit `{"state":"absent",...}` and
   stubbing `uname -r`: a `-wintermute` uname yields a `PENDING:` line;
   a stock uname yields none.
4. **`malformed` flags.** Stubbed `{"state":"malformed",...}` yields a
   `PENDING:` line quoting the JSON.
5. **`live` records, does not flag.** Stubbed `{"state":"live",
   "session_id":"<32hex>","intent_tag":"/build"}` yields a healthy
   journal line with the sid + intent, no Pending.
6. **Degrades without the doctor.** With `agentns-doctor` absent from
   PATH, the fallback branch runs: init ns yields the corrected
   "expected, NOT a kernel fault" line — never "registration failed."
7. **The string "registration failed" never appears** in the draft's
   output for any of the four states (the explicit anti-regression for
   the 20-run misdiagnosis).
8. **Wiring note present.** The draft includes a header comment naming
   the exact SKILL.md lines it replaces (123-124) and the swap command
   (`# replace the agentns bullet in Phase B/B.5 with this block`).
9. **Verified live by jsy** by swapping the draft into SKILL.md and
   running one `/self-review`, confirming the agentns line reads
   `init (expected)` and no Pending is emitted. (Self-mod is
   user-gated — mechanical AC1-8 are testable without the swap.)

ACs 1-8 are today-testable against the draft + stubs; AC9 is the
user-gated live swap.

## 4. Bootstrap notes

- `shell` target into `~/.claude/skills/self-review`; ships as a
  `proposals/*.draft.md`, never a live SKILL.md edit (classifier-gated).
- Degrades safely if `PRD-agentns-doctor.md` hasn't shipped: the
  fallback `cat` branch still corrects the interpretation, so this PRD
  can land in either order relative to the doctor.
- The standing journal phrasing (§2.2) is the durable fix — it stops
  the misdiagnosis propagating into recall reflective notes even before
  the doctor exists.
