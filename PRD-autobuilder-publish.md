# PRD: autobuilder-publish — codify Stage 6 into an `autobuilder publish` subcommand

**Status:** Draft v0.1
**build_auto:** true
**build_target:** rust-extend
**build_into:** /home/jsy/wintermute/autobuilder/autobuilder
**build_version_bump:** minor
**deferred_acs:** [10]
**Created:** 2026-05-28

---

## TL;DR

Stage 6 of the autobuilder pipeline (publish a finished slice as its own
`github.com/j0yen/<slug>` repo) is today a **manual convention** documented in
`SKILL.md` — the companion binary has no `publish` subcommand, so the human (or
the `/build` skill) performs the README/LICENSE generation, branch rename, repo
creation, push, and `REPOS.md` update by hand each time. This PRD adds an
`autobuilder publish` subcommand that codifies those steps deterministically,
idempotently, and **without bypassing the existing `wm-publish` / `wm-push`
safety wrappers**. It is dogfooding: autobuilder building the last unautomated
stage of itself.

## Motivation

A research pass over the skill (2026-05-28) found that of four doc-vs-reality
discrepancies, three were documentation errors (since fixed) and one was a
genuine limitation: **Stage 6 publish is not automated.** `SKILL.md` §"Stage 6
— Publish" already specifies the exact four steps; they are stable enough to
encode in the binary. Automating them removes a hand-run, error-prone ritual
(wrong branch left on origin, missing dual license, `REPOS.md` drift) and makes
the pipeline self-contained end-to-end.

## Goals

- Add `autobuilder publish` that performs the documented Stage-6 steps.
- Preserve the safety boundary: the subcommand **shells out to `wm-publish`
  and `wm-push`**; it must not invoke `gh repo create` or `git push` directly.
- Be idempotent and re-runnable; support `--dry-run`.
- Emit a `publish-receipt.json` consistent with the existing receipt model.

## Non-goals

- Changelog generation (owned elsewhere; out of scope).
- Monorepo import (explicitly replaced by the per-repo convention).
- Batch / multi-slice publishing (a future `experiment`-level concern).
- Mutating the `wm-publish` / `wm-push` ALLOW lists (a human/`/build` step).

## Design

### Interface

```
autobuilder publish [OPTIONS]
  --project <path>      Project root (default: cwd). Must contain agent/intent-card.json.
  --slug <slug>         Repo slug (default: intent-card .slug).
  --visibility <v>      public | private (default: public).
  --license-year <YYYY> Year stamped into LICENSE files (default: from $AUTOBUILDER_DATE
                        or required; NEVER from a nondeterministic clock — keeps runs reproducible).
  --category <cat>      REPOS.md category (pipeline|runtime|memory|session|artist|...).
  --force               Regenerate README/LICENSE even if present.
  --dry-run             Print the plan; make zero writes and zero network calls.
```

### Steps (mirrors SKILL.md Stage 6)

1. **Resolve.** Read `agent/intent-card.json`; derive slug, `root_motivation`,
   and the MUST-level acceptance criteria. Fail clearly if the card is absent
   or fails schema validation.
2. **README + LICENSE.** Generate `README.md` (overview = `root_motivation`;
   "Acceptance criteria" list = MUST ACs; "Install" + "License" footer). Write
   dual `LICENSE-MIT` + `LICENSE-APACHE` with the `Joe Yen` copyright holder and
   the supplied `--license-year`. Skip existing files unless `--force`.
3. **Branch normalize.** If on `autobuilder/<slug>`, delete any stale local
   `main` (the iter-0 scaffold baseline) and rename `autobuilder/<slug>` →
   `main`. No-op if already on `main`.
4. **Commit.** A single commit "Prep for standalone distribution: README +
   dual MIT/Apache-2.0 license" using the `Joe Yen <jyen.tech@gmail.com>`
   identity (per-command `-c`, never `.git/config`).
5. **Create + push (via wrappers).** Invoke `wm-publish --slug <slug>
   [--private]` to create the remote; if it reports the repo already exists,
   continue. Then `wm-push` to push `main`. The subcommand resolves these
   wrappers from `$PATH` and shells out — it does not embed `gh`/`git push`.
6. **REPOS.md.** Ensure exactly one line for the slug under `--category` in
   `~/wintermute/REPOS.md` (resolve via `$WINTERMUTE_HOME` or default). Append
   if missing; leave untouched if present (idempotent).
7. **Receipt.** Write `target/autobuilder/receipts/publish-receipt.json`.

### Receipt shape

```json
{
  "slug": "...", "repo_url": "https://github.com/j0yen/<slug>",
  "branch": "main", "head_sha": "...",
  "readme_generated": true, "license_generated": true,
  "repos_md_updated": true, "repo_preexisting": false,
  "dry_run": false, "verdict": "published",
  "captured_at": "...", "schema": "publish-receipt/v1"
}
```

## Acceptance criteria

1. **(MUST)** `autobuilder publish --help` lists the subcommand and all flags
   above. *(unit: trycmd/assert_cmd help snapshot)*
2. **(MUST)** With a fixture project containing `agent/intent-card.json`,
   `publish --dry-run` prints the ordered plan (README/LICENSE, branch rename,
   create, push, REPOS.md) and makes **zero filesystem writes and zero network
   calls**, exit 0. *(unit: run under a temp dir; assert no mtime changes via a
   pre/post snapshot; no wrapper invoked)*
3. **(MUST)** Generated `README.md` contains `root_motivation` as the overview
   and each MUST AC from the intent-card as a list item. *(unit: fixture card →
   assert README contents)*
4. **(MUST)** `LICENSE-MIT` and `LICENSE-APACHE` are written to the project
   root with holder "Joe Yen" and the `--license-year` value. *(unit)*
5. **(MUST)** From a local fixture git repo on `autobuilder/<slug>`, after a
   non-dry-run publish (with stub wrappers, no network) the working branch is
   `main` and the stale `main` baseline is gone. *(unit: local git fixture)*
6. **(MUST)** The subcommand invokes `wm-publish` and `wm-push` from `$PATH`
   and does **not** call `gh repo create` or `git push` directly. *(unit: PATH
   shim records wrapper invocations; static-grep check that the source contains
   no direct `gh repo create` / `git push` string)*
7. **(MUST)** Idempotent create: when the stub `wm-publish` reports the repo
   already exists, publish does not error, skips creation, and still re-syncs
   README/REPOS.md. *(unit)*
8. **(MUST)** `REPOS.md` gains exactly one line for the slug under the given
   category; re-running does not duplicate it. *(unit: fixture REPOS.md, run
   twice, assert single entry)*
9. **(MUST)** A `publish-receipt.json` matching the shape above is written to
   `target/autobuilder/receipts/` and validates against a `publish-receipt/v1`
   schema added under `skill/schemas/`. *(unit + schema validation)*
10. **(DEFERRED — live/network)** A real end-to-end publish of a fresh slice
    creates the public `j0yen/<slug>` repo and pushes `main`. *Deferred reason:*
    requires live GitHub auth + network + a genuinely new slug, which cannot run
    inside the hermetic `sbx --no-net` gate. Verified by one supervised live
    publish of the next real slice through the subcommand.

## Testing strategy

- ACs 1–9 are hermetic: fixtures for the intent-card, REPOS.md, and a local git
  repo; **stub `wm-publish`/`wm-push` scripts** placed on `$PATH` that record
  their args and return canned results (success / already-exists). This lets the
  Stage-3 loop run the full suite under `sbx --no-net` with no real network.
- AC10 is the single live-verification criterion, exercised once out-of-band.

## Risks & mitigations

- **Bypassing the safety wrappers** would defeat their slug/allow/remote checks.
  → AC6 enforces shell-out + a static check for forbidden direct calls.
- **Non-idempotent re-runs** could double-create or duplicate REPOS.md lines.
  → ACs 7 and 8 pin idempotency.
- **Nondeterministic license year** would break reproducible builds.
  → `--license-year` is explicit; no clock read in the publish path.
- **`REPOS.md` location drift** across hosts. → resolve via `$WINTERMUTE_HOME`
  with a documented default; never hard-code an absolute path in logic.

## Out of scope / future

- `autobuilder publish --changelog` integration.
- Auto-managing the `wm-publish`/`wm-push` ALLOW lists (stays a human/`/build`
  decision so the safety boundary keeps a human in the loop).
