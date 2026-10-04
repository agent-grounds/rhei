#!/usr/bin/env python3
"""When the scheduled release is due. §FS-rhei-distribution.5.3

`prepare_changelog_release.py due <tag>` is `Auto bump`'s gate step
(§AR-ci-release.3). It answers on the step's `ok` output, and the release is due
exactly when the list `prepare` would write is not empty: a pull request that
changed more than docs and CI has merged since the tag. It holds - green, with a
notice - otherwise, and fails only when the forge cannot answer. Every history
is a `TempRepo` under the test's own temporary directory
(§REQ-test-isolation.1), with every inherited `GIT_*` variable stripped
(§REQ-test-isolation.3), so the tests run unchanged on all three platforms
(§REQ-cross-platform.3).
"""

from __future__ import annotations

import re
import unittest

from scripts.tests.changelog_test_support import (
    AUTO_BUMP_WORKFLOW,
    RELEASE_MINOR_WORKFLOW,
    RELEASE_SCRIPT,
    ScriptTestCase,
    pull,
    release_changelog,
    workflow_steps,
)

TAG = "v0.1.0"
SOURCE = "crates/rhei-cli/src/main.rs"
EARLIER_SOURCE = "crates/rhei-cli/src/lib.rs"

DOCS_NOTICE = f"::notice::Only docs/CI changes since {TAG}; skipping."
NO_PULL_NOTICE = f"::notice::No pull request that changed code merged since {TAG}; skipping."


class DueTests(ScriptTestCase):
    """Each history read by `due` as the workflow would."""

    def setUp(self) -> None:
        super().setUp()
        self.github_output = self.tmp / "github_output"
        self.github_output.write_text("", encoding="utf-8")
        self.pulls: dict[str, list[object]] = {}

    def released(self):
        """A repo whose tag sits on its release commit."""
        repo = self.repo()
        repo.write_changelog(release_changelog())
        repo.write("Cargo.toml", 'version = "0.1.0"\n')
        repo.write(EARLIER_SOURCE, "released\n")
        tagged = repo.commit("Release v0.1.0")
        repo.git("tag", TAG)
        return repo, tagged

    def land(self, repo, pr, *paths: str) -> str:
        """One commit changing `paths`, belonging to `pr` (or to no pull request)."""
        for path in paths:
            repo.write(path)
        sha = repo.commit(f"a commit of {pr and pr['number']}")
        self.pulls[sha] = [pr] if pr is not None else []
        return sha

    def due(self, repo, **env: str | None):
        self.set_pulls(self.pulls)
        return self.run_script(RELEASE_SCRIPT, ["due", TAG, "--output", str(self.github_output)], repo, **env)

    def ok(self) -> list[str]:
        """Every `ok=` line `due` wrote for the step, which should be exactly one."""
        lines = self.github_output.read_text(encoding="utf-8").splitlines()
        return [line for line in lines if line.startswith("ok=")]

    def notices(self, result) -> list[str]:
        return [line for line in result.stdout.splitlines() if line.startswith("::notice::")]

    def test_a_merged_pull_request_that_changed_code_is_due(self):
        """The ticket's case: today it holds until somebody writes `## Unreleased`."""
        repo, _ = self.released()
        self.land(repo, None, "Cargo.toml")
        self.land(repo, pull(7, "Fix the runner"), SOURCE)

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=true"], self.report(result))
        self.assertEqual(self.notices(result), [], self.report(result))

    def test_the_dev_advance_alone_holds_because_it_belongs_to_no_pull_request(self):
        repo, _ = self.released()
        self.land(repo, None, "Cargo.toml")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, f"a hold ends green\n{self.report(result)}")
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertEqual(self.notices(result), [NO_PULL_NOTICE], self.report(result))

    def test_a_docs_only_pull_request_and_the_dev_advance_hold(self):
        repo, _ = self.released()
        self.land(repo, None, "Cargo.toml")
        self.land(repo, pull(7, "Reword the guide"), "docs/guide.md")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertEqual(self.notices(result), [NO_PULL_NOTICE], self.report(result))

    def test_only_docs_and_ci_since_the_tag_holds_without_asking_the_forge(self):
        """Passes today, and must keep passing: a docs-only week costs no forge call."""
        repo, _ = self.released()
        self.land(
            repo,
            pull(7, "Docs and CI"),
            "docs/guide.md", ".github/workflows/ci.yml", "README.md", "crates/rhei-cli/README.md",
            "LICENSE", "lychee.toml",
        )

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertIn(DOCS_NOTICE, self.notices(result), self.report(result))
        self.assertEqual(self.gh_calls(), [], "the paths alone answered; the forge was not asked")

    def test_a_forge_that_cannot_answer_fails_the_run(self):
        """A list `due` could not build is not an empty one. §FS-rhei-distribution.5.2"""
        repo, _ = self.released()
        unanswered = self.land(repo, pull(7, "Fix the runner"), SOURCE)

        result = self.due(repo, GH_STUB_FAIL=unanswered)
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(
            f"error: could not ask the forge about commit {unanswered[:7]}: HTTP 502: Bad Gateway",
            result.stderr,
            self.report(result),
        )
        self.assertNotIn("ok=true", self.ok())

    def test_due_never_speaks_of_a_last_write(self):
        """The "last written" hold is gone with `## Unreleased`."""
        repo, _ = self.released()
        self.land(repo, pull(7, "Fix the runner"), SOURCE)
        self.land(repo, pull(8, "Fix the runner again"), EARLIER_SOURCE)

        result = self.due(repo)
        self.assertNotIn("last written", self.output(result), self.report(result))
        self.assertEqual(self.ok(), ["ok=true"], self.report(result))


class AutoBumpGateStepTests(unittest.TestCase):
    """`Auto bump` asks `due`, and everything after it waits on the answer. §AR-ci-release.3"""

    STEP = "Gate - non-doc/CI changes since last tag"
    CONDITION = "steps.gate_substantive.outputs.ok == 'true'"

    def steps(self):
        steps = workflow_steps(AUTO_BUMP_WORKFLOW, "patch-release")
        names = [step.get("name") for step in steps]
        self.assertIn(self.STEP, names, f"{AUTO_BUMP_WORKFLOW.name} lost its gate step")
        return steps, names.index(self.STEP)

    def test_the_gate_step_is_one_due_call(self):
        steps, at = self.steps()
        step = steps[at]
        self.assertEqual(step.get("id"), "gate_substantive", "every later step reads its output by this id")
        script = step.get("run", "")
        commands = [
            command
            for command in re.split(r"(?<!\\)\n", script)
            if command.strip() and command.strip() != "set -euo pipefail" and not command.strip().startswith("#")
        ]
        self.assertEqual(len(commands), 1, f"the step runs more than the one call:\n{script}")
        self.assertIn("scripts/prepare_changelog_release.py due ", commands[0])
        self.assertIn('--output "$GITHUB_OUTPUT"', commands[0])

    def test_every_later_step_waits_on_the_gate_steps_ok(self):
        """Passes today; the `-dev` advance holds with the release only through this."""
        steps, at = self.steps()
        later = steps[at + 1 :]
        self.assertTrue(later, "nothing follows the gate step")
        for step in later:
            self.assertEqual(step.get("if"), self.CONDITION, step.get("name") or step.get("uses"))

    def test_release_minor_does_not_ask_whether_a_release_is_due(self):
        """Passes today; a release a person starts does not hold (§FS-rhei-distribution.5.3)."""
        text = RELEASE_MINOR_WORKFLOW.read_text(encoding="utf-8")
        self.assertNotRegex(text, r"prepare_changelog_release\.py\s+due\b")


class NoStampStepTests(unittest.TestCase):
    """Neither release helper stamps; both generate the notes in `prepare`. §AR-ci-release.3"""

    def test_neither_helper_calls_stamp(self):
        for workflow in (AUTO_BUMP_WORKFLOW, RELEASE_MINOR_WORKFLOW):
            text = workflow.read_text(encoding="utf-8")
            stamp = re.search(r"prepare_changelog_release\.py\s+stamp\b", text)
            self.assertIsNone(stamp, f"{workflow.name} still calls `stamp`")
            prepare = re.search(r"prepare_changelog_release\.py\s+prepare\b", text)
            self.assertIsNotNone(prepare, f"{workflow.name} no longer calls `prepare`")


if __name__ == "__main__":
    unittest.main()
