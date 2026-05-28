# PRD: wintermute-brain — Claude API loop with persistent memory

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-24
**Vision:** `visions/wintermute.md`
**Builds on:** `PRD-wintermute-dialog.md` (which forwards utterances), `PRD-recall-daemon.md` (sub-10 ms memory retrieval)
**Required by:** Fleet 2 action layer (browser, mail, desktop, etc. — they're brain-routed tool calls)
build_auto: true
build_target: rust-cli
build_priority: high

---

## TL;DR

`wmd` is the brain — the Claude API conversation loop with
recall-backed persistent memory. Sonnet 4.6 by default (Opus 4.7
opt-in), prompt-cached on her persistent profile + the day's
history, with sub-10 ms recall retrieval for context. Routes tool
calls to the action layer (Fleet 1 has a minimal tool stub; Fleet 2
adds browser/mail/etc.). Handles offline gracefully with a spoken
apology and a small cached-content fallback. Returns destructive
intents to wm-dialog for verbal confirmation — the brain never
acts destructively without dialog gating.

Plan-agent's split point: this PRD is *just* the Claude-loop +
tool-router + memory. Turn-taking, barge-in, and verbal-confirm are
in `wm-dialog`.

---

## 1. Why this exists

Three observations:

1. **Long-running conversations need persistent memory.** A whole
   day of small exchanges accumulates context — what she did this
   morning, what she's worried about, what her caregiver mentioned.
   The Claude API loop needs to find the relevant memories on every
   turn; `recall` v0.4's daemon mode (PRD-recall-daemon.md) gives us
   the sub-10 ms retrieval the inner loop demands.

2. **Sonnet is the chat default.** Plan-agent's challenge: defaulting
   to Opus burns money and adds latency on every utterance. Sonnet
   4.6 handles 95% of casual conversation; Opus 4.7 is opt-in for
   complex questions ("help me write a letter to my sister"). Bring
   `wmd --model opus` and the brain uses it for the next turn only.

3. **Tool routing belongs at the brain layer.** Fleet 1 has minimal
   tools (shell `date`, `weather`, a stub for action layer). Fleet 2
   adds the real action surface — browser, mail, calendar, music,
   desktop. The brain's tool-router contract is the seam Fleet 2
   plugs into; getting it right in Fleet 1 saves rework.

---

## 2. What this builds

### 2.1 Binary: `wmd`

A long-running Rust daemon. On startup:

1. Load the API key from `WM_ANTHROPIC_API_KEY`.
2. Connect to `recall-daemon`'s Unix socket
   (`$XDG_RUNTIME_DIR/recall.sock`); fall back to in-process `recall`
   library if daemon is down (warn loudly — sub-10 ms retrieval
   becomes 500 ms).
3. Load her profile: `WM_USER_NAME`, `WM_TIMEZONE`, plus any
   accumulated profile recall under subject `wintermute-profile`.
4. Subscribe to `wm.dialog.turn.user` events; publish `wm.brain.*`.

Events subscribed:

| Topic | Payload |
|---|---|
| `wm.dialog.turn.user` | `{transcript, confidence, ts}` |
| `wm.dialog.confirm.granted` | `{intent_id, ts}` |
| `wm.dialog.confirm.denied` | `{intent_id, reason, ts}` |

Events published:

| Topic | Payload |
|---|---|
| `wm.brain.reply` | `{text, ts}` (normal reply → wm-dialog → wm-tts) |
| `wm.brain.reply.destructive` | `{text, intent_id, summary, confirm_keyword, action, ts}` |
| `wm.brain.tool.call` | `{tool, args, ts}` |
| `wm.brain.tool.result` | `{tool, ok, body, ts}` |
| `wm.brain.error` | `{kind, message, ts}` |

### 2.2 Conversation loop

On `wm.dialog.turn.user`:

1. **Retrieve.** Query `recall-daemon` for top-K memories relevant
   to the utterance, filtered by subject `wintermute-thread-<day>`
   and `wintermute-profile`.
2. **Compose.** Build the Anthropic API request:
   - System prompt: her name, time zone, persona ("you are her
     companion, voice-first; speak naturally; one paragraph max
     per turn unless asked"), available tools, child-lock state.
   - **Prompt cache breakpoint #1**: persistent profile (rarely
     changes; long TTL via `cache_control: ephemeral` then upgrade
     to extended-cache if request volume warrants).
   - **Prompt cache breakpoint #2**: today's conversation history
     (grows during the day; rotates at midnight).
   - User message: the transcript.
3. **Call.** Stream the response (`stream: true`) to allow
   sentence-boundary-driven TTS handoff (first sentence → wm-tts
   while remaining sentences still arrive — Fleet 2 optimization
   if needed; v1 buffers full response).
4. **Route.** If the response contains tool calls, execute them
   (see 2.3); loop. If destructive (see 2.4), publish
   `wm.brain.reply.destructive`. Otherwise publish `wm.brain.reply`.
5. **Memorize.** Append the (user_text, assistant_text) pair to
   recall under subject `wintermute-thread-<YYYY-MM-DD>`. Trim oldest
   entries if today's thread exceeds a configured size.

### 2.3 Tool router (Fleet 1 minimum surface)

| Tool | Purpose | Notes |
|---|---|---|
| `wm.time.now` | "what time is it?" | local; no API call needed |
| `wm.weather.today` | weather lookup | calls wttr.in or similar; cached 15 min |
| `wm.recall.search` | "what did we talk about last week?" | recall daemon hit |
| `wm.recall.save_fact` | "remember that I prefer chamomile tea" | writes to wintermute-profile subject |
| `wm.tts.tone` | speak with a specific tone | passes to wm-tts as a parameter |
| `wm.fleet2.<stub>` | Fleet 2 tool stubs | return "not yet" gracefully |

Tool calls are dispatched async; results feed back into the API
request as `tool_result` blocks. Fleet 2 will add browser / mail /
calendar / music / desktop tools through the same router shape.

### 2.4 Destructive intent gating

The system prompt instructs the model: "If you intend to take any
action that deletes data, sends a message, makes a purchase, or
changes anything outside the conversation, format your reply as a
JSON block `{intent: '...', summary: '...', confirm_keyword: 'short-keyword'}`
in a final fenced block. Do not perform the action — wait for
confirmation."

`wmd` parses for this block. If present:
- Publish `wm.brain.reply.destructive` with the parsed fields
- wm-dialog runs verbal confirmation
- On `wm.dialog.confirm.granted`: execute the intent (publish
  `wm.brain.tool.call`)
- On denied: speak a cancellation acknowledgment, drop the intent

### 2.5 Offline behavior

If the API call fails (network down, rate limit, 5xx):
- Cache the user utterance with a `pending: true` flag
- Speak a friendly apology via wm-tts: "I can't reach the internet
  right now. I'll remember what you said. Would you like me to play
  some music?"
- Fleet 1 minimum: tell time, read pre-cached news headlines (if
  Fleet 2 news shipped), play music (if Fleet 2 music shipped) —
  in v1, just the apology and time
- Retry every 30 s on a background task; on success, replay the
  pending utterance

### 2.6 Model swap

`wmd --model opus` for the next turn only; `wmd --default-model
sonnet|opus` to change persistent default. Plan-agent's note: Opus
is for deep questions, not chatty defaults.

### 2.7 Recall integration

Today's thread: subject `wintermute-thread-<YYYY-MM-DD>`. Each turn
saved as one memory record:
```yaml
---
subject: wintermute-thread-2026-05-24
type: turn
ts: 2026-05-24T14:23:45Z
---
USER: What's the weather like?
ASSISTANT: It's sunny and 72 right now in Los Angeles.
```

Profile: subject `wintermute-profile`. Facts like "prefers chamomile
tea" or "daughter's name is Sara" saved here as separate records.
Retrieved on every turn with high priority.

Long-term: nightly, summarize the day's thread into a "day digest"
memory under `wintermute-day-digest`; trim raw turns older than
30 days.

---

## 3. Open-source dependencies

| Crate / tool | Version | Purpose | License |
|---|---|---|---|
| `anthropic-sdk-rust` (or hand-rolled via `reqwest`) | ^0.4 | Claude API | MIT |
| `recall` library + `recalld` socket client | local | memory | local |
| `tokio` | ^1.40 | async | MIT |
| `serde` + `serde_json` | ^1 | API + event payloads | MIT |
| `tracing` | ^0.1 | logs | MIT |
| `agorabus` client | local | pub/sub | local |
| Claude API | Anthropic | LLM | commercial |

Anthropic SDK choice: prefer the official Rust SDK if it exists at
build time; otherwise `reqwest` + hand-rolled JSON is straightforward
for the streaming + tool-use surface we need.

---

## 4. Acceptance criteria

1. End-to-end "wake → first TTS audio of brain reply" ≤2 s for a
   short query, with all components warm and the prompt cache hot.
2. Conversation context survives reboot — after restart, asking
   "what were we talking about?" surfaces today's earlier turns via
   recall.
3. Prompt cache hit rate ≥60% across a typical day of conversation
   (measured by `cache_read_input_tokens` from API responses).
4. Network drop → spoken apology within 3 s, not a hang or crash.
   Pending utterance replays on network restore.
5. Destructive intent test suite: 10 scripted destructive prompts
   each produce a `wm.brain.reply.destructive` event with valid
   intent_id and confirm_keyword; none execute without granted
   confirmation.
6. Model swap via `wmd --model opus` for next turn uses Opus exactly
   once, then reverts.
7. `wm.recall.save_fact` and `wm.recall.search` tool calls work
   end-to-end (verified via `recall list --subject wintermute-profile`).
8. 8-hour steady-state run with simulated 100 turns: no leaks
   (RSS growth <100 MB), no missed dialog events, no zombie pending
   utterances.

## 5. Out of scope (Fleet 2 / 3)

- Browser / mail / desktop tools — Fleet 2 plugs them into the same
  tool-router.
- Multi-day thread continuity beyond recall summarization — Fleet 3.
- Voice profile / speaker recognition — Fleet 3.
- Sentiment-aware reply tuning — Fleet 3.
- Sentence-stream-to-TTS for sub-1-s perceived latency — Fleet 2
  optimization.

## 6. Risks

- **recall-daemon dependency.** If `PRD-recall-daemon.md` hasn't
  shipped, fall back to in-process recall (500 ms instead of
  10 ms). Loud warning; performance acceptable but not ideal.
- **API cost surprise.** Sonnet on a chatty day is ~$0.50-$2; Opus
  could 10× that. Prompt caching is the main mitigation; track and
  surface daily cost via `wm cost` (small follow-up).
- **JSON-in-text destructive parsing.** Models occasionally emit
  malformed JSON. Mitigation: robust parser + clear system-prompt
  examples; on parse failure, treat as non-destructive and log
  for debugging.
- **Thread bloat.** A talkative day could push recall storage hard.
  30-day raw retention + nightly digest is the mitigation; revisit
  if storage growth surprises.

## 7. Open questions

- Should the system prompt include her photo / preferences from
  the bootstrap form, or wait until Fleet 3's voice-profile work?
  Leaning: include her name + time zone in v1; richer persona
  Fleet 3.
- Should `wmd` listen on its own Unix socket for direct queries
  from other tools, or stay agorabus-only? Leaning: agorabus-only
  in v1; add a socket if a future tool needs synchronous-RPC shape.
- Daily digest: model-generated summary or templated extraction?
  Leaning: model-generated, cached, generated overnight when
  she's not talking.
