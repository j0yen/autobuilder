#!/usr/bin/env python3
"""Tests for spec-drift-probe (PRD autobuilder-spec-drift-probe, AC1-AC4, AC8).

Pure stdlib. Run with:
    python3 -m unittest tests/test_spec_drift_probe.py
or:
    python3 tests/test_spec_drift_probe.py
from the skill root.

AC1: `recall list --since 7d` -> recall.list real -> exit 0, drift_count 0.
AC2: `letter-curate aggregate` -> no aggregate verb -> verdict drift, exit 4.
AC3: `future-tool start` not on PATH -> verdict unavailable, exit 0.
AC4: receipt validates against schemas/spec-drift.schema.json.
AC8: module imports as `spec_drift_probe` with stdlib only.
"""

import json
import os
import sys
import tempfile
import unittest

SKILL_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(SKILL_ROOT, "scripts"))

import spec_drift_probe  # noqa: E402  (AC8: importable)

SCHEMA_PATH = os.path.join(SKILL_ROOT, "schemas", "spec-drift.schema.json")


def _write_prd(body: str) -> str:
    fd, path = tempfile.mkstemp(suffix=".md")
    with os.fdopen(fd, "w") as fh:
        fh.write(body)
    return path


def _run(body: str, strict: bool = False):
    prd = _write_prd(body)
    out_fd, out_path = tempfile.mkstemp(suffix=".json")
    os.close(out_fd)
    try:
        receipt, code = spec_drift_probe.run(prd, out_path, strict)
        with open(out_path) as fh:
            on_disk = json.load(fh)
        return receipt, code, on_disk
    finally:
        os.unlink(prd)
        if os.path.exists(out_path):
            os.unlink(out_path)


def _tool(receipt, binary):
    for t in receipt["tools"]:
        if t["binary"] == binary:
            return t
    return None


class SpecDriftProbeTests(unittest.TestCase):
    def test_ac1_recall_list_clean(self):
        if spec_drift_probe.shutil.which("recall") is None:
            self.skipTest("recall not on PATH in this environment")
        receipt, code, _ = _run("Body uses `recall list --since 7d` here.\n")
        rc = _tool(receipt, "recall")
        self.assertIsNotNone(rc)
        self.assertTrue(rc["found"])
        self.assertIn("list", rc["actual_verbs"])
        self.assertEqual(rc["verdict"], "clean")
        self.assertEqual(receipt["summary"]["drift_count"], 0)
        self.assertEqual(code, 0)

    def test_ac2_letter_curate_aggregate_drift(self):
        if spec_drift_probe.shutil.which("letter-curate") is None:
            self.skipTest("letter-curate not on PATH in this environment")
        receipt, code, _ = _run("PRD asserts `letter-curate aggregate` output.\n")
        lc = _tool(receipt, "letter-curate")
        self.assertIsNotNone(lc)
        self.assertTrue(lc["found"])
        self.assertNotIn("aggregate", lc["actual_verbs"])
        self.assertEqual(lc["verdict"], "drift")
        self.assertEqual(lc["missing_verbs"], ["aggregate"])
        self.assertEqual(code, 4)

    def test_ac3_unavailable_binary(self):
        receipt, code, _ = _run("Future state: `zzz-future-tool start` runs.\n")
        ft = _tool(receipt, "zzz-future-tool")
        self.assertIsNotNone(ft)
        self.assertFalse(ft["found"])
        self.assertEqual(ft["verdict"], "unavailable")
        self.assertEqual(code, 0)

    def test_ac4_schema_validates(self):
        receipt, _, on_disk = _run(
            "Body uses `recall list` and `zzz-future-tool start`.\n"
        )
        with open(SCHEMA_PATH) as fh:
            schema = json.load(fh)
        # Lightweight stdlib validation: required keys, types, enum membership.
        self._validate(on_disk, schema)
        self.assertEqual(on_disk["schema"], "spec-drift.v1")

    def test_blocklist_ignores_rust_idents(self):
        receipt, code, _ = _run("Use `let x = 1` and `fn main`.\n")
        self.assertEqual(receipt["summary"]["tools_checked"], 0)
        self.assertEqual(code, 0)

    def test_blocklist_ignores_common_binaries(self):
        receipt, _, _ = _run("Run `cargo build` then `git commit`.\n")
        self.assertIsNone(_tool(receipt, "cargo"))
        self.assertIsNone(_tool(receipt, "git"))

    # --- minimal draft-07 subset validator (no jsonschema dependency) ---
    def _validate(self, obj, schema):
        t = schema.get("type")
        if t == "object":
            self.assertIsInstance(obj, dict)
            for req in schema.get("required", []):
                self.assertIn(req, obj, f"missing required key {req}")
            props = schema.get("properties", {})
            for k, v in obj.items():
                if k in props:
                    self._validate(v, props[k])
        elif t == "array":
            self.assertIsInstance(obj, list)
            item_schema = schema.get("items")
            if item_schema:
                for item in obj:
                    self._validate(item, item_schema)
        elif t == "string":
            self.assertIsInstance(obj, str)
            if "const" in schema:
                self.assertEqual(obj, schema["const"])
            if "enum" in schema:
                self.assertIn(obj, schema["enum"])
        elif t == "integer":
            self.assertIsInstance(obj, int)
            if "minimum" in schema:
                self.assertGreaterEqual(obj, schema["minimum"])
        elif t == "boolean":
            self.assertIsInstance(obj, bool)


if __name__ == "__main__":
    unittest.main()
