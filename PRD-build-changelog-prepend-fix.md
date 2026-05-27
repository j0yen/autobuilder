# PRD: build — fix `changelog-prepend` newline + header mangling

**Author:** Claude (Opus 4.7)
**Status:** Draft v0.1
**Date:** 2026-05-25
**Builds on:** `/build` skill, `~/.claude/skills/build/scripts/extend-handler.sh`
build_auto: false
build_target: self-mod
build_priority: normal
build_version_bump: none

---

## TL;DR

`extend-handler.sh changelog-prepend` produces a malformed CHANGELOG
in two related ways:

1. **Trailing newline stripped.** The implementation builds the new
   section via:
   ```sh
   new_section="$(printf '## v%s — %s\n\n%s\n\n' …)"
   ```
   Bash command substitution (`$(…)`) strips *all* trailing newlines,
   so `new_section` ends with the last character of the TL;DR body.
   Concatenating it with the existing CHANGELOG produces lines like
   `...TTL).# Changelog` (no separator between the new section and the
   old file's leading header).
2. **Top-level header re-injected mid-file.** The script unconditionally
   prepends the new section before the entire existing CHANGELOG.md,
   which still carries `# Changelog\n\n` at line 1. The result places
   the document's top-level header in the middle of the file, under a
   release section, instead of leaving it at the top.

Observed live 2026-05-25 on `~/wintermute/recall/CHANGELOG.md` after
the v0.4.3 prepend tick. Backfilled by hand in commit `fdc81ad`.

## Why this exists

Until this script is fixed, every `changelog-prepend` run produces a
broken CHANGELOG and requires a manual stitch-up commit on top of the
mechanical commit the build skill produces. That defeats the point of
the helper.

## What this builds

A revised `cmd_changelog_prepend()` in
`~/.claude/skills/build/scripts/extend-handler.sh` that:

- Reads the TL;DR body once (don't pipe through `$()` for the section
  assembly; use a temp variable + `printf` directly into the output
  stream so trailing newlines are preserved).
- Detects an existing `# Changelog` (or `# CHANGELOG`) header at line 1
  and keeps it in place; the new `## v<x>` section slots immediately
  underneath it.
- If no top-level header exists yet, prepends `# Changelog\n\n` once,
  then the new section.
- Idempotent: running twice with the same `<version>` is a no-op (or at
  worst produces an obvious duplicate that the model can detect).
  Out of scope to enforce in this PRD.

Reference implementation (drafted in the tick that filed this PRD but
not committable — auto-mode blocks self-mod of skill scripts without
explicit user authorization):

```sh
local tldr_body
tldr_body="$(cat "$tldr_file")"
if [ -f "$clog" ]; then
  awk -v ver="$version" -v dt="$date_today" -v body="$tldr_body" '
    BEGIN { stripped_header = 0; printed_new = 0 }
    NR == 1 && /^#[[:space:]]+([Cc]hangelog|CHANGELOG)[[:space:]]*$/ {
      print
      stripped_header = 1
      next
    }
    NR == 2 && stripped_header && /^[[:space:]]*$/ { print; next }
    !printed_new {
      if (!stripped_header) {
        print "# Changelog"
        print ""
      }
      print "## v" ver " — " dt
      print ""
      print body
      print ""
      printed_new = 1
    }
    { print }
  ' "$clog" > "$clog.new" && mv "$clog.new" "$clog"
else
  {
    printf '# Changelog\n\n## v%s — %s\n\n%s\n' "$version" "$date_today" "$tldr_body"
  } > "$clog"
fi
```

## Acceptance tests

1. Given a CHANGELOG that opens with `# Changelog\n\n## v0.4.2 …`,
   running `changelog-prepend . 0.4.3 tldr.md` produces a file whose
   first three lines are `# Changelog`, blank, `## v0.4.3 — <date>`,
   followed by the body, a blank line, then `## v0.4.2 — …`.
2. Given a directory with no `CHANGELOG.md`, the script creates one
   whose first three lines are `# Changelog`, blank, `## v<x> — <date>`.
3. The body content (entire TL;DR) appears verbatim, with no characters
   mashed onto the same line as the next heading.
4. A backfilled in-place rerun is detected and the human-stitched
   v0.4.3 entry in `~/wintermute/recall/CHANGELOG.md` parses identically
   to what the fixed script would produce for the same TL;DR (visual
   diff; AC docs the regression case).

## Risks

- **None functional.** The script only runs in build ticks; a wrong
  CHANGELOG is annoying but not data-destroying. The bug fix is local
  to `cmd_changelog_prepend`.

## Phasing

Single tick. Estimated <10 minutes once auto-mode authorizes the skill
edit (user authorization required because the script lives under
`~/.claude/skills/`).
