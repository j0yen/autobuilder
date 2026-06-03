# PRD: hearth — persona as configuration, not a recompile

**Author:** /dream (Claude Opus 4.8), for jsy
**Status:** Draft v0.1
**Date:** 2026-05-29
**Vision:** visions/hearth.md
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/wintermute-brain
**build_version_bump:** minor
**Depends on:** none
**Codename:** *register* — how she speaks becomes a knob, not a const.

## TL;DR

The companion's entire personality is a single `const` string in
`wintermute-brain/src/daemon.rs:47`
(`DEFAULT_PERSONA = "You are wintermute, a voice-first companion daemon.
…"`). Retuning how she speaks — warmer, briefer, what she calls the user,
whether she even calls herself "wintermute" — means editing Rust and
recompiling on a device at jsy's mother's home. This PRD lifts the
persona into a `[persona]` table in the existing `brain.toml`, ships a
default calibrated for a non-technical elder, and keeps it inside the
prompt-cache prefix so the change is free at inference time.

## 1. Why this exists

Found live in Phase 1 (2026-05-29), by reading the source:

- **The persona is hardcoded.** `src/daemon.rs:47` defines
  `DEFAULT_PERSONA` as a compile-time `const`. `compose_persona`
  (`src/daemon.rs:~118`) layers child-lock + destructive-gate + recall +
  session-recap onto it, but the *base* is never configurable.
- **The config struct is already the right home.** `src/lib.rs`
  `BrainConfig` carries `user_name`, `timezone`, `recap_opener`,
  `recap_max_memories`, `child_lock`, model fields — all `#[serde]` from
  `brain.toml` (`DEFAULT_CONFIG_BASENAME = "wintermute/brain.toml"`).
  Adding a persona section is a natural extension of an existing,
  atomic-write config surface, not a new mechanism.
- **The vision names it.** `visions/companion.md` Open Question #4 and
  `PRD-wintermute-dialog-turn-fsm.md` Non-goal #2 both defer the
  "personality model." `visions/hearth.md` makes it Component 1.
- **It's a deployment-safety issue.** On the target device there is no
  developer to recompile (see `PRD-wintermute-unit-recovery-watchdog`'s
  "no human" framing). A persona that can only change by rebuild is a
  persona that will never change in the field.

## 2. What this builds

### 2.1 A `[persona]` table in `BrainConfig`

Extend `BrainConfig` (`src/lib.rs`) with an optional persona section,
all fields `#[serde(default)]` so existing `brain.toml` files keep
working unchanged:

```toml
[persona]
self_name      = "wintermute"   # what she calls herself out loud
register       = "warm-elder"   # named preset; see 2.2
addresses_user = true           # weave user_name into replies
max_sentences  = 3              # soft brevity ceiling for spoken turns
extra          = ""             # free-text appended to the composed base
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonaConfig {
    #[serde(default = "default_self_name")]
    pub self_name: String,
    #[serde(default)]
    pub register: Register,        // enum, default WarmElder
    #[serde(default = "default_true")]
    pub addresses_user: bool,
    #[serde(default = "default_max_sentences")]
    pub max_sentences: u8,
    #[serde(default)]
    pub extra: String,
}
```

Add `persona: PersonaConfig` to `BrainConfig` with `#[serde(default)]`.

### 2.2 Named registers → composed base prose

`Register` is an enum (`WarmElder` (default), `Plain`, `Brisk`) mapping
to a base prose template. `WarmElder` is the calibrated default — the
prose the current `const` *should* have been for the actual user:

> *You are {self_name}, a kind companion who speaks with {user_name}
> aloud — never on a screen. Talk like a warm, patient person: short
> sentences, plain everyday words, one thought at a time. Never use
> technical words like "daemon", "config", "API", or "error code"; if
> something is wrong, say it the way a friend would. Keep replies to a
> few sentences unless asked for more. No markdown, lists, code, or
> emoji — they do not speak well.*

`compose_persona` is refactored to build the base from `PersonaConfig`
instead of reading the `const`. `DEFAULT_PERSONA` is retained as the
`Plain` register's template (zero behavior change for anyone who selects
it) so nothing silently regresses. `{self_name}` / `{user_name}` are
substituted from config; if `addresses_user` is false or `user_name` is
unset, the user clause is omitted cleanly.

### 2.3 Prompt-cache discipline

The composed persona is the **stable prefix** of the system prompt and
must remain byte-stable across turns for cache hits (see
`PRD-brain-prompt-cache`). The persona is composed **once at config
load**, not per turn; the per-turn recall/recap blocks continue to come
*after* the cache breakpoint. AC asserts the composed persona string is
identical across two `compose_request` calls with the same config.

### 2.4 CLI surface

Mirror the existing `swap-model` / `default-model` pattern in
`src/main.rs`: add `wmd persona show` (print the composed base) and
`wmd persona set-register <warm-elder|plain|brisk>` (atomic-write the
config, same path the model swaps use). No daemon restart semantics
beyond what model-swap already does.

## 3. Acceptance criteria

1. **AC1 — tests grow.** `cargo test --release --lib` passes with
   ≥ current+6 tests (persona deserialization with all-defaults,
   per-field override, register→prose mapping for all three variants,
   `{name}` substitution, user-clause omission when unset, cache-prefix
   stability).
2. **AC2 — backward compatible.** A `brain.toml` with **no** `[persona]`
   table deserializes to `PersonaConfig::default()` (register
   `WarmElder`) and the daemon starts. A test loads a persona-less TOML
   fixture and asserts defaults.
3. **AC3 — register selects prose.** `compose_persona` with
   `register = Plain` produces a base byte-identical to the retained
   `DEFAULT_PERSONA` const; with `WarmElder` it contains "short
   sentences" and contains none of the substrings "daemon", "API",
   "config".
4. **AC4 — name substitution.** With `self_name = "Ada"` and
   `user_name = "Mum"`, the composed base contains "Ada" and "Mum" and
   not the literal "{self_name}" / "{user_name}".
5. **AC5 — cache prefix stable.** Two `compose_request` calls with the
   same config and different transcripts yield byte-identical system-
   prompt content up to the persona boundary.
6. **AC6 — CLI round-trips.** `wmd persona set-register brisk` followed
   by `wmd persona show` prints the brisk base; the written `brain.toml`
   re-parses to `register = Brisk`. Verified against the built binary.
7. **AC7 — destructive + child-lock guards still layer.** With a custom
   persona and `child_lock = true`, the composed system prompt still
   contains both `DESTRUCTIVE_GATE_GUARD` and `CHILD_LOCK_GUARD` (the
   layering order from `compose_persona` is preserved).

## 4. Non-goals

- Learned/adaptive persona (recall-backed) — vision OQ #1.
- Per-turn persona switching — register is a config-level setting.
- Touching the degrade phrase banks — separate PRDs
  (`hearth-dialog-degrade-warmth`, and `companion-degrade` owns the
  brain fault bank).
