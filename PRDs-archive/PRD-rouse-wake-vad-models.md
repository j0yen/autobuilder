# PRD: rouse-wake-vad-models — realize the `wm-models` bundle

Status: Draft v0.1
build_target: rust-extend
build_into: /home/jsy/wintermute/wintermute-audio
Vision: visions/rouse.md
Depends on: none (independent floor; ship first)
Codename: *provender* — the inference workers can't eat models that aren't there.

## TL;DR

`wm-audio`'s config already *names* a model bundle — config.rs:11:
*"Pretrained wake words shipped via the `wm-models` bundle."* — but that
bundle does not exist. `/usr/share/wintermute/models/wake/` and
`/usr/share/wintermute/models/vad/` are present, **root-owned, and
empty** (verified 2026-05-29), and the repo has **no `install.sh`**
(grep-confirmed). So the queued `audio-inference` PRD, which assumes it
can "drop them as part of the install step ... install.sh
--download-models" (PRD-wintermute-audio-inference §2.3), has nothing to
build on. This PRD makes the bundle real: a first-class
`wm-audio fetch-models` subcommand that downloads, checksum-verifies, and
installs the microWakeWord wake models and the Silero VAD model into the
model dirs, idempotently and with recorded provenance.

## Why this exists

- **The model dirs are empty and root-owned.** `ls -la
  /usr/share/wintermute/models/{wake,vad}/` → both empty; `stat` → `root:root 755`
  (verified this session). Nothing has ever provisioned them.
- **The code already expects a bundle.** `src/config.rs:11` documents the
  `wm-models bundle`; the `WakeWord` enum (config.rs:14-18) defines three
  pretrained wake words — `HeyJarvis` (default), `OkayNabu`, `HeyMycroft`
  — each of which needs an on-disk ONNX model. `WakeWord::parse`
  (config.rs:31) accepts all three. None are installed.
- **The inference PRD hand-waves provisioning.** PRD-wintermute-audio-
  inference §2.3 says models "live at /usr/share/wintermute/models/{wake,
  vad}/. Drop them as part of the install step ... downloadable via an
  install.sh --download-models flag." But there is no install.sh, and the
  target dir needs privilege to write. That PRD even ships an AC7
  "fallback on missing models" — i.e. missing-models is a known expected
  state precisely because nothing provisions them. This PRD removes the
  cause.
- **The runtime is proven.** `ort`/onnxruntime is already a dependency
  across the fleet (agorabus, cadence, atlas, ac-judge, ambient, …,
  grep-confirmed) — so a model that lands on disk here will load with an
  established toolchain; this PRD only has to *place* the bytes.

## What this builds

Extends `~/wintermute/wintermute-audio/` (rust-extend; preserves all
existing behavior, adds one subcommand + a module):

- **New subcommand `wm-audio fetch-models`** (and a `src/models.rs`
  module). It:
  1. Provisions the **Silero VAD** ONNX model into `<prefix>/vad/` and
     the three **microWakeWord** wake models (hey_jarvis, okay_nabu,
     hey_mycroft) into `<prefix>/wake/`, where `<prefix>` defaults to
     `/usr/share/wintermute/models` (matching wm-stt's hardcoded
     `models_root` and the inference PRD) and is overridable with
     `--prefix <dir>` for unprivileged/test installs (e.g.
     `~/.local/share/wintermute/models`).
  2. **Pins each model by exact source URL + sha256.** A static manifest
     in the module lists `{name, kind(wake|vad), url, sha256, license,
     filename}`. After download, the sha256 is verified; a mismatch is a
     hard error (no unverified blob is ever installed). microWakeWord
     models are Apache-2.0; Silero VAD is MIT — the license string is
     recorded per entry.
  3. Is **idempotent**: a model whose target file already exists and
     matches its pinned sha256 is skipped (logged `already-current`);
     `--force` re-downloads. Re-running `fetch-models` on a fully
     provisioned tree does no network I/O and exits 0.
  4. Writes a **provenance sidecar** `<prefix>/MODELS.json` recording,
     per installed model, `{name, kind, filename, sha256, url, license,
     fetched_ts_unix}` so an operator (or a future `selftest`) can audit
     what's installed and from where.
  5. Handles the **privileged-write** case cleanly: if `<prefix>` is not
     writable (the default root-owned case), it does NOT silently fail —
     it stages downloads to a temp dir, verifies them, then either
     installs via a single `install -m644` per file when writable, or
     prints the exact `sudo` command to complete the install and exits 2
     (a clear "re-run me with privilege" contract, not a panic).
  6. `--list` prints the manifest (names, kinds, sizes, licenses, target
     paths) without downloading; `--format json|text`.
- **No change to the daemon's runtime path.** This PRD only adds an
  out-of-band provisioning subcommand; `wm-audio start` is untouched.
  Detector loading itself is the inference PRD's job — this PRD just
  guarantees the files those detectors will look for are present.
- **Reuses existing deps where possible** (the crate's HTTP/hashing
  stack if present; otherwise add a minimal `ureq` + `sha2`). No async
  surface needed — `fetch-models` is a short-lived command.
- README + CHANGELOG entry + version bump per the repo's convention
  (it tracks v0.2.0 sections).

## Acceptance criteria

1. `wm-audio fetch-models --help` documents `--prefix`, `--force`,
   `--list`, and `--format`; `wm-audio --help` lists the new subcommand.
   All pre-existing subcommands/tests are unchanged and still pass;
   clippy clean.
2. `wm-audio fetch-models --list --format json` emits a valid JSON array
   of `{name, kind, filename, url, sha256, license, target_path}` for
   the four models (3 wake + 1 vad) without performing any download.
3. Into a writable `--prefix <tmp>`, `fetch-models` downloads and installs
   all four models; each installed file's sha256 equals its pinned value;
   `<tmp>/wake/` contains the three wake models and `<tmp>/vad/` the
   Silero model.
4. A pinned-sha256 mismatch (simulated via a test manifest entry with a
   wrong hash against a local fixture server) causes a hard error, a
   non-zero exit, and **no file installed** at the target path.
5. Idempotence: a second `fetch-models --prefix <tmp>` immediately after
   AC3 performs no network I/O, logs each model `already-current`, and
   exits 0. `--force` re-downloads and re-verifies.
6. `<prefix>/MODELS.json` exists after a successful run and records all
   four installed models with sha256, url, license, and a fetched
   timestamp.
7. Privileged-target contract: with a non-writable `--prefix`, the
   command verifies downloads in a temp dir, installs nothing it can't
   write, prints the exact `sudo` completion command, and exits 2
   (no panic, no partial unverified install).
8. `cargo test --release` ≥ current+5 (manifest parse, sha256 verify
   pass/fail, idempotence skip, provenance sidecar write, privileged-dir
   exit-2 path — using a local fixture HTTP server, no live network in
   tests). `cargo deny check bans licenses sources` clean.
9. After a real `sudo wm-audio fetch-models` (or `--prefix` user dir),
   `/usr/share/wintermute/models/wake/` is no longer empty — the
   condition that makes `audio-inference` AC7's fallback the *default*
   is removed. (Human-gated deployment check.)

## Non-goals

1. Loading or running the models — that's `audio-inference`.
2. Custom wake-word training — stock microWakeWord models only.
3. STT/TTS model provisioning (whisper, piper/lessac) — separate
   concern; wm-stt already references its own `models_root`. This PRD is
   wake + VAD only.
4. A general package manager — `fetch-models` is a single-purpose,
   manifest-pinned provisioner, not a plugin system.
