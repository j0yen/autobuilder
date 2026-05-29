# PRD: wintermute-browser — voice-driven web browsing

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** shipped
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`
**Used by:** wintermute-news (Fleet 2 bullet, future)
build_target: rust-cli
build_priority: medium
deferred_acs: [1, 6, 9, 10]
deferred_ac_reasons:
  1: "open returns a real page title within 5s — requires a live headed Chromium over CDP, unavailable in the offline /build sandbox"
  6: "wmd registering browser.* as Claude tools, confirmed via recall log — requires a running wmd brain plus recall, neither present offline"
  9: "kill -9 the Chromium subprocess then verify auto-restart — requires a real browser process to kill; only the connection-lost predicate is offline-testable"
  10: "real user voice round-trip (open→find→click→read, dialog speaks summary) — requires jsy plus the full live Fleet 1+2 stack (mic, TTS, brain, browser)"
mock_unjustified_for: [1, 6, 9, 10]
mock_justifications:
  1: "Mocking Chromium's CDP responses would make the 5s-warm-browser timing AC tautological — the latency claim is the whole point and only a real browser proves it."
  6: "A mock wmd tool registry would assert nothing about whether the real brain actually wires and invokes browser.* in a live turn; the recall-log evidence requires the genuine integration."
  9: "The restart half of AC9 (kill -9 subprocess → daemon detects within 2 s → next tool call succeeds) requires a real Chromium process to kill; the detection predicate is already covered by ac9_connection_lost_predicate_* lib tests, and a mock that fakes SIGKILL on a fake subprocess proves nothing about the real process lifecycle."
  10: "An end-to-end voice round-trip is inherently a live-human, full-stack acceptance; any mock would substitute the very components (mic/TTS/brain/browser) the AC exists to validate together."

---

## TL;DR

A long-running daemon `wm-browser` that exposes a small JSON-over-
agorabus tool interface for the brain: `open`, `read`, `click`,
`type`, `back`, `find`. Driven by a headed Chromium instance via CDP
(`chromiumoxide` crate, not Playwright — there is no Rust Playwright
binding, the wintermute vision doc's "Playwright" label is aspirational
shorthand for "browser automation"). Accessibility snapshot is the
canonical read mode; image-mode fallback uses `wm-screen-narrate`.

---

## 1. Why this exists

Vision §End-state #7: *"Browse the web by description. 'Find me a
recipe for chicken soup with celery.'"* This is the largest single
capability gap in Fleet 1 — the brain can talk but it cannot do.

Concrete evidence from Phase 1:

- `~/wintermute/baton/` already drives X11 keystroke injection
  cleanly; the browser is a different surface (in-page DOM, not
  process-keystroke-stream) and needs its own driver.
- `~/wintermute/wintermute-bootstrap` is shipped — caregiver UX
  already runs an HTTP server, but production browse needs
  multi-page navigation, link clicks, form fills, all directed by
  voice.
- No Rust binding exists for Playwright. `chromiumoxide` is the
  mature Rust CDP client (commit history active through 2025); it
  gives us full browser control including a11y snapshot.

---

## 2. What this builds

### 2.1 Binary: `wm-browser`

Rust daemon. Subprocess-launches one headed Chromium (or chromium-
headed via `--app=about:blank` for kiosk feel). Holds a `chromiumoxide
::Browser` handle and a single active tab.

### 2.2 Tools (JSON envelopes over agorabus topic `wm.browser.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `open` | `{url}` | `{ok, title, url, snapshot_id}` |
| `read` | `{}` | `{snapshot: <a11y-tree-json>, snapshot_id}` |
| `click` | `{ref}` | `{ok}` — `ref` from snapshot |
| `type` | `{ref, text, submit?}` | `{ok}` |
| `back` | `{}` | `{ok, url}` |
| `find` | `{query}` | `{matches: [{ref,role,name}]}` — text+role filter on snapshot |
| `screenshot` | `{}` | `{path}` — PNG into `/tmp/wm-browser-shots/` |

Replies go on `wm.browser.reply` with the `cmd_id` echoed.

### 2.3 Brain integration

Brain (wmd) registers `browser.*` as Claude API tools. Tool schemas
generated from the table above. Snapshot is the LLM-readable view;
brain pages through it on long pages.

### 2.4 A11y snapshot format

Reuse Playwright's mental model — a flat JSON list of `{ref, role,
name, value, children_refs}` where each element gets a stable per-
snapshot ref. `ref` is opaque to the brain (string), resolved
internally to a DOM selector + frame path.

### 2.5 Process lifecycle

- Started by `wmd` on demand (first tool call), not at boot.
- Idle timeout 5 min → daemon exits cleanly, browser closes.
- Pevent supervises (`pevent run -n wm-browser …`) for crash restart.

---

## 3. Risks

- **Chromium snap/flatpak vs system package** — `chromium` from the
  Arch repo gives a stable CDP. Document this in install.sh.
- **Snapshot can be massive** (10k+ refs on a search-results page).
  Cap returned snapshot at 2000 refs with a `truncated: true` flag
  and a `find` instruction for the brain to query rather than scroll.
- **Auth and cookies** — persistent profile dir at
  `~/.local/share/wintermute/browser-profile/`. No auth tooling in
  v1; user types passwords by voice or via caregiver setup.
- **Per-page JS hostility** (anti-bot, cloudflare) — out of scope v1;
  brain says "I can't load that page" and moves on.

---

## 4. Sequencing

Depends on Fleet 1 brain + dialog being shipped (they are — see
CLAUDE_SELF changelog 2026-05-27 for tts/platform/dialog and
2026-05-28 for stt). No new external substrate.

Can ship in parallel with `wintermute-desktop` (different surface).
`wintermute-screen-narrate` is independent but composes well — when
`wm-browser read` returns a non-a11y page (e.g. canvas-heavy), brain
can chain `wm-screen-narrate.image_describe`.

---

## 5. Acceptance criteria

1. `wm-browser open https://example.com` returns `{ok:true,
   title:"Example Domain"}` within 5 s on a warm browser.
2. `wm-browser read` returns a snapshot containing at least the H1
   text "Example Domain" with role `heading` and a resolvable ref.
3. `wm-browser find {query:"More information"}` on example.com
   returns at least one match with role `link` and a usable ref.
4. `wm-browser click {ref}` on the matched link navigates to the
   target page; subsequent `read` returns the new page's title in
   the snapshot.
5. `wm-browser type {ref, text, submit:true}` on Google's
   search box submits and lands on a results page (text "results"
   appears in next snapshot).
6. Brain (wmd) registers `browser.open` etc. as Claude tools — a
   `recall` log entry confirms the tool was called from a brain
   turn (`recall query --subject self --since 1h | grep wm-browser`
   shows the invocation).
7. Snapshot capped at 2000 refs; on a Google results page,
   `read` returns `truncated:true` and brain falls back to `find`.
8. Daemon idle timeout: no tool call for 5 min → exits with rc=0
   and removes its lockfile.
9. Crash recovery: kill -9 the Chromium subprocess → daemon detects
   within 2 s, restarts the browser, next tool call succeeds.
10. **[live]** Real user round-trip: jsy says "find me a recipe for
    chicken soup", brain plans `open → find → click → read`, dialog
    speaks the summary. End-to-end <30 s on a warm browser.
