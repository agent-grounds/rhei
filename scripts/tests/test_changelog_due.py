#!/usr/bin/env python3
"""When the scheduled release is due, and what a release refuses. §FS-rhei-distribution.5.3

`prepare_changelog_release.py due <tag>` is `Auto bump`'s gate step
(§AR-ci-release.3). It answers on the step's `ok` output and it holds - green,
with a notice - while code has merged since `## Unreleased` was last written,
because no change adds a bullet of its own any more and a release cut before
the write-up would ship code the section does not describe. Every history is a
`TempRepo` under the test's own temporary directory (§REQ-test-isolation.1),
with every inherited `GIT_*` variable stripped (§REQ-test-isolation.3), so the
tests run unchanged on all three platforms (§REQ-cross-platform.3).
"""

from __future__ import annotations

import re
import unittest

from scripts.tests.changelog_test_support import (
    AUTO_BUMP_WORKFLOW,
    RELEASE_MINOR_WORKFLOW,
    STAMPER as RELEASE_SCRIPT,
    ScriptTestCase,
    release_changelog,
    workflow_steps,
)

TAG = "v0.1.0"
SOURCE = "crates/rhei-cli/src/main.rs"
EARLIER_SOURCE = "crates/rhei-cli/src/lib.rs"

NOTE = "*Every pull request adds a bullet under `## Unreleased`.*"
REWORDED_NOTE = "*`## Unreleased` is written before a release; see CONTRIBUTING.md.*"


def hold_notice(last_written: str) -> str:
    return (
        f"::notice::Changes merged since ## Unreleased was last written ({last_written[:7]}) "
        "wait for their release section; skipping."
    )


DOCS_NOTICE = f"::notice::Only docs/CI changes since {TAG}; skipping."


class DueTests(ScriptTestCase):
    """The five histories of the proposal, each read by `due` as the workflow would."""

    def setUp(self) -> None:
        super().setUp()
        self.github_output = self.tmp / "github_output"
        self.github_output.write_text("", encoding="utf-8")

    def released(self, note: str | None = None):
        """A repo whose tag sits on its release commit, which empties `Unreleased`."""
        repo = self.repo()
        repo.write_changelog(release_changelog([], note=note))
        repo.write("Cargo.toml", 'version = "0.1.0"\n')
        repo.write(EARLIER_SOURCE, "released\n")
        tagged = repo.commit("Release v0.1.0")
        repo.git("tag", TAG)
        return repo, tagged

    def due(self, repo):
        return self.run_script(RELEASE_SCRIPT, ["due", TAG, "--output", str(self.github_output)], repo)

    def ok(self) -> list[str]:
        """Every `ok=` line `due` wrote for the step, which should be exactly one."""
        lines = self.github_output.read_text(encoding="utf-8").splitlines()
        return [line for line in lines if line.startswith("ok=")]

    def notices(self, result) -> list[str]:
        return [line for line in result.stdout.splitlines() if line.startswith("::notice::")]

    def test_code_then_its_write_up_is_due(self):
        repo, _ = self.released()
        repo.write(SOURCE)
        repo.commit("the change itself")
        repo.write_changelog(release_changelog(["- The change itself. (PR #7)"]))
        repo.commit("Write the release section")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=true"], self.report(result))
        self.assertNotIn("wait for their release section", result.stdout)

    def test_code_merged_after_the_write_up_holds(self):
        """The ticket's case: the workflow would release code no bullet describes."""
        repo, _ = self.released()
        repo.write(EARLIER_SOURCE)
        repo.commit("an earlier change")
        repo.write_changelog(release_changelog(["- An earlier change. (PR #7)"]))
        written = repo.commit("Write the release section")
        repo.write(SOURCE)
        repo.commit("a change merged after the write-up")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, f"a hold ends green\n{self.report(result)}")
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertIn(hold_notice(written), self.notices(result), self.report(result))
        self.assertIn(SOURCE, result.stdout, "the notice lists what is waiting")
        self.assertNotIn(EARLIER_SOURCE, result.stdout, "only what merged after the write-up waits")

    def test_the_dev_advance_after_a_release_holds(self):
        """Today this reaches `prepare` with nothing pending and the run goes red."""
        repo, tagged = self.released()
        repo.write("Cargo.toml", 'version = "0.1.1-dev"\n')
        repo.commit("Open 0.1.1-dev for development")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertIn(hold_notice(tagged), self.notices(result), "the tag counts as the last write")
        self.assertIn("Cargo.toml", result.stdout)

    def test_only_docs_and_ci_since_the_tag_is_not_due(self):
        """The filter the workflow ran inline, moved into the script unchanged."""
        repo, _ = self.released()
        for path in ("docs/guide.md", ".github/workflows/ci.yml", "README.md", "crates/rhei-cli/README.md",
                     "LICENSE", "lychee.toml"):
            repo.write(path)
        repo.commit("docs and CI only")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertIn(DOCS_NOTICE, self.notices(result), self.report(result))
        self.assertNotIn("wait for their release section", result.stdout)

    def test_an_edit_to_the_note_above_the_section_is_not_a_write(self):
        repo, tagged = self.released(note=NOTE)
        repo.write(SOURCE)
        repo.commit("the change itself")
        repo.write_changelog(release_changelog([], note=REWORDED_NOTE))
        repo.commit("Reword the changelog's note")

        result = self.due(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.ok(), ["ok=false"], self.report(result))
        self.assertIn(hold_notice(tagged), self.notices(result), self.report(result))
        self.assertIn(SOURCE, result.stdout)


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


class PrepareRefusalTests(ScriptTestCase):
    """What `Release minor` meets when nobody wrote the section. §FS-rhei-distribution.5.3"""

    def test_prepare_refuses_an_empty_section_and_says_to_write_it(self):
        repo = self.repo()
        repo.write_changelog(release_changelog([]))
        repo.commit("Release v0.1.0")
        before = repo.read_changelog()

        result = self.run_script(RELEASE_SCRIPT, ["prepare", "0.1.1", "--date", "2026-10-05"], repo)
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(
            "## Unreleased has no bullet entries to promote; write the release section first",
            result.stderr,
        )
        self.assertEqual(repo.read_changelog(), before, "a refusal writes nothing")
        self.assertFalse((repo.path / "docs" / "changelog" / "0.1.0.md").exists(), "nor archives anything")


if __name__ == "__main__":
    unittest.main()
