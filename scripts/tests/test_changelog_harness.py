#!/usr/bin/env python3
"""The harness interposes nothing between the scripts and git. §REQ-cross-platform.4

Two invariants, both of them learned the hard way and neither visible in the
behaviour of any other test: what `PATH` resolves `git` to, and what the tests'
environment carries into it. Each failed on one platform or one invocation only,
so each is asserted directly here rather than left to surface as three unrelated
assertions about changelog bullets.
"""

from __future__ import annotations

import os
import shutil
import unittest
from pathlib import Path

from scripts.tests.changelog_test_support import ScriptTestCase, clean_env


class HarnessTests(ScriptTestCase):
    def test_the_tests_path_resolves_git_outside_the_harness(self):
        """A wrapper forwarding to the real git is a shell, and `cmd` eats `^`.

        Windows reads `^` as its own escape character, so a `.bat` forwarding `%*`
        hands git `main{commit}` where `_Git.commit` wrote `main^{commit}`; every
        base candidate then fails to resolve and the check degrades. This reads the
        same on all three platforms, so the rule holds on the legs it never broke.
        """
        path = f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}"
        resolved = shutil.which("git", path=path)
        self.assertIsNotNone(resolved, "the tests' PATH does not reach git at all")
        self.assertFalse(
            Path(resolved).resolve().is_relative_to(self.tmp.resolve()),
            f"the tests' PATH resolves git to {resolved}, which this harness "
            f"generated; forward nothing to a real tool",
        )

    def test_the_tests_environment_carries_no_inherited_git_variable(self):
        """Git exports `GIT_DIR` into a hook's subprocesses, and this is a hook.

        Left in place it aims `TempRepo`'s `init`, `checkout -b` and `commit` at
        the repository being committed to. The only `GIT_*` the environment may
        carry are the ones `clean_env` sets itself.
        """
        env = clean_env()
        ours = {
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_SYSTEM",
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GIT_AUTHOR_DATE",
            "GIT_COMMITTER_NAME",
            "GIT_COMMITTER_EMAIL",
            "GIT_COMMITTER_DATE",
        }
        self.assertEqual(
            sorted(key for key in env if key.startswith("GIT_") and key not in ours),
            [],
            "an inherited GIT_* variable would let the caller's repository answer",
        )


if __name__ == "__main__":
    unittest.main()
