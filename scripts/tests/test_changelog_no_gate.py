#!/usr/bin/env python3
"""No change is asked for a changelog bullet. §FS-rhei-distribution.5.1

The release generates its section from the merged pull requests, so nothing
checks a branch for a bullet: no hook (§AR-ci-release.2), no CI step
(§AR-ci-release.1), and no gate script. What stays is a check of the shape the
release generates into - the header, one inline release section, then
`Older releases`, and no `## Unreleased` - run over the repository's own
`docs/changelog.md` inside `gate script tests`, so a changelog the release could
not generate into fails on its own pull request rather than on the release.
"""

from __future__ import annotations

import importlib
import re
import sys
import unittest
from unittest import mock

from scripts.tests.changelog_test_support import (
    CHANGELOG,
    CI_WORKFLOW,
    PRE_COMMIT_CONFIG,
    REPO_ROOT,
    SCRIPTS,
    job_lines,
    release_changelog,
    workflow_steps,
)

# `main`'s ruleset requires this status check by its exact name.
REQUIRED_JOB = "repository gates (grund, fissile, lychee, changelog, attribution)"


class NoChangelogGateTests(unittest.TestCase):
    def test_no_hook_checks_the_changelog(self):
        """The registration goes first, so no push on this branch meets the hook."""
        text = PRE_COMMIT_CONFIG.read_text(encoding="utf-8")
        hooks = re.findall(r"^\s*(?:-\s+)?(?:id|name|entry):\s*(.+)$", text, flags=re.MULTILINE)
        self.assertTrue(hooks, f"no hooks read from {PRE_COMMIT_CONFIG.name}")
        self.assertEqual([hook for hook in hooks if "changelog" in hook.lower()], [])

    def test_ci_runs_no_changelog_check(self):
        for job in ("test", "lint"):
            for step in workflow_steps(CI_WORKFLOW, job):
                where = f"{job}: {step.get('name') or step.get('uses')}"
                self.assertNotIn("changelog", (step.get("name") or "").lower(), where)
                self.assertNotIn("check_changelog_pr_entry", step.get("run", ""), where)

    def test_the_lint_job_checks_out_one_commit(self):
        """The full history was only for the gate's base commit (§AR-ci-release.1)."""
        self.assertEqual([line for line in job_lines(CI_WORKFLOW, "lint") if "fetch-depth" in line], [])

    def test_the_gate_script_and_its_tests_are_gone(self):
        for path in ("scripts/check_changelog_pr_entry.py", "scripts/tests/test_changelog_gate.py"):
            self.assertFalse((REPO_ROOT / path).exists(), path)

    def test_the_stamper_and_its_bullet_parser_are_gone(self):
        """No bullet is written by hand, so nothing parses or stamps one."""
        for path in ("scripts/changelog_bullets.py", "scripts/tests/test_changelog_stamp.py"):
            self.assertFalse((REPO_ROOT / path).exists(), path)

    def test_the_required_job_keeps_its_name(self):
        """Passes today. Renamed, it is a required check no pull request can satisfy."""
        names = [line.strip() for line in job_lines(CI_WORKFLOW, "lint") if re.match(r"^    name:", line)]
        self.assertEqual(names, [f"name: {REQUIRED_JOB}"])


def release_parser():
    """`prepare_changelog_release` as the release imports it, with `scripts/` on the path."""
    with mock.patch.object(sys, "path", [str(SCRIPTS), *sys.path]):
        return importlib.import_module("prepare_changelog_release")


class ChangelogShapeTests(unittest.TestCase):
    """The shape `prepare` generates into, checked with its own heading patterns."""

    def assert_release_shape(self, text: str) -> None:
        release = release_parser()
        titles = [line.rstrip("\r\n") for line in text.splitlines() if line.startswith("## ")]
        self.assertTrue(titles, "no ## section at all")
        self.assertEqual([title for title in titles if re.match(r"^## Unreleased\s*$", title)], [],
                         "nobody writes ## Unreleased; the release generates its section")
        self.assertRegex(titles[0], release.RELEASE_RE, "the inline release is the first section")
        self.assertEqual(len(titles), 2, f"one inline release, then Older releases: {titles}")
        self.assertRegex(titles[1], release.OLDER_RE, "an Older releases section follows the inline release")

    def test_the_repositorys_changelog_has_the_shape_the_release_reads(self):
        self.assert_release_shape(CHANGELOG.read_text(encoding="utf-8"))

    def test_a_generated_changelog_has_that_shape(self):
        self.assert_release_shape(release_changelog())

    def test_entries_are_not_checked(self):
        """A maintainer may still correct an entry by hand after a release."""
        self.assert_release_shape(release_changelog(["Changes in this release:", "- One change, reworded."]))

    def test_a_hand_written_unreleased_section_does_not(self):
        """The check refuses what it exists to catch, so a pass means something."""
        malformed = release_changelog().replace("## 2. [0.1.0]", "## Unreleased\n\n- A change. (PR #7)\n\n## 2. [0.1.0]")
        with self.assertRaises(self.failureException):
            self.assert_release_shape(malformed)


if __name__ == "__main__":
    unittest.main()
