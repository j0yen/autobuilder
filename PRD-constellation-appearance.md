# PRD: constellation-appearance — every node looks pixel-identical

Status: Draft v0.1
build_target: mixed
build_into: /home/jsy/wintermute/constellation
Vision: visions/constellation.md

## TL;DR

The user's requirement is explicit: "Make them identical in appearance — i3, the
terminal window shapes, all taskbar tools — everything." Provisioning installs the
same *packages*; this PRD makes the *configuration* identical down to the i3 gaps,
terminal geometry, status bar, and theme — while still rendering correctly on
machines with different monitors and GPUs. It does this with **chezmoi**, the one
dotfile manager that templates per-host.

## Why this exists

Phase 1 research compared the dotfile managers for exactly this requirement:

- The need is "identical appearance but different monitors/GPU" — which is
  **per-host templating**. **GNU stow** (symlink farm) and **bare git** cannot do
  it; they break precisely on the per-host divergence. **chezmoi** does it natively
  (Go `text/template`, conditions on `.chezmoi.hostname`/custom data), needs no
  external deps, and has built-in `age` secret encryption.
- The raw material already exists: `~/wintermute/dotfiles/` and the
  `wintermute-desktop` repo hold the i3 config and desktop setup. Migrating them
  into a chezmoi source tree is contained and reversible.
- Without templating, "identical i3" fails the moment one machine has one monitor
  (`eDP-1`) and another has two (`DP-1`/`DP-2`), or one is HiDPI — the i3 `output`/
  scaling stanzas must differ while everything else stays byte-identical.

## What this builds

A chezmoi source tree under `constellation/chezmoi/` (or a dedicated
`wintermute-dotfiles` chezmoi repo referenced by it), migrating and templating the
existing config:

- **i3** — one `config.tmpl` producing byte-identical keybindings, gaps, colors,
  bar, and workspace layout on every host; only the `output`/monitor/scaling
  stanza is gated on `{{ .chezmoi.hostname }}` / a `.monitors` data var.
- **Terminal** — identical emulator config: font, size, padding, **window
  geometry/"shapes"**, colorscheme, so terminals are visually identical across
  nodes.
- **Status/taskbar** — the bar (i3status/i3blocks/polybar — whichever the existing
  desktop uses) config identical, with per-host blocks (e.g. battery only on the
  laptop) gated by template conditions, not divergent files.
- **Theme/appearance** — GTK/Qt theme, cursor, fonts, wallpaper — one source,
  applied everywhere.
- **Per-host data** — a typed `.chezmoidata`/`chezmoi.toml.tmpl` per host
  declaring `gpu`, `monitors`, `voice_node`, `role` — the same metadata
  constellation-provision uses, so the two layers agree.
- **Secrets** — any tokens in dotfiles (e.g. a status-bar weather key) stored
  **age-encrypted** in the source; decrypted at `chezmoi apply` with the host's
  age key (never in the repo).
- A `chezmoi apply` (run by the constellation-provision `desktop` role) renders the
  identical environment on a fresh node.

Non-goals: package install (constellation-provision), the boot wiring (provision),
mesh/bus. This PRD is the *look*: byte-identical config with per-host templating.

## Acceptance criteria

1. The existing `~/wintermute/dotfiles/` + `wintermute-desktop` i3 config are
   migrated into a chezmoi source tree that `chezmoi apply`s cleanly on this
   laptop with no visual regression (i3, terminal, bar unchanged).
2. `chezmoi diff` on two hosts with different `.monitors`/`gpu` data shows
   **differences only** in the monitor/output/GPU-gated stanzas; all other rendered
   config (keybindings, gaps, colors, terminal geometry, bar layout) is
   byte-identical (asserted by diffing the rendered outputs).
3. A simulated second host (different hostname + monitor data) renders a valid i3
   config whose non-output sections are byte-identical to this laptop's rendered
   config.
4. The terminal config renders identical font/size/padding/geometry/colors across
   hosts (one rendered output compared byte-for-byte minus any host-gated line).
5. Per-host blocks (e.g. a battery indicator) appear only on hosts whose data
   enables them, via template conditions — there is exactly one source file for the
   bar, not per-host copies.
6. Any secret in the dotfiles is stored age-encrypted in the source tree; the tree
   contains no plaintext secret (grep-asserted), and `chezmoi apply` decrypts with
   the host key.
7. The chezmoi per-host data schema (`gpu`, `monitors`, `voice_node`, `role`)
   matches the keys constellation-provision's `host_vars` use (documented
   cross-reference; a test/lint asserts the key names agree).
