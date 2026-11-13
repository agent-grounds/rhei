#!/usr/bin/env python3
"""History rewrites must not silently truncate notes. §FS-rhei-distribution.5.1

Real git topology and a stub forge reproduce #680 offline. The scheduled gate
must validate even identical endpoint trees (§FS-rhei-distribution.5.3).
"""

from __future__ import annotations

import re
import unittest

from scripts.tests.changelog_test_support import REPO_ROOT, RELEASE_SCRIPT, ScriptTestCase, pull, release_changelog

TAG = "v0.1.0"
SOURCE = "crates/rhei-cli/src/main.rs"
OTHER_SOURCE = "crates/rhei-core/src/lib.rs"


class AncestryTests(ScriptTestCase):
    def history(self, rewritten=False, identical=False):
        repo = self.repo()
        repo.write("README.md", "Before the release\n")
        base = repo.commit("Common ancestor")
        repo.write_changelog(release_changelog())
        repo.write("docs/changelog/0.0.9.md", "# 0.0.9 - 2026-09-01\n\nBefore it.\n")
        repo.write("Cargo.toml", 'version = "0.1.0"\n')
        repo.write(SOURCE, "released\n")
        released = repo.commit("Release v0.1.0")
        repo.git("tag", TAG)
        repo.write("Cargo.toml", 'version = "0.1.1-dev"\n')
        dev = repo.commit("Advance the development version directly")
        repo.write(SOURCE, "Remove the States line\n")
        older = repo.commit("Merge PR 11")
        if rewritten:
            # Same patches, new identities; the forge retains only the originals.
            repo.git("reset", "--hard", base)
            for sha in (released, dev, older):
                repo.git("cherry-pick", "--no-commit", sha)
                repo.commit(f"Rewritten copy of {sha}")
        repo.write(OTHER_SOURCE, "Point the spend grade at its issue\n")
        newer = repo.commit("Merge PR 12 after any rewrite")
        self.set_pulls({
            older: [pull(11, "Remove the States line")],
            newer: [pull(12, "Point the spend grade at its issue")],
        })
        if identical:
            repo.git("read-tree", "--reset", "-u", TAG)
            repo.commit("Restore the tagged tree on the rewritten line")
            self.assertEqual(repo.rev("HEAD^{tree}"), repo.rev(f"{TAG}^{{tree}}"))
        self.assertEqual(repo.git("merge-base", TAG, "HEAD").strip(), base if rewritten else released)
        return repo, dev

    def snapshot(self, repo):
        """All working files, including untracked files, plus git's index/status."""
        files = {
            path.relative_to(repo.path).as_posix(): path.read_bytes()
            for path in repo.path.rglob("*")
            if path.is_file() and ".git" not in path.relative_to(repo.path).parts
        }
        return files, repo.git("status", "--porcelain=v1", "--untracked-files=all"), repo.git("ls-files", "--stage")

    def invoke(self, repo, command):
        self.step_output = self.tmp / "github_output"
        self.step_output.write_bytes(b"existing=value\n")
        args = (["prepare", "0.2.0", "--date", "2026-10-12"] if command == "prepare" else
                ["due", TAG, "--output", str(self.step_output)])
        return self.run_script(RELEASE_SCRIPT, args, repo)

    def assert_refusal(self, repo, command):
        # Sentinels also catch clobbering of existing untracked release artifacts.
        repo.write("release-notes.md", "Reviewed notes must survive\n")
        before = self.snapshot(repo)
        result = self.invoke(repo, command)
        for check in ("exit", "diagnostic", "forge", "tree", "output"):
            with self.subTest(check=check):
                if check == "exit":
                    self.assertEqual(result.returncode, 1, self.report(result))
                elif check == "diagnostic":
                    self.assertIn("error:", result.stderr, self.report(result))
                    self.assertIn(TAG, result.stderr, self.report(result))
                    self.assertRegex(result.stderr.lower(), r"ancestor|ancestry", self.report(result))
                elif check == "forge":
                    self.assertEqual(self.gh_calls(), [], "invalid ancestry must be rejected before forge lookups")
                elif check == "tree":
                    self.assertEqual(self.snapshot(repo), before, "a refusal leaves every working file and index alone")
                else:
                    self.assertEqual(self.step_output.read_bytes(), b"existing=value\n")

    def test_prepare_refuses_an_off_history_tag_without_writes_or_forge_reads(self):
        repo, _ = self.history(rewritten=True)
        self.assert_refusal(repo, "prepare")

    def test_due_refuses_an_off_history_tag_without_output_or_forge_reads(self):
        """The scheduled release shares the range prerequisite. §FS-rhei-distribution.5.3"""
        repo, _ = self.history(rewritten=True)
        self.assert_refusal(repo, "due")

    def test_due_refuses_an_off_history_tag_even_with_identical_endpoint_trees(self):
        """No path difference does not validate the range. §FS-rhei-distribution.5.3"""
        repo, _ = self.history(rewritten=True, identical=True)
        self.assert_refusal(repo, "due")

    def test_ancestral_control_lists_both_prs_and_only_warns_for_the_dev_advance(self):
        repo, dev = self.history()
        before = self.snapshot(repo)
        due = self.invoke(repo, "due")
        self.assertEqual(due.returncode, 0, self.report(due))
        self.assertEqual(self.step_output.read_bytes(), b"existing=value\nok=true\n")
        self.assertEqual(self.snapshot(repo), before)
        prepared = self.invoke(repo, "prepare")
        self.assertEqual(prepared.returncode, 0, self.report(prepared))
        for result in (due, prepared):
            self.assertEqual(result.stderr.splitlines(), [
                f"warning: commit {dev[:7]} changed code but belongs to no pull request; it is not in the notes"
            ], self.report(result))
        self.assertEqual(re.findall(r"\(PR #(\d+)\)", repo.read_changelog().split("## 3.")[0]), ["12", "11"])


class HistoricalNotesTests(unittest.TestCase):
    """Correct the published 0.6.1 record, without regenerating it. §FS-rhei-distribution.5.1"""

    ORIGINAL = [488, 486, 485, 482, 481, 478, 477, 473, 472, 468, 469, 467, 463, 461]
    MISSING = [440, 441, 442, 443, 444, 445, 446, 449, 452, 453, 456, 457, 458, 459]

    def test_0_6_1_archive_retains_originals_and_restores_exact_missing_set_once(self):
        text = (REPO_ROOT / "docs/changelog/0.6.1.md").read_text(encoding="utf-8")
        numbers = [int(n) for n in re.findall(r"\(PR #(\d+)\)", text)]
        self.assertEqual(sorted(numbers), sorted(self.ORIGINAL + self.MISSING),
                         f"missing PRs: {sorted(set(self.ORIGINAL + self.MISSING) - set(numbers))}")
        self.assertEqual(len(numbers), 28)
        for number in numbers:
            self.assertIsNotNone(re.search(
                rf"^- \[.+\]\(https://github.com/agent-grounds/rhei/pull/{number}\) \(PR #{number}\)$", text, re.M
            ), f"PR #{number} must retain a linked title")

    def test_0_6_1_archive_index_reports_28_pull_requests(self):
        text = (REPO_ROOT / "docs/changelog.md").read_text(encoding="utf-8")
        line = next(line for line in text.splitlines() if line.startswith("- [0.6.1]"))
        self.assertEqual(line, "- [0.6.1](changelog/0.6.1.md) - 2026-10-12: 28 pull requests.")


if __name__ == "__main__":
    unittest.main()
