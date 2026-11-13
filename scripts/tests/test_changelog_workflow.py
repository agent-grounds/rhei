#!/usr/bin/env python3
"""The scheduled workflow validates before holding. §FS-rhei-distribution.5.3

Execute its actual version and due shell steps over real git histories, with
only the forge stubbed by the existing subprocess harness. In particular, an
empty tag..HEAD range does not establish that the tag is an ancestor (#680).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import unittest

from scripts.tests.changelog_test_support import (
    AUTO_BUMP_WORKFLOW,
    RELEASE_SCRIPT,
    ScriptTestCase,
    clean_env,
    pull,
    release_changelog,
    workflow_steps,
)

TAG = "v0.1.0"
SOURCE = "crates/core.rs"
CONDITION = "steps.gate_substantive.outputs.ok == 'true'"


class AutoBumpWorkflowTests(ScriptTestCase):
    def released(self):
        repo = self.repo()
        repo.write_changelog(release_changelog())
        # Run the literal relative command from the workflow, with its imports.
        for name in (RELEASE_SCRIPT.name, "tool_lookup.py"):
            repo.write(f"scripts/{name}", RELEASE_SCRIPT.with_name(name).read_text(encoding="utf-8"))
        repo.write(SOURCE, "Before the release\n")
        before_release = repo.commit("Main before the release")
        repo.write(SOURCE, "Released\n")
        repo.commit("Release v0.1.0")
        repo.git("tag", TAG)
        return repo, before_release

    def snapshot(self, repo):
        files = {
            path.relative_to(repo.path).as_posix(): path.read_bytes()
            for path in repo.path.rglob("*")
            if path.is_file() and ".git" not in path.relative_to(repo.path).parts
        }
        return (files, repo.git("status", "--porcelain=v1", "--untracked-files=all"),
                repo.git("ls-files", "--stage"), repo.rev("HEAD"), repo.rev(TAG))

    def scheduled_gate(self, repo):
        steps = workflow_steps(AUTO_BUMP_WORKFLOW, "patch-release")
        at = next(i for i, step in enumerate(steps) if step.get("id") == "gate_substantive")
        # No earlier range shortcut may skip validation. Do not emulate Actions'
        # expression engine: require the gate to run unconditionally after setup.
        self.assertEqual([s.get("id") for s in steps[:at] if "run" in s], ["versions"])
        for step in steps[:at + 1]:
            self.assertNotIn("if", step, "scheduled validation must not be conditional")
            self.assertNotIn("continue-on-error", step, "validation failure must fail the run")
        self.assertTrue(steps[at + 1:], "the gate must guard the release steps")
        for step in steps[at + 1:]:
            self.assertEqual(step.get("if"), CONDITION)

        env = clean_env(
            PATH=f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}",
            GH_STUB_PULLS=str(self.bin / "pulls.json"),
            GH_STUB_CALLS=str(self.calls_file()),
            PYTHONDONTWRITEBYTECODE="1",
        )
        self.gate_output = self.tmp / "gate-output"
        self.gate_output.write_bytes(b"existing=value\n")
        versions = {}
        for step in [s for s in steps[:at] if "run" in s] + [steps[at]]:
            self.assertEqual(step.get("shell"), "bash")
            output = self.gate_output if step is steps[at] else self.tmp / "versions-output"
            env["GITHUB_OUTPUT"] = str(output)
            body = step["run"]
            for key, value in versions.items():
                body = body.replace("${{ steps.versions.outputs." + key + " }}", value)
            self.assertNotIn("${{", body, "unresolved workflow expression")
            # Git for Windows also provides bash; resolve it through PATH.
            bash = shutil.which("bash")
            self.assertIsNotNone(bash, "the workflow's bash shell must be available")
            result = subprocess.run(
                [bash, "--noprofile", "--norc", "-e", "-o", "pipefail", "-c", body],
                cwd=repo.path, env=env, capture_output=True, text=True,
            )
            if step is steps[at]:
                return result
            self.assertEqual(result.returncode, 0, self.report(result))
            versions = dict(line.split("=", 1) for line in output.read_text().splitlines())
            self.assertEqual(versions["current_tag"], TAG)

    def test_reset_before_tag_refuses_even_when_commit_count_is_zero(self):
        """The workflow must reach the shared validator before any hold. §FS-rhei-distribution.5.3"""
        repo, before_release = self.released()
        repo.git("reset", "--hard", before_release)
        self.assertEqual(repo.git("rev-list", "--count", f"{TAG}..HEAD").strip(), "0")
        self.assertEqual(repo.git("merge-base", TAG, "HEAD").strip(), before_release)
        self.assertNotEqual(repo.rev(TAG), before_release)
        repo.write("release-notes.md", "Reviewed release notes must survive\n")
        before = self.snapshot(repo)

        result = self.scheduled_gate(repo)

        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(f"error: the previous release tag {TAG} is not an ancestor of HEAD", result.stderr)
        self.assertEqual(result.stdout, "", "no successful gate or hold notice before refusal")
        self.assertEqual(self.gate_output.read_bytes(), b"existing=value\n")
        self.assertEqual(self.gh_calls(), [], "reject before any forge lookup")
        self.assertEqual(self.snapshot(repo), before, "no release, tree, index or ref changes")

    def test_head_at_tag_holds_without_forge_reads(self):
        repo, _ = self.released()
        self.assertEqual(repo.rev("HEAD"), repo.rev(TAG))
        self.assert_hold(repo)

    def test_docs_and_ci_only_hold_without_forge_reads(self):
        repo, _ = self.released()
        repo.write("docs/guide.md", "Updated docs\n")
        repo.write(".github/workflows/ci.yml", "Updated CI\n")
        repo.commit("Docs and CI only")
        self.assert_hold(repo)

    def assert_hold(self, repo):
        before = self.snapshot(repo)
        result = self.scheduled_gate(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.gate_output.read_bytes(), b"existing=value\nok=false\n")
        self.assertIn(f"::notice::Only docs/CI changes since {TAG}; skipping.", result.stdout)
        self.assertEqual(result.stderr, "")
        self.assertEqual(self.gh_calls(), [])
        self.assertEqual(self.snapshot(repo), before)

    def test_code_pull_request_makes_the_release_due(self):
        repo, _ = self.released()
        repo.write(SOURCE, "Fix from PR 11\n")
        merged = repo.commit("Merge PR 11")
        self.set_pulls({merged: [pull(11, "Fix the runner")]})
        before = self.snapshot(repo)

        result = self.scheduled_gate(repo)

        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.gate_output.read_bytes(), b"existing=value\nok=true\n")
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        self.assertEqual(len(self.gh_calls()), 1)
        self.assertIn(f"/commits/{merged}/pulls", " ".join(self.gh_calls()[0]))
        self.assertEqual(self.snapshot(repo), before)


if __name__ == "__main__":
    unittest.main()
