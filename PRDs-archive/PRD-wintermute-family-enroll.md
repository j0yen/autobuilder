# PRD: wintermute-family-enroll — the caregiver setup wizard

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-28
**Vision:** visions/kin.md
**build_target:** rust-cli
**build_version_bump:** n/a (new repo j0yen/wintermute-family-enroll)
**Depends on:** PRD-wintermute-family-intents, PRD-wintermute-reach, PRD-wintermute-presence
**Codename:** *handshake* — who is Joe, how do we reach him, what gets shared.

## TL;DR

A device with no enrolled caregiver is a companion, not kin. The companion
vision claimed `wintermute-bootstrap` "already assumes a headless device"
with a caregiver-setup flow — but `bootstrap/install.sh` is 217 lines of
package install with no enrollment wizard. This PRD builds the real one: a
`wm-family setup` CLI that records who "Joe" is, the transport to reach him,
mother's waking-hours window, and the per-feature opt-in toggles — writing it
all to `/etc/wintermute/conf.d/` where reach, presence, and dialog read it.
It is the deployment capstone of kin.

## 1. Why this exists

- **kin vision Component 6; the capstone.** Every other kin PRD reads config
  from `/etc/wintermute/conf.d/` — recipient names (family-intents), transport
  (reach), waking hours + opt-ins (presence, digest). Until something *writes*
  that config, the fleet runs on hard-coded fallbacks. This PRD writes it.
- **The claimed caregiver flow doesn't exist.** Phase 1: `bootstrap/install.sh`
  has no `caregiver|enroll|wizard` match; it's 217 lines of `pacman`/symlink
  setup. The companion vision's assumption was aspirational. This is honest
  fulfillment of it.
- **Privacy must be set, not defaulted silently.** Vision OQ#2 makes consent
  load-bearing: presence/silence/digest default OFF, distress defaults ON, and
  the enroll flow is where a human deliberately turns the surveillance-shaped
  features on — and where wintermute can be made to *tell mother* what's shared.

## 2. What this builds

A new repo `j0yen/wintermute-family-enroll`, a CLI (not a daemon).

### 2.1 Config schema (`/etc/wintermute/conf.d/50-family.env` or `.toml`)

```
WM_FAMILY_RECIPIENT_NAME=Joe
WM_FAMILY_RECIPIENT_TRANSPORT=email        # email|ntfy|webhook
WM_FAMILY_RECIPIENT_ADDRESS=jyen.tech@gmail.com
WM_FAMILY_WAKING_START=08:00
WM_FAMILY_WAKING_END=21:00
WM_FAMILY_PRESENCE_ENABLED=false           # opt-in
WM_FAMILY_SILENCE_ENABLED=false            # opt-in
WM_FAMILY_DIGEST_ENABLED=false             # opt-in
WM_FAMILY_DIGEST_TIME=20:00
WM_FAMILY_DISTRESS_ENABLED=true            # on by default
```

### 2.2 Setup flow

- `wm-family setup` — interactive prompts (recipient name/transport/address,
  waking hours, each opt-in), with sane defaults and the privacy defaults
  above. Writes the config atomically (temp + rename), never clobbering
  unrelated keys in the conf.d dir.
- `wm-family setup --non-interactive --recipient … --transport … …` — flag
  form for headless/scripted provisioning (the bootstrap path).
- `wm-family show` — print current enrollment (redacting secrets).
- `wm-family announce` — emit a `wm.tts.say` so wintermute speaks the privacy
  summary aloud ("Joe will get a note that we talked today") — the spoken
  consent moment from vision OQ#2. Optional, invoked at end of setup.

### 2.3 Validation

- Transport address validated for the chosen transport (email shape, URL for
  ntfy/webhook); waking-end after waking-start; refuses to write an invalid
  config.
- A `test-reach` convenience that shells out to `wm-reach test-transport` if
  installed, so setup can confirm the transport actually works before
  declaring done (degrades to a note if reach isn't installed).

## 3. Acceptance criteria

1. `wm-family --help` lists `setup`, `show`, `announce`.
2. `wm-family setup --non-interactive --recipient Joe --transport email
   --address jyen.tech@gmail.com` writes a config file under a `--conf-dir`
   override containing all the keys in §2.1 (integration test against a temp
   dir).
3. Presence/silence/digest default to `false` and distress defaults to `true`
   when not specified (defaults test).
4. Writing is atomic (temp + rename) and leaves other files in the conf.d dir
   untouched (test with a sentinel sibling file).
5. An invalid transport address (e.g. malformed email) causes setup to refuse
   to write and exit non-zero (validation test).
6. Waking-end ≤ waking-start is rejected (validation test).
7. `wm-family show` prints the enrollment with the transport address present
   but any secret (token/password keys) redacted (test).
8. `wm-family announce` emits a `wm.tts.say` whose text names the recipient
   and what is shared (bus smoke test).
9. The written keys match exactly the names reach/presence/family-intents read
   (cross-checked against those PRDs' config keys — a documented key-name
   table in the README).
10. `cargo test` green; `cargo clippy` clean; autobuilder receipts produced.
