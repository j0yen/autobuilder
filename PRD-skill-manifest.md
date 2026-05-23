# PRD: Skill Manifest 2.0 (codename: *manifest*)

**Author:** Claude (Opus 4.7), for me
**Status:** Draft v0.1 — extension to Claude Code's SKILL.md format
**Date:** 2026-05-22
**Forks:** the Claude Code skill loader's frontmatter parser. Backwards-compatible: a missing manifest block means the skill loads exactly as today.

---

## TL;DR

SKILL.md frontmatter today is two fields: `name` and `description`. That's enough for matching but nothing else. I have 14+ skills installed; I have no way to express "this skill requires `ctrace` to be on the path," "this skill calls into `update-config`," "this version of self-review wants recall ≥ v0.2," "this skill's args must be a path that exists." When I tested `/self-review` this session and found three SKILL.md bugs, two of them were the kind of thing a manifest schema with a validation step would have caught at install time. This PRD adds an optional `manifest:` block in the frontmatter — version, requires, exports, inputs (JSON schema), tests — and a `skill validate` command that runs it. Old skills keep working unchanged.

---

## 1. Why this exists

Things that hurt during today's `/self-review` work:

1. **No version pinning.** I edited the SKILL.md mid-session. There's no record of "self-review needs to be at least version N for recall v0.2 to work." Future-me regrets this.
2. **No declared dependencies.** `/self-review` calls `ctrace`, `procstat`, `wchg`, `txn-edit`, and `recall`. If any are missing the skill fails partway through. No precondition check.
3. **No declared composition.** `/self-review` invokes `Skill(fewer-permission-prompts)` and `Skill(update-config)`. Today this is text in the prose; the loader doesn't know.
4. **No input schema.** Skills take free-form text args. `/loop 5m /self-review` is a parse-this-yourself contract. A schema would catch typos and make scripted invocation possible.
5. **No tests.** A skill is a markdown file; I can't run it against a fixture and check the output. The first real consumer of `/self-review` (today) was production. That's wrong.
6. **No version on installed skills.** When I `git pull` a skill plugin, am I on v1 or v3? Today: I don't know.

---

## 2. Who this is for

Me — I write and read SKILL.md files daily. The user — when they install or update a plugin, they want to know what changed. Skill authors — they want their skills validated automatically.

---

## 3. What I'd use it for (concretely)

| Today                                                                          | With manifest |
| ------------------------------------------------------------------------------ | ------------- |
| `/self-review` silently crashes if `ctrace` isn't installed                    | Precondition fails at load time with "self-review requires ctrace ≥ 0.3.0; not found in PATH" |
| I edit a SKILL.md and lose track of whether the field schema changed           | Manifest's `version` bumps trigger a changelog entry; `skill diff` shows old-vs-new manifest |
| Tests for a skill don't exist                                                  | `tests:` block points at fixture(s); `skill test self-review` runs them |
| `/init` skill scaffolds new skills with consistent shape                       | The scaffold includes a manifest stub; auto-validate is a one-liner CI check |
| When two skills define overlapping behavior, I have no way to know             | `exports:` enumerates the public surface; conflicts are reportable |

---

## 4. Functional requirements

### 4.1 New frontmatter block

```yaml
---
name: self-review
description: Daily self-optimization pass for this laptop. ...
manifest:
  version: 2.1.0
  requires:
    binaries:
      - name: ctrace
        version: ">=0.3.0"
      - name: recall
        version: ">=0.2.0"
      - name: wchg
      - name: procstat
      - name: txn-edit
    skills:
      - name: update-config
        version: ">=1.0.0"
      - name: fewer-permission-prompts
        optional: true
  inputs:
    type: object
    properties:
      dry_run:
        type: boolean
        default: false
    additionalProperties: false
  exports:
    - name: run
      description: Execute the full six-phase pass
    - name: check_only
      description: Phase A only (no mutations)
  tests:
    - path: tests/fresh-laptop.fixture.tar.gz
    - path: tests/with-stale-jsonls.fixture.tar.gz
---
```

Everything under `manifest:` is optional. A SKILL.md with no `manifest:` block loads as today.

### 4.2 `skill` CLI

A new binary at `~/.local/bin/skill`:

```
skill validate <skill-name|path>      # check frontmatter shape, requires, inputs schema
skill list [--with-versions]          # list installed skills + versions
skill deps <skill-name>               # resolve requires graph; list missing
skill diff <skill-name>               # vs prior version (git-aware)
skill test <skill-name> [--fixture ...]  # run declared tests
skill scaffold <new-name>             # produces a stub SKILL.md with a manifest block
skill graph                           # dot-format of skill→skill composition
```

### 4.3 Precondition gating

At session start (or at first skill invocation), the loader runs `skill validate` on every installed skill. Validation failures:

- **Missing binary requirement:** skill loads but is flagged unavailable; invocation returns a clear error: "self-review requires ctrace ≥0.3.0 (not found)".
- **Missing skill requirement (non-optional):** same as above.
- **Frontmatter schema error:** skill does not load; SessionStart hook surfaces a warning line.
- **Version constraint unsatisfied:** skill loads but is flagged degraded.

### 4.4 Versioning

Semver. Skills bump explicitly; there's no auto-bump on edit. A `version-locked` skill installation pins to a specific version — `git pull`s that bump it require explicit `skill upgrade <name>` to take effect.

### 4.5 Composition contract

`Skill(other)` invocations check the caller's `requires.skills` list. Calling a skill not declared in `requires.skills` produces a runtime warning (not an error — keeps backward compat). Auto-fix: `skill validate --autofix self-review` adds discovered invocations to the manifest.

### 4.6 Input schema

`inputs:` is a JSON Schema (draft-07 minimum). When a skill is invoked with args, the loader validates against the schema. If validation fails, the error surfaces to the user and the skill is not invoked. Skills can carry the validated input dict in their context.

### 4.7 Tests

`tests:` is a list of fixture pointers. A fixture is a `.tar.gz` containing a minimal environment (memory store, state files, ctrace logs). `skill test` un-tars the fixture into a temp dir, runs the skill against it (with `--root <tmp>` overrides), and checks declared post-conditions.

Post-conditions can be:
- `journal.matches_glob: 'YYYY-MM-DD.md'`
- `apply_log.contains: {action: prune_session_jsonls}`
- `exit: 0`

---

## 5. Architecture

```
~/.local/bin/skill                   # the CLI
~/.claude/skills/<name>/SKILL.md     # extended frontmatter, backward-compatible
~/.claude/skills/<name>/tests/       # fixtures (gitignored if heavy)
~/.claude/skill-state/installed.json # version pins, last-validated-at, etc.
```

`skill` is a thin Python script (~400 LoC) or Rust binary. Pure userspace; no daemon. Validation runs synchronously at SessionStart and is fast (<100ms per skill).

---

## 6. Non-goals

1. Replacing Claude Code's existing skill loader. Manifest is *additive*.
2. Cross-machine skill distribution. Out of scope; today's `git clone` install path is fine.
3. Sandbox enforcement at the skill level. A skill that declares `requires.binaries: [rm]` doesn't get *prevented* from calling other things; the manifest is documentation + lint, not enforcement. (Pair with sbx for enforcement.)
4. Skill rollback. If `skill upgrade self-review 2.1.0` is bad, `git revert` is the rollback. Manifest doesn't add a new rollback machinery.

---

## 7. Phasing

| Phase | Scope                                                                 |
| ----- | --------------------------------------------------------------------- |
| 0     | Parser + `skill validate` + the schema. No enforcement.               |
| 1     | `skill deps` + `skill list --with-versions`. Precondition gating at load time. |
| 2     | `skill test` + fixture format. Self-review and recall ship test fixtures first. |
| 3     | `skill scaffold` (replace the bespoke `/init` scaffold) + `skill graph`. |
| 4     | Composition runtime warnings for undeclared `Skill(...)` calls.       |

---

## 8. Risks

- **Manifest goes stale.** A skill author edits behavior but not the manifest. *Mitigation:* `skill validate --strict` checks invariants the author claimed (e.g. "if `exports.run` is listed, the SKILL.md mentions `run` in its instructions").
- **JSON Schema is verbose.** For simple skills the schema block dwarfs the rest. *Mitigation:* the loader accepts a shorthand syntax for trivial cases (`inputs: free-text` ≡ accept any string).
- **Version pinning conflicts.** Two skills require different versions of the same dependency. *Mitigation:* require ranges, not exact versions; report conflicts via `skill deps`.

---

## 9. Open questions

1. Should manifest be in SKILL.md frontmatter or a separate `MANIFEST.yaml`? Frontmatter is one less file; separate file is easier to validate independently. Lean frontmatter; revisit if it bloats.
2. Should the manifest be queryable by recall? Tagging skills as `subject: tool:<name>` would let me ask recall "what do I know about self-review's history?". Cute but possibly duplicative of `spool`.
3. Should `skill test` integrate with autobuilder's iterate-and-prove loop? Skills are a perfect autobuilder target — bounded scope, declarative tests. Long-term yes; v0.1 keeps it simple.
4. Should declared `requires.binaries` install missing tools automatically? No — that's `/init`'s job. `skill deps` reports; user installs.
