# PRD: wintermute-desktop — AT-SPI tree reading + keystroke injection

**Author:** /dream (Claude Opus 4.7), with jsy
**Status:** Draft v0.1
**Date:** 2026-05-27
**Vision:** `visions/wintermute.md` (Fleet 2 — action layer)
**Builds on:** `PRD-wintermute-dialog.md`, `PRD-wintermute-brain.md`,
  reuses `~/wintermute/baton/` for keystroke injection on X11
**Used by:** `PRD-wintermute-screen-narrate.md` (a11y-first fallback)
build_target: rust-cli
build_priority: medium

---

## TL;DR

A daemon `wm-desktop` that gives the brain a read+act surface on the
running X11 desktop: AT-SPI accessibility tree as the read mode,
xdotool (via the `baton` wrapper) as the act mode. Mirrors
`wm-browser`'s shape (tools over agorabus, snapshot-with-refs) so the
brain has one consistent mental model.

---

## 1. Why this exists

Vision §End-state #8: *"Read what's on the screen if a sighted helper
points at it."* Browser is one surface; the rest of the laptop is the
other — text editors, settings apps, file managers, anything she
might end up looking at.

Concrete evidence from Phase 1:

- `~/wintermute/baton/` shipped at j0yen/baton (CLAUDE_SELF
  changelog 2026-05-24) — X11 window-id resolution + keystroke
  envelope already solved. Reuse, don't rebuild.
- `atspi-rs` crate (active maintenance, 0.x but covering accessible-
  object tree, event subscription, action invocation) is the right
  read interface.
- The laptop is X11 (per `~/.claude/CLAUDE_SELF.md` defaults: baton
  uses xdotool). No Wayland AT-SPI complication yet.

---

## 2. What this builds

### 2.1 Binary: `wm-desktop`

Rust daemon. Subscribes to AT-SPI for application-level events;
exposes tools over agorabus.

### 2.2 Tools (topic `wm.desktop.cmd`)

| Tool | Args | Returns |
|---|---|---|
| `apps` | `{}` | `{apps:[{name, pid, window_ids}]}` |
| `focus` | `{app or window_id}` | `{ok, window_id}` |
| `read_window` | `{window_id?}` | `{snapshot, snapshot_id}` — AT-SPI tree of the focused window |
| `click` | `{ref}` | `{ok}` — resolves ref to action+target |
| `type` | `{text}` | `{ok}` — into focused window via baton |
| `key` | `{combo}` | `{ok}` — e.g. `ctrl+s`, via baton |
| `find` | `{query, role?}` | `{matches}` — text+role filter on snapshot |

### 2.3 Snapshot shape

Same `{ref, role, name, value, children_refs}` shape as `wm-browser`
for brain-side mental-model consistency. Roles map from AT-SPI to a
small vocabulary: `button | textfield | label | menuitem | window |
container | other`.

### 2.4 Permissions

AT-SPI requires the accessibility bus to be running. `wm-bootstrap`'s
already-shipped one-time setup didn't enable it — wm-desktop's
install.sh enables `accessibility.service` (user) and writes the
required `at-spi-bus-launcher` autostart if missing.

### 2.5 Brain integration

Brain registers `desktop.*` as Claude tools. Distinct namespace from
`browser.*` so the brain can pick the right surface.

---

## 3. Risks

- **AT-SPI coverage is app-dependent.** GTK apps and modern Qt apps
  have rich trees; Electron apps vary. When `read_window` returns a
  near-empty tree, brain falls back to `wm-screen-narrate`.
- **xdotool race conditions** under window-manager animations —
  inherited from baton; baton's window-resolution cache helps but
  isn't a fix. Document a 100ms post-focus delay before typing.
- **Wayland future** — out of scope; revisit when the user's
  deployment hardware moves to Wayland.

---

## 4. Sequencing

Independent of `wm-browser`. Both can develop in parallel; brain
picks the right tool based on context. Composes with
`wm-screen-narrate` for non-a11y surfaces.

Reuses `baton` directly — depends on `baton` being installed in
`~/.local/bin/` (verified: CLAUDE_SELF defaults document `baton` as
a built-and-tested local tool family member, per j0yen/baton ship).

---

## 5. Acceptance criteria

1. `wm-desktop apps` lists at least 3 running applications by name
   on a typical X11 desktop with terminal + browser + file-manager
   open.
2. `wm-desktop focus {app:"firefox"}` focuses Firefox; subsequent
   `xdotool getactivewindow` returns Firefox's window-id.
3. `wm-desktop read_window` on a focused gedit returns a snapshot
   containing the menubar items "File", "Edit", "View" with role
   `menuitem`.
4. `wm-desktop click {ref}` on a gedit menubar "File" ref opens the
   File menu (next snapshot contains "New", "Open").
5. `wm-desktop type {text:"hello"}` into focused gedit inserts the
   text (a follow-up `read_window` snapshot contains it in the doc
   tree).
6. `wm-desktop key {combo:"ctrl+s"}` triggers the save dialog
   (snapshot contains a `window` role with name matching `Save`).
7. `wm-desktop find {query:"Cancel", role:"button"}` on a focused
   dialog returns the Cancel-button ref; clicking dismisses.
8. AT-SPI bus health: `wm-desktop apps` on a fresh user session
   with accessibility off auto-enables the bus via install-time
   autostart, then succeeds on the next launch.
9. Snapshot capped at 1500 refs; on a busy file manager,
   `read_window` returns `truncated:true` and brain uses `find`.
10. **[live]** Real round-trip: jsy says "open my notes and add 'buy
    bread'", brain chains `focus → key(ctrl+s)`-like flow on her
    actual notes app. End-to-end <15 s.
