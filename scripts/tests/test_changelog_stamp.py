#!/usr/bin/env python3
"""The release stamping the numbers the contributor did not know. §FS-rhei-distribution.5.2

`prepare_changelog_release.py stamp` runs before `prepare`, over the bullets
where their authors left them, and its whole point is that it is allowed to
achieve nothing: a release that cannot be cut over a changelog annotation costs
more than the annotation is worth (§AR-ci-release.3).
"""

from __future__ import annotations

import unittest

from scripts.tests.changelog_test_support import (
    STAMPER,
    ScriptTestCase,
    changelog,
    path_with_no_gh,
)

class ChangelogStampTests(ScriptTestCase):
    def stamp(self, repo, path: str | None = None):
        return self.run_script(STAMPER, ["stamp"], repo, path=path)

    def repo_with(self, bullets: list[str], pulls_for_head: list[int] | None = (339,)):
        """A repo whose one changelog commit resolves to `pulls_for_head`."""
        repo = self.repo()
        repo.write_changelog(changelog(bullets))
        sha = repo.commit("write the changelog bullets")
        self.set_pulls({sha: list(pulls_for_head or [])})
        return repo, sha

    # --- what it writes -----------------------------------------------------

    def test_it_replaces_the_placeholder_where_it_stands(self):
        repo, _ = self.repo_with(["- The change this branch made. (PR #TBD)"])

        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        after = repo.read_changelog()
        self.assertIn("- The change this branch made. (PR #339)", after)
        self.assertNotIn("TBD", after, "a placeholder is replaced, not followed by a second token")

    def test_it_appends_a_number_where_there_is_no_placeholder(self):
        repo, _ = self.repo_with(["- The change this branch made."])

        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertIn("- The change this branch made. (PR #339)", repo.read_changelog())

    def test_it_leaves_a_bullet_that_already_carries_a_number(self):
        repo, _ = self.repo_with(["- An earlier change. (PR #12)"])

        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertIn("- An earlier change. (PR #12)", repo.read_changelog())
        self.assertNotIn("left unstamped", self.output(result), "nothing to report about it")

    # --- what it refuses to guess -------------------------------------------

    def test_it_leaves_a_bullet_whose_lines_belong_to_two_pull_requests(self):
        repo = self.repo()
        repo.write_changelog(changelog(["- The change this branch made."]))
        first = repo.commit("write the bullet")
        repo.write_changelog(
            changelog(["- The change this branch made.\n  And a second line about it."])
        )
        second = repo.commit("add a line to the bullet")
        self.set_pulls({first: [339], second: [340]})

        before = repo.read_changelog()
        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(repo.read_changelog(), before, "neither pull request owns it")
        self.assertIn("left unstamped", self.output(result))

    def test_it_leaves_a_bullet_with_a_line_that_resolves_to_nothing(self):
        repo, _ = self.repo_with(["- The change this branch made."], pulls_for_head=[])

        before = repo.read_changelog()
        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(repo.read_changelog(), before)
        self.assertIn("left unstamped", self.output(result))

    # --- one number, one bullet ---------------------------------------------

    GUARDED = "left unstamped (PR #500 would go into 2 bullets; write each its own (PR #500))"

    def test_a_number_two_bullets_resolve_to_goes_into_neither(self):
        """A write-up merged as one pull request is not credited with the release.

        Both bullets were written in one commit, which resolves to #500, so today
        each becomes `(PR #500)` (§FS-rhei-distribution.5.2).
        """
        repo, _ = self.repo_with(["- The first change.", "- The second change."], pulls_for_head=[500])

        before = repo.read_changelog()
        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, "stamping never fails a release")
        self.assertEqual(repo.read_changelog(), before, "neither bullet is stamped")
        self.assertEqual(self.output(result).count(self.GUARDED), 2, self.report(result))

    def test_a_placeholder_counts_as_no_number(self):
        repo, _ = self.repo_with(["- The first change. (PR #TBD)", "- The second change."], pulls_for_head=[500])

        before = repo.read_changelog()
        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(repo.read_changelog(), before, "the placeholder stays where it stands")
        self.assertEqual(self.output(result).count(self.GUARDED), 2, self.report(result))

    def test_a_bullet_already_carrying_the_number_is_not_counted(self):
        """Passes today, and must keep passing: a lone unnumbered bullet is stamped."""
        repo, _ = self.repo_with(["- The first change. (PR #500)", "- The second change."], pulls_for_head=[500])

        result = self.stamp(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        after = repo.read_changelog()
        self.assertIn("- The first change. (PR #500)", after)
        self.assertIn("- The second change. (PR #500)", after)
        self.assertNotIn("would go into", self.output(result))

    def test_it_warns_and_writes_nothing_with_no_gh_on_path(self):
        repo, _ = self.repo_with(["- The change this branch made."])

        before = repo.read_changelog()
        result = self.stamp(repo, path=path_with_no_gh())
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(repo.read_changelog(), before)
        self.assertIn("gh", self.output(result), "the warning names the cause")


if __name__ == "__main__":
    unittest.main()
