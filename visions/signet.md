# Vision: signet — make the agent-session signet real, readable, trusted

**Authored by:** /dream (Claude Opus 4.8), with jsy
**Created:** 2026-05-29
**Status:** active
**Fleet 1 drafted:** 3 PRDs (tri-state doctor + self-review wiring + session receipt)
**Sibling of:** [onramp](onramp.md) (which builds the *wrapper*; signet builds the *reading* of it)

---

## TL;DR

The wintermute kernel gives every task a 128-bit `agent_session_id` —
a signet that stamps provfs xattrs, can key recall memories, and can
demux memlog records. The kernel works: `CONFIG_AGENT_NS=y`,
`/proc/self/ns/agent` resolves (inode `4026531996` this session),
`/proc/self/agent_counters` is live. **But nothing reads the signet
correctly.** Every Claude session lives in the *init* agent namespace
because nothing has called `unshare(CLONE_NEWAGENT)` on the launch
path yet — so the session id is 32 zeros and the counters are all
zero. That is the *correct, expected* reading of an unwrapped process.

Self-review has mis-read it as a kernel fault for **~20 consecutive
runs** ("agentns all-zeros — lone broken kernel asset"). The SKILL.md
check (`self-review/SKILL.md:123-124`) only knows two states: file
present, or "empty / file missing → registration failed." It has no
case for *present-but-all-zeros = init ns, unwrapped, fine* — so a
healthy kernel keeps getting flagged as broken, and the proposed
"fixes" in runs 13-15 (edit the hook to unshare) are structurally
impossible (`PRD-claude-agentns-wrap.md` §1.2 proved this).

signet is the *reading* layer. It does not wrap (onramp's
`claude-agentns-wrap` does that). It builds the diagnostic that tells
a healthy-but-unwrapped kernel apart from a broken one, wires that
truth into self-review so the misdiagnosis stops, and turns the
per-ns counters into a per-session resource receipt once a session
*is* wrapped.

## End-state

When signet Fleet 1 ships:

1. **`agentns-doctor status` names exactly which of three states you
   are in** — `absent` (stock kernel, no CLONE_NEWAGENT),
   `init` (kernel present, unwrapped — *expected*, not a fault), or
   `live` (wrapped: prints the 32-hex session id, intent_tag, and
   counters). No tool conflates these today.
2. **Self-review stops mis-flagging.** Phase B.5 calls the doctor and
   journals the tri-state verdict. "agentns all-zeros = broken" never
   appears again; an `init` reading is reported as "unwrapped
   (expected until `claude-agentns-wrap` lands)," and only `absent`
   on a `-wintermute` kernel or a genuinely malformed surface is a
   Pending line.
3. **A wrapped session yields a resource receipt.** When launches are
   routed through `agentns-claude` (onramp), `agentns-doctor receipt`
   reads the 7 per-ns counters (syscalls / openat / write_bytes /
   connect / unlink / fork / elapsed_ns) — which **no userspace tool
   reads today** — and emits a per-session JSON ledger joinable with
   ctrace session records and recall's session stamp.

## Components (Fleet 1)

- **PRD-agentns-doctor.md** — `rust-cli`, new repo
  `~/wintermute/agentns-doctor` → `j0yen/agentns-doctor`. The tri-state
  diagnostic: `status`, `explain`, `counters`. Reads `/proc/self/ns/agent`
  inode + `/proc/self/agent_session` + `/proc/self/agent_counters`,
  classifies absent|init|live. This is the diagnostic
  `PRD-claude-agentns-wrap.md` §Out-of-scope explicitly deferred ("A
  claude-doctor CLI to check namespace status from outside").
- **PRD-agentns-doctor-self-review.md** — `shell`. Rewrite the
  self-review B.5 agentns block to call the doctor and journal the
  tri-state, killing the 20-run misdiagnosis. Ships as a `.draft`
  under `proposals/` (skill self-mod is classifier-gated; same
  precedent as the agorabus-boot-handshake and ctrace-session-end
  drafts). Degrades safely if the doctor isn't on PATH.
- **PRD-agentns-session-receipt.md** — `rust-extend` into
  `agentns-doctor`. `receipt --emit` snapshots the per-ns counters at
  session end (or on demand) into a JSON ledger keyed by
  `agent_session_id` + `intent_tag`, joinable with ctrace + recall.

## Order

```
agentns-doctor
   ├──► agentns-doctor-self-review   (shells out to the doctor)
   └──► agentns-session-receipt      (rust-extend of the doctor)
```

`agentns-doctor` is the root — ship it first. The other two both build
on it. `-self-review` degrades safely if the doctor isn't installed
yet (keeps the current `cat`, but with corrected interpretation text),
so it can scaffold ahead.

## Relationship to other visions

- **onramp builds the wrapper; signet reads it.** `claude-agentns-wrap`
  (onramp Fleet 1) routes launches through `agentns-claude` so the
  signet becomes *non-zero*. signet is the layer that *reads and trusts*
  the signet whether zero or not. They are complementary: until
  `claude-agentns-wrap` lands, the doctor's honest verdict is `init`
  for every session; after it lands, `live`. Neither blocks the other —
  the doctor is useful *now* precisely because it explains why today's
  sessions read zero.
- onramp Fleet 2 has an `onramp-doctor` bullet that "runs all three
  checks" (memlog + provfs + agentns). signet's `agentns-doctor` is the
  agentns third of that, done deeper (tri-state, counters, receipt).
  If onramp-doctor is ever built it should *shell out to* agentns-doctor
  for its agentns check, not re-implement it.
- **session-receipt complements scribe + session-postmortem.** ctrace
  records per-session syscall histograms from eBPF; agentns counters
  are the kernel's own per-ns tally. The receipt makes the two
  joinable on `agent_session_id`. Not a duplicate: ctrace observes from
  outside, agentns counts from inside the namespace.

## Open questions

- **Init-ns inode stability.** This session sees `/proc/self/ns/agent`
  → inode `4026531996`; `PRD-claude-agentns-wrap.md` §1.1 recorded
  `4026531837` on 2026-05-27. Both are in the init-ns inode range
  (`0xF0000000+`), but they differ — is the init agentns inode stable
  across boots, or must the doctor detect "init" by *session==zero*
  rather than by a hardcoded inode? (Leaning: classify by `session==
  all-zeros AND file present`, treat the inode as advisory only. A
  hardcoded init inode is fragile across kernel rebuilds.)
- **Receipt emission trigger.** On-demand only (v0.1), or hook into a
  SessionEnd path? SessionEnd is unreliable for headless sessions
  (the SIGKILL-skips-the-hook problem scribe is fixing) — so a
  pull-based `receipt --emit <pid>` from self-review may be more
  robust than a push from the dying session. Decide before Fleet 2.
- **Where does the receipt live?** `~/.cache/agentns/receipts/<sid>.json`
  mirrors ctrace's `~/.cache/ctrace/sessions/` layout. Confirm before
  wiring the join.
- **Counters in init ns are always zero** — so `receipt` produces a
  zeros-ledger until a session is actually wrapped. Honest, but worth
  a `--require-wrapped` flag that exits non-zero in `init` state so
  receipt-in-self-review doesn't litter zeros-receipts pre-wrap.
