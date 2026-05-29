# PRD: wintermute-screen-narrate — describe-the-screen via Claude vision

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`,
  composes with `PRD-wintermute-desktop.md` (a11y-first fallback)
**Used by:** brain image-mode fallback when a11y is empty
build_target: rust-cli
build_priority: medium

---

## TL;DR

A small daemon `wm-screen-narrate` that captures the focused window
(or full screen) and answers a brain prompt against it using Claude's
vision input. The a11y tree from `wm-desktop` is preferred — this is
the image-mode fallback for canvas-heavy apps, video, or
electron-without-a11y.

---

## 1. Why this exists

Vision §End-state #8: *"Read what's on the screen if a sighted helper
points at it."* `wm-desktop` covers the AT-SPI-rich case;
`wm-screen-narrate` covers the rest. Without it, a non-literate user
can't get help with anything that's a rasterized surface — a YouTube
thumbnail, a Steam UI, a PDF viewer's render.

Concrete evidence from Phase 1:

- AT-SPI tree on a focused Firefox-with-YouTube returns empty for
  the video canvas region (verified pattern; identical to Chrome's
  a11y behavior).
- `grim` is X11-Wayland-portable (Arch has it for both). `scrot`
  is X11-only; given current X11-only baseline, scrot is fine for
  v1, but the codepath should accept either binary present.
- Claude API supports image inputs at the messages-API level (per
  the `claude-api` skill docs).

---

## 2. What this builds

### 2.1 Binary: `wm-screen-narrate`

Rust daemon. Captures via `scrot` (or `grim` if present), POSTs to
the Anthropic Messages API with the image + user prompt.

### 2.2 Tools (topic `wm.screen.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `describe` | `{prompt, region?}` | `{text, model, latency_ms}` |
| `read_text` | `{region?}` | `{text}` — OCR-flavored ("read me what it says") |
| `find_in_image` | `{prompt}` | `{found, where_natural_language}` |
| `screenshot` | `{region?}` | `{path}` — PNG only, returned for inspection |

`region` defaults to focused window (via `xdotool getactivewindow
getwindowgeometry`). Optional explicit `{x,y,w,h}`.

### 2.3 Pipeline

```
xdotool window geom ─▶ scrot -a x,y,w,h /tmp/wm-screen.png
                  ─▶ Claude messages API (vision)
                  ─▶ JSON reply on wm.screen.reply
```

PNG kept for 60 s in `/tmp/wm-screen-shots/` for debugging, then
deleted by a small janitor coroutine.

### 2.4 Model selection

Default Sonnet (cheap+fast for "what does my screen say"). Opt-up
to Opus on `{model:"opus"}` in tool call for vision-heavy questions
("describe this diagram in detail").

### 2.5 Cost & rate

A per-day soft budget in `~/.config/wintermute/screen-narrate.toml`
(default 100 calls/day) — exceed → brain says "I've used my image-
seeing budget for today, sorry." Tunable by caregiver.

---

## 3. Risks

- **Privacy.** Whole-screen capture can include private content.
  Default region is focused window only, not full screen. Caregiver
  can opt-in full-screen mode in the config.
- **Cost creep** — vision tokens are pricier; budget is the
  pressure-release valve. Logged to `recall` so jsy/caregiver can
  inspect.
- **Latency** — round-trip is bounded by the API call (~1-3s for
  Sonnet vision on a 1080p screenshot). Acceptable for "what's on
  the screen", not for real-time UI control.

---

## 4. Sequencing

Independent of `wm-browser` and `wm-desktop`. Brain calls
`describe`/`read_text` as a fallback when a11y is thin. Can ship
without `wm-desktop` (full-screen-on-demand still works); composes
better with it.

---

## 5. Acceptance criteria

1. `wm-screen-narrate describe {prompt:"what app is focused?"}` on
   a focused Firefox returns text containing `firefox` or `browser`
   (case-insensitive).
2. `wm-screen-narrate read_text` on a focused terminal showing the
   string "HELLO WORLD" returns text containing "HELLO WORLD".
3. `wm-screen-narrate find_in_image {prompt:"is there a play
   button?"}` on a YouTube page returns `{found:true}` with a
   natural-language location.
4. Region defaults to focused window only — full-screen mode requires
   `region:"screen"` or config flag.
5. Screenshots written to `/tmp/wm-screen-shots/` and reaped after
   60s by the janitor (gone on next `ls` after sleep 65).
6. Daily budget: 101st call same day rejected with explicit
   `budget_exceeded` reply; jsy hears a budget message via dialog.
7. Latency on a 1080p screenshot + Sonnet: p95 < 4 s in 20 sequential
   runs.
8. Cost telemetry: each call logs `tokens_in, tokens_out, model,
   latency_ms` to `recall` under `subject=self`.
9. Brain integration: a brain turn that requests `screen.describe`
   visible in recall as a single tool-call entry.
10. **[live]** Real round-trip: jsy says "what does this say?" with
    a recipe PDF open in a viewer, dialog speaks the recipe summary.
    End-to-end <8 s.
