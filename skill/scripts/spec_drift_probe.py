#!/usr/bin/env python3
"""spec-drift-probe — pre-Stage-3 ground-truth check for /autobuilder.

Before /autobuilder enters Stage 3, this probe inspects every external tool
the PRD backticks (e.g. `recall list --since 7d`, `letter-curate aggregate`)
and verifies that the PRD-asserted subcommand verbs actually exist in each
tool's real `--help` surface. Drift (a PRD that names a verb the binary does
not expose) blocks Stage 3 with exit 4.

CLI:
    spec-drift-probe.py <PRD-path> [--out <json-path>] [--strict]

Exit codes:
    0  no drift detected (clean, or only unavailable/help-broken tools)
    4  drift detected (one or more PRD-asserted verbs not in tool surface)
    2  bad invocation

Pure stdlib. No network calls. Writes only to the --out path.
PRD: autobuilder-spec-drift-probe.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from typing import Dict, List, Optional, Tuple

SCHEMA_ID = "spec-drift.v1"
DEFAULT_OUT = "target/autobuilder/spec-drift.json"

# A backticked invocation: a lowercase binary name, optional sub-verbs, then
# optional flags (each flag may carry a value token). Anchored at start; we only
# inspect spans that look like a CLI call. The trailing group tolerates flag
# values such as `--since 7d` or `--root ~/x` without treating them as verbs.
INVOCATION_RE = re.compile(
    r"^([a-z][a-z0-9-]*)"  # binary
    r"((?:\s+[a-z][a-z0-9-]*)*?)"  # zero+ sub-verbs (non-greedy)
    r"((?:\s+--?[a-z][a-z0-9-]*(?:[= ][^\s`]+)?)*)"  # zero+ flags w/ optional values
    r"\s*$"
)

# Words that read like a verb but are language/markup noise, not subcommands.
WORD_BLOCKLIST = frozenset(
    {
        # Rust idents that frequently appear backticked in PRDs.
        "let", "fn", "use", "mut", "pub", "impl", "mod", "enum", "struct",
        "trait", "async", "await", "match", "loop", "while", "for", "ref",
        "dyn", "self", "crate", "super", "where", "move", "type", "const",
        "static", "unsafe", "extern",
        # shell/markup noise.
        "true", "false", "null", "none", "some", "ok", "err",
    }
)

# Binary names that are not external CLIs in the spec-drift sense.
BINARY_BLOCKLIST = frozenset(
    {
        "cargo", "rustc", "rustup", "git", "bash", "sh", "python", "python3",
        "ls", "cd", "cat", "grep", "rg", "find", "sed", "awk", "echo", "cp",
        "mv", "rm", "mkdir", "touch", "chmod", "curl", "wget", "jq", "make",
        "cmake", "gcc", "clang", "npm", "node", "pip", "pip3", "docker",
        "systemctl", "journalctl", "sudo", "env", "export", "source", "set",
    }
)


# Hedging cues that mark a backticked CLI mention as speculative future-design
# rather than a hard assertion the generated code will call. PRDs routinely
# write "presumably `wm-tts metrics`" or "e.g. `tool foo`"; treating those as
# drift produces false positives (observed in AC6 backfill).
HEDGE_RE = re.compile(
    r"\b(presumably|maybe|perhaps|might|could|would|should|e\.?g\.?|"
    r"i\.?e\.?|such as|like|some|possibly|future|planned|tbd|placeholder|"
    r"hypothetical|or a |or an )\b",
    re.IGNORECASE,
)


def _sentence_around(text: str, idx: int) -> str:
    """Return the sentence-ish window of `text` containing offset `idx`."""
    start = max(text.rfind(".", 0, idx), text.rfind("\n", 0, idx))
    end_dot = text.find(".", idx)
    end_nl = text.find("\n", idx)
    ends = [e for e in (end_dot, end_nl) if e != -1]
    end = min(ends) if ends else len(text)
    return text[start + 1 : end]


def extract_invocations(prd_text: str) -> List[Tuple[str, bool]]:
    """Return (raw-span, hedged) pairs for every backticked span parsing as a CLI call.

    Handles both inline `code` spans and ``` fenced ``` blocks. Fenced-block
    lines are never hedged (they are concrete usage examples). Inline spans are
    flagged hedged when their surrounding sentence carries a speculation cue.
    """
    results: List[Tuple[str, bool]] = []

    # Fenced blocks first; record their extent so inline scanning skips them.
    fenced_spans: List[Tuple[int, int]] = []
    for block in re.finditer(r"```[a-zA-Z0-9_-]*\n(.*?)```", prd_text, re.DOTALL):
        fenced_spans.append((block.start(), block.end()))
        for line in block.group(1).splitlines():
            line = line.strip()
            if line:
                cleaned = re.sub(r"^[#$>]\s*", "", line).strip()
                if INVOCATION_RE.match(cleaned):
                    results.append((cleaned, False))

    def _in_fence(pos: int) -> bool:
        return any(s <= pos < e for s, e in fenced_spans)

    # Inline single-backtick spans outside fenced blocks.
    for m in re.finditer(r"`([^`\n]+)`", prd_text):
        if _in_fence(m.start()):
            continue
        cleaned = re.sub(r"^[#$>]\s*", "", m.group(1).strip()).strip()
        if not INVOCATION_RE.match(cleaned):
            continue
        hedged = bool(HEDGE_RE.search(_sentence_around(prd_text, m.start())))
        results.append((cleaned, hedged))
    return results


def parse_invocation(span: str) -> Optional[Tuple[str, List[str]]]:
    """Split a CLI span into (binary, [verbs]). Flags are dropped.

    Returns None if the binary is blocklisted noise.
    """
    m = INVOCATION_RE.match(span)
    if not m:
        return None
    binary = m.group(1)
    if binary in BINARY_BLOCKLIST or binary in WORD_BLOCKLIST:
        return None
    verbs_blob = m.group(2).strip()
    verbs = [
        v
        for v in verbs_blob.split()
        if v and v not in WORD_BLOCKLIST and not v.startswith("-")
    ]
    return binary, verbs


def collect_assertions(prd_text: str) -> Dict[str, set]:
    """Map binary -> set of asserted verbs across the whole PRD body.

    Hedged inline mentions register the binary (so it is still probed and
    reported) but contribute no asserted verbs, so they never produce drift.
    """
    assertions: Dict[str, set] = {}
    for span, hedged in extract_invocations(prd_text):
        parsed = parse_invocation(span)
        if parsed is None:
            continue
        binary, verbs = parsed
        bucket = assertions.setdefault(binary, set())
        if hedged:
            continue
        for v in verbs:
            bucket.add(v)
    return assertions


def help_text(binary: str) -> Tuple[Optional[str], int]:
    """Run `<binary> --help`; return (combined-output, exit-code).

    A None output means the process could not be launched at all.
    """
    try:
        proc = subprocess.run(
            [binary, "--help"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=5,
            text=True,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None, 127
    return proc.stdout, proc.returncode


# A subcommand line in clap/argparse help: at least two leading spaces, then the
# verb as the first word.
SUBCMD_RE = re.compile(r"^\s{2,}([a-z][a-z0-9-]*)\b")


def parse_subcommands(help_out: str) -> List[str]:
    """Extract the subcommand verbs from a tool's --help output."""
    verbs: List[str] = []
    seen = set()
    for line in help_out.splitlines():
        m = SUBCMD_RE.match(line)
        if not m:
            continue
        verb = m.group(1)
        # 'help' is universally present; keep it but de-dup.
        if verb not in seen:
            seen.add(verb)
            verbs.append(verb)
    return verbs


def probe_tool(binary: str, asserted_verbs: set, strict: bool) -> Dict:
    """Build the per-tool receipt entry."""
    entry: Dict = {
        "binary": binary,
        "found": False,
        "asserted_verbs": sorted(asserted_verbs),
        "actual_verbs": [],
        "missing_verbs": [],
        "verdict": "unavailable",
    }
    if shutil.which(binary) is None:
        # Intentional future-state binary; not drift.
        entry["verdict"] = "unavailable"
        return entry

    entry["found"] = True
    out, code = help_text(binary)
    if out is None or code != 0:
        entry["verdict"] = "help_fails"
        # In strict mode a help failure is treated as drift downstream.
        if out is not None:
            entry["actual_verbs"] = parse_subcommands(out)
        return entry

    actual = parse_subcommands(out)
    entry["actual_verbs"] = actual
    if not actual:
        # Help parsed cleanly but exposes no indented subcommand list — either a
        # flat single-purpose CLI or a non-clap/argparse help format (e.g.
        # `pactl`'s `tool [options] verb` usage lines). We have no positive
        # evidence of the subcommand surface, so we cannot prove drift.
        entry["verdict"] = "help_fails"
        return entry
    actual_set = set(actual)
    missing = sorted(v for v in asserted_verbs if v not in actual_set)
    entry["missing_verbs"] = missing
    entry["verdict"] = "drift" if missing else "clean"
    return entry


def run(prd_path: str, out_path: str, strict: bool) -> Tuple[Dict, int]:
    with open(prd_path, "r", encoding="utf-8") as fh:
        prd_text = fh.read()

    assertions = collect_assertions(prd_text)
    tools: List[Dict] = []
    drift_count = 0
    help_fail_count = 0
    for binary in sorted(assertions):
        entry = probe_tool(binary, assertions[binary], strict)
        tools.append(entry)
        if entry["verdict"] == "drift":
            drift_count += 1
        elif entry["verdict"] == "help_fails":
            help_fail_count += 1

    receipt = {
        "schema": SCHEMA_ID,
        "prd": os.path.abspath(prd_path),
        "tools": tools,
        "summary": {
            "tools_checked": len(tools),
            "drift_count": drift_count,
            "help_fail_count": help_fail_count,
        },
    }

    os.makedirs(os.path.dirname(os.path.abspath(out_path)) or ".", exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(receipt, fh, indent=2, sort_keys=True)
        fh.write("\n")

    blocked = drift_count > 0 or (strict and help_fail_count > 0)
    return receipt, (4 if blocked else 0)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="spec-drift-probe.py",
        description="Verify PRD-named CLI verbs exist before Stage 3.",
    )
    parser.add_argument("prd", help="path to the PRD markdown file")
    parser.add_argument(
        "--out",
        default=DEFAULT_OUT,
        help=f"output JSON path (default: {DEFAULT_OUT})",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="also block (exit 4) when a tool's --help fails",
    )
    try:
        args = parser.parse_args(argv)
    except SystemExit:
        return 2

    if not os.path.isfile(args.prd):
        print(f"spec-drift-probe: no such PRD: {args.prd}", file=sys.stderr)
        return 2

    receipt, code = run(args.prd, args.out, args.strict)
    if code == 4:
        json.dump(receipt, sys.stderr, indent=2, sort_keys=True)
        sys.stderr.write("\n")
    return code


if __name__ == "__main__":
    sys.exit(main())
