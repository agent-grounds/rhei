#!/usr/bin/env python3
"""Neither script hands a bare tool name to the process launcher. §REQ-cross-platform.3

What the resolution buys only shows on Windows, where `CreateProcess` appends
`.exe` and nothing else, so a `gh` installed as a `.bat` - which is how a shim is
written, and how these tests' own stub is written - is unreachable. The stamp
tests are the behavioural proof and they run on that platform in CI
(§AR-ci-release.1); this reads the two sources instead, so that dropping the
resolution fails on every platform rather than on the one leg.
"""

from __future__ import annotations

import ast
import unittest
from pathlib import Path

from scripts.tests.changelog_test_support import GATE, STAMPER

# The tools both scripts spawn, and the only two whose lookup is at stake.
SPAWNED = ("git", "gh")


class ToolLookupTests(unittest.TestCase):
    def assert_no_bare_name_is_spawned(self, script: Path) -> None:
        spawns = list(self.argv_of_every_spawn(script))
        self.assertTrue(spawns, f"{script.name}: no subprocess.run to check")
        for argv in spawns:
            where = f"{script.name}:{argv.lineno}"
            self.assertIsInstance(
                argv,
                ast.List,
                f"{where} spawns an argv whose executable the source does not show; "
                f"build `[tool(<name>), *rest]` so the lookup is there to read",
            )
            self.assertTrue(argv.elts, f"{where} spawns an empty argv")
            first = argv.elts[0]
            if not (isinstance(first, ast.Constant) and isinstance(first.value, str)):
                continue
            self.assertNotIn(
                first.value,
                SPAWNED,
                f"{where} spawns {first.value!r} by its bare name; resolve it through "
                f"`tool_lookup.tool` so PATHEXT decides on Windows",
            )

    def argv_of_every_spawn(self, script: Path):
        for node in ast.walk(ast.parse(script.read_text(encoding="utf-8"))):
            if not isinstance(node, ast.Call):
                continue
            func = node.func
            if not isinstance(func, ast.Attribute) or func.attr != "run":
                continue
            if not isinstance(func.value, ast.Name) or func.value.id != "subprocess":
                continue
            self.assertTrue(node.args, f"{script.name}:{node.lineno} spawns with no argv")
            yield node.args[0]

    def test_the_gate_resolves_what_it_spawns(self):
        self.assert_no_bare_name_is_spawned(GATE)

    def test_the_stamper_resolves_what_it_spawns(self):
        self.assert_no_bare_name_is_spawned(STAMPER)


if __name__ == "__main__":
    unittest.main()
