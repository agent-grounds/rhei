#!/usr/bin/env python3
"""What the hooks and the sweep declare. §AR-ci-release.2.1, §AR-ci-release.5

Read from the files, so all three platforms prove them. The one exception runs
the sweep's own step under bash with a stub lychee: that step runs on
`ubuntu-latest` and nowhere else, so it needs no leg on Windows.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.tests.changelog_test_support import CI_WORKFLOW, REPO_ROOT, _install, clean_env, workflow_steps
from scripts.tests.link_gate_test_support import (
    STAGED_HOOK,
    STAGED_SCRIPT,
    SWEEP_WORKFLOW,
    WHOLE_TREE_HOOK,
    named_inputs,
    read_hooks,
    sweep_step_running_lychee,
    sweep_steps,
)

SWEEP_TITLE = "Nightly link sweep: external links failing"


class RequiredJobTests(unittest.TestCase):
    def test_the_required_job_still_runs_the_link_hooks(self):
        """Passes today. A `SKIP` naming one takes the internal links out of the required job (§AR-ci-release.1)."""
        steps = [step for step in workflow_steps(CI_WORKFLOW, "lint") if "pre-commit run" in step.get("run", "")]
        self.assertEqual(len(steps), 1, "the job should run the hooks once")
        self.assertIn("--all-files", steps[0]["run"])
        skipped = re.search(r"SKIP:\s*(\S+)", steps[0]["raw"])
        self.assertIsNotNone(skipped, "the job skips the cargo hooks by name")
        self.assertNotIn("lychee", skipped.group(1))


class LinkHookTests(unittest.TestCase):
    def test_the_whole_tree_hook_asks_no_host(self):
        entry = read_hooks()[WHOLE_TREE_HOOK]["entry"].split()
        for flag in ("--offline", "--include-fragments", "--no-progress"):
            self.assertIn(flag, entry)

    def test_the_staged_hook_runs_the_script_at_commit_time_only(self):
        hook = read_hooks()[STAGED_HOOK]
        entry = hook["entry"].split()
        self.assertEqual(entry[0], "python")
        self.assertIn(STAGED_SCRIPT.relative_to(REPO_ROOT).as_posix(), entry)
        self.assertEqual(hook["pass_filenames"], "false")
        self.assertEqual(hook["stages"], "[pre-commit]")

    def test_the_two_hooks_and_the_sweep_name_the_same_inputs(self):
        """So none of the three can narrow alone."""
        hooks = read_hooks()
        whole_tree = named_inputs(hooks[WHOLE_TREE_HOOK]["entry"])
        self.assertTrue(whole_tree, "the whole-tree hook names no input")
        for name in whole_tree:
            self.assertTrue((REPO_ROOT / name).exists(), f"{name} is not in the repository")
        self.assertEqual(named_inputs(hooks[STAGED_HOOK]["entry"]), whole_tree)
        passes = [line for line in sweep_step_running_lychee()["run"].splitlines() if "--include-fragments" in line]
        self.assertTrue(passes, "the sweep's step names no command")
        for line in passes:
            self.assertEqual(named_inputs(line), whole_tree, line)


def triggers(workflow: Path) -> dict[str, str]:
    """The keys under a workflow's `on:`, each with the lines nested below it."""
    lines = workflow.read_text(encoding="utf-8").splitlines()
    start = lines.index("on:")
    found: dict[str, list[str]] = {}
    current = None
    for line in lines[start + 1 :]:
        if line and not line.startswith(" "):
            break
        key = re.match(r"^  ([a-z_]+):", line)
        if key:
            current = found.setdefault(key.group(1), [])
        elif current is not None:
            current.append(line)
    return {key: "\n".join(body) for key, body in found.items()}


class SweepTests(unittest.TestCase):
    def test_it_runs_on_a_schedule_and_by_hand_and_on_nothing_else(self):
        """No pull request and no required check depends on it."""
        found = triggers(SWEEP_WORKFLOW)
        self.assertEqual(sorted(found), ["schedule", "workflow_dispatch"])
        self.assertRegex(found["schedule"], r"""cron:\s*["']\d+ \d+ \* \* \*["']""")

    def test_it_runs_the_whole_tree_command(self):
        passes = [line for line in sweep_step_running_lychee()["run"].splitlines() if "--include-fragments" in line]
        self.assertTrue(passes)
        for line in passes:
            self.assertIn("lychee --no-progress --include-fragments", line)
            self.assertNotIn("--offline", line)

    def test_it_runs_the_lychee_ci_pins(self):
        pin = re.search(r"LYCHEE_VERSION:\s*(\S+)", CI_WORKFLOW.read_text(encoding="utf-8"))
        self.assertIsNotNone(pin, "ci.yml pins no lychee")
        version = re.escape(pin.group(1))
        self.assertRegex(SWEEP_WORKFLOW.read_text(encoding="utf-8"), rf"LYCHEE_VERSION:\s*{version}\b|lychee --version {version}\b")

    def test_it_gives_lychee_the_token_for_the_rate_limit(self):
        self.assertRegex(sweep_step_running_lychee()["raw"], r"GITHUB_TOKEN:\s*\$\{\{\s*secrets\.GITHUB_TOKEN\s*\}\}")

    def test_a_failing_verdict_files_one_issue_and_then_comments_on_it(self):
        self.assertRegex(SWEEP_WORKFLOW.read_text(encoding="utf-8"), r"(?m)^\s*issues:\s*write\s*$")
        steps = [step for step in sweep_steps() if "gh issue create" in step.get("run", "")]
        self.assertEqual(len(steps), 1, "one step should file the failure")
        self.assertIn("failure()", steps[0].get("if", ""), "only a failing verdict files anything")
        for needed in (SWEEP_TITLE, "gh issue list", "gh issue comment", "--label bug"):
            self.assertIn(needed, steps[0]["raw"])


# The exit codes of successive passes; the last repeats.
SWEEP_LYCHEE_STUB = r"""
import os, sys

calls = os.environ["SWEEP_STUB_LYCHEE_CALLS"]
with open(calls, "a", encoding="utf-8") as handle:
    handle.write(" ".join(sys.argv[1:]) + "\n")
with open(calls, encoding="utf-8") as handle:
    count = len(handle.readlines())
exits = [int(code) for code in os.environ["SWEEP_STUB_LYCHEE_EXITS"].split(",")]
sys.exit(exits[min(count, len(exits)) - 1])
"""

SWEEP_SLEEP_STUB = r"""
import os, sys

with open(os.environ["SWEEP_STUB_SLEEP_CALLS"], "a", encoding="utf-8") as handle:
    handle.write(" ".join(sys.argv[1:]) + "\n")
"""


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "the sweep's step runs on ubuntu-latest, under bash")
class SweepVerdictTests(unittest.TestCase):
    """The step itself, as GitHub runs it: a 5xx that passes by files nothing (§AR-ci-release.5)."""

    def setUp(self) -> None:
        holder = tempfile.mkdtemp(prefix="link-sweep-")
        self.tmp = Path(holder)
        self.addCleanup(shutil.rmtree, holder, True)
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        _install(self.bin, "lychee", SWEEP_LYCHEE_STUB)
        _install(self.bin, "sleep", SWEEP_SLEEP_STUB)
        self.lychee_calls = self.tmp / "lychee-calls"
        self.sleep_calls = self.tmp / "sleep-calls"

    def run_step(self, exits: str) -> subprocess.CompletedProcess[str]:
        script = self.tmp / "step.sh"
        script.write_text(sweep_step_running_lychee()["run"], encoding="utf-8")
        for name in ("output", "summary"):
            (self.tmp / name).touch()
        return subprocess.run(
            ["bash", "--noprofile", "--norc", "-eo", "pipefail", str(script)],
            cwd=str(self.tmp),
            capture_output=True,
            text=True,
            env=clean_env(
                PATH=f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}",
                RUNNER_TEMP=str(self.tmp),
                GITHUB_OUTPUT=str(self.tmp / "output"),
                GITHUB_STEP_SUMMARY=str(self.tmp / "summary"),
                SWEEP_STUB_LYCHEE_EXITS=exits,
                SWEEP_STUB_LYCHEE_CALLS=str(self.lychee_calls),
                SWEEP_STUB_SLEEP_CALLS=str(self.sleep_calls),
            ),
            timeout=60,
        )

    def passes(self) -> int:
        return len(self.lychee_calls.read_text(encoding="utf-8").splitlines())

    def pauses(self) -> int:
        return len(self.sleep_calls.read_text(encoding="utf-8").splitlines()) if self.sleep_calls.exists() else 0

    def test_a_first_pass_that_succeeds_is_the_verdict(self):
        result = self.run_step("0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.passes(), self.pauses()), (1, 0))

    def test_a_failure_that_passes_by_is_run_again_after_a_pause_and_succeeds(self):
        result = self.run_step("2,0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.passes(), 2)
        self.assertGreaterEqual(self.pauses(), 1)

    def test_a_second_failure_is_the_verdict(self):
        result = self.run_step("2,2")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.passes(), 2)
        self.assertGreaterEqual(self.pauses(), 1)


if __name__ == "__main__":
    unittest.main()
