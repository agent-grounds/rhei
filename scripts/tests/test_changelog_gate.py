#!/usr/bin/env python3
"""The changelog pull-request check, both halves of it. §FS-rhei-distribution.5.1

One rule over one file, run by the pre-push hook (§FS-rhei-distribution.6) and
by CI (§AR-ci-release.1). What these tests exist to stop is the two halves
disagreeing: every case here asserts both invocations where both have an answer,
because agent-grounds/rhei#341 is exactly the state where the local one passed
and the remote one refused.
"""

from __future__ import annotations

import unittest

from scripts.tests.changelog_test_support import (
    GATE,
    ScriptTestCase,
    changelog,
    checkout_fetch_depth,
    hook_stages,
)

# A bullet already in the section before the branch exists. The section is not
# empty, so "is there an Unreleased section at all" is not what is under test.
EARLIER = "- An earlier, unrelated change that is already recorded here. (PR #1)"

WRAPPED = (
    "- A bullet that is long enough to be wrapped over two lines, which is how\n"
    "  every bullet in this changelog is written."
)
REWRAPPED = (
    "- A bullet that is long enough to be wrapped over two lines,\n"
    "  which is how every bullet in this changelog is written."
)
REWORDED = (
    "- A bullet that is long enough to be wrapped across two lines, which is how\n"
    "  every bullet in this changelog is written."
)

NO_PULL_REQUEST = "not a pull_request event"


class ChangelogGateTests(ScriptTestCase):
    def branch_off(self, unreleased: list[str], branch: str = "fix/the-thing"):
        """A repo whose `main` carries these bullets, with a branch checked out."""
        repo = self.repo()
        repo.write_changelog(changelog(unreleased))
        base = repo.commit("the base of the branch")
        repo.git("checkout", "-b", branch)
        return repo, base

    def pre_push(self, repo, *args: str, **env: str | None):
        """The hook's own invocation: `--local-pr`, base resolved by the script."""
        return self.run_script(GATE, ["--local-pr", *args], repo, **env)

    def ci(self, repo, pr_number: int, base: str, *args: str):
        """`ci.yml`'s invocation: the number and the base commit."""
        return self.run_script(
            GATE, ["--pr-number", str(pr_number), "--base-ref", base, *args], repo
        )

    # --- the reproducer, ported ----------------------------------------------

    def test_both_halves_refuse_a_section_with_no_bullet_for_the_change(self):
        """The ticket itself: the local gate must refuse what CI refuses."""
        repo, base = self.branch_off([EARLIER])
        repo.touch_source("src.txt")
        repo.commit("a change with no changelog bullet")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 1, self.report(local))
        self.assertIn("no new or changed bullet", local.stderr)
        self.assertIn("SKIP=changelog-pr-entry", local.stderr)

        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 1, self.report(remote))
        self.assertIn("no new or changed bullet", remote.stderr)
        self.assertNotIn("SKIP=", remote.stderr, "there is nothing to skip on a pull request")

    def test_a_bullet_with_no_number_passes_both_halves(self):
        repo, base = self.branch_off([EARLIER])
        repo.write_changelog(changelog([EARLIER, "- The change this branch makes."]))
        repo.commit("a change with a numberless changelog bullet")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))
        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 0, self.report(remote))

    # --- what counts as a new or changed bullet ------------------------------

    def test_a_rewrapped_bullet_is_not_a_new_bullet(self):
        repo, _ = self.branch_off([EARLIER, WRAPPED])
        repo.write_changelog(changelog([EARLIER, REWRAPPED]))
        repo.commit("rewrap a bullet somebody else wrote")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 1, self.report(local))
        self.assertIn("no new or changed bullet", local.stderr)

    def test_a_reworded_bullet_is_a_changed_bullet(self):
        repo, base = self.branch_off([EARLIER, WRAPPED])
        repo.write_changelog(changelog([EARLIER, REWORDED]))
        repo.commit("reword a bullet")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))
        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 0, self.report(remote))

    def test_writing_your_number_onto_somebody_elses_bullet_is_not_a_bullet(self):
        borrowed = "- Somebody else's change, recorded before this branch. (PR #338)"
        repo, base = self.branch_off([EARLIER, borrowed])
        repo.write_changelog(
            changelog([EARLIER, borrowed.replace("(PR #338)", "(PR #341)")])
        )
        repo.commit("borrow a bullet by writing my number on it")

        remote = self.ci(repo, 341, base)
        self.assertEqual(remote.returncode, 1, self.report(remote))
        self.assertIn("no new or changed bullet", remote.stderr)

    def test_the_base_is_the_merge_base_not_the_base_branch_tip(self):
        """A release promotion on `main` must not make untouched bullets read as new."""
        repo, _ = self.branch_off([EARLIER, WRAPPED])
        repo.touch_source("src.txt")
        repo.commit("a change with no changelog bullet")

        repo.git("checkout", "main")
        repo.write_changelog(
            changelog([], released=f"{EARLIER}\n\n{WRAPPED}")
        )
        repo.commit("promote Unreleased into a release section")
        repo.git("checkout", "fix/the-thing")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 1, self.report(local))
        self.assertIn("no new or changed bullet", local.stderr)

    def test_a_stale_origin_main_does_not_decide_the_base(self):
        """A fork's default branch left behind must not make merged-in bullets ours.

        The ordinary way a contributor brings a branch up to date is to merge
        upstream in without first syncing their fork's `main`. Reading the base
        off whichever candidate resolves first then counts the bullets that merge
        brought along as this branch's own, and R2 refuses them for carrying
        somebody else's number - a state CI accepts on the identical tree.
        """
        theirs = "- Somebody else's newer change, merged in from upstream. (PR #338)"
        repo, branch_point = self.branch_off([EARLIER])
        repo.git("update-ref", "refs/remotes/origin/main", branch_point)

        repo.git("checkout", "-b", "as-upstream-has-it")
        repo.write_changelog(changelog([EARLIER, theirs]))
        upstream = repo.commit("upstream main gains somebody else's bullet")
        repo.git("update-ref", "refs/remotes/upstream/main", upstream)

        repo.git("checkout", "fix/the-thing")
        repo.git("branch", "-D", "as-upstream-has-it")
        repo.touch_source("src.txt")
        repo.commit("the change itself")
        repo.git("merge", "--no-edit", "-m", "merge upstream main", upstream)
        repo.write_changelog(changelog([EARLIER, theirs, "- The change this branch makes."]))
        repo.commit("my own bullet")

        local = self.pre_push(repo, GH_STUB_PR="339")
        self.assertEqual(local.returncode, 0, self.report(local))
        self.assertNotIn("PR #338", self.output(local), "that bullet is not this branch's")
        remote = self.ci(repo, 339, upstream)
        self.assertEqual(remote.returncode, 0, self.report(remote))

    def test_a_named_base_the_checkout_does_not_hold_is_refused(self):
        """A shallow clone must fail loudly rather than degrade to passing everything.

        The degrade of §FS-rhei-distribution.6 is for a base the hook could not
        find. A base the caller named is its own claim about its checkout, and CI
        passing one it cannot reach is how a green gate comes to check nothing.
        """
        repo, _ = self.branch_off([EARLIER])
        repo.touch_source("src.txt")
        repo.commit("a change with no changelog bullet")
        absent = "4e1410270d3dca4b2908f2e3963ff495fcd8dfb9"

        remote = self.ci(repo, 339, absent)
        self.assertNotEqual(remote.returncode, 0, self.report(remote))
        self.assertIn(absent, remote.stderr, "it must name the ref it was given")
        self.assertNotIn(
            "checking only that",
            self.output(remote),
            "a named base that is missing is not a reason to check less",
        )

    # --- the number, where a number is known ---------------------------------

    def test_a_new_bullet_carrying_another_pull_requests_number_is_refused(self):
        repo, base = self.branch_off([EARLIER])
        repo.write_changelog(changelog([EARLIER, "- The change this branch makes. (PR #338)"]))
        repo.commit("a new bullet with the wrong number on it")

        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 1, self.report(remote))
        self.assertIn("PR #338", remote.stderr)
        self.assertIn("not PR #339", remote.stderr)

    def test_a_number_in_a_bullets_prose_is_not_the_number_it_carries(self):
        """A revert may name what it reverses: the number checked is the trailing token."""
        repo, base = self.branch_off([EARLIER])
        repo.write_changelog(
            changelog(
                [EARLIER, "- Revert the behaviour introduced in PR #338, which broke nested runs."]
            )
        )
        repo.commit("a bullet whose prose names another pull request")

        local = self.pre_push(repo, GH_STUB_PR="339")
        self.assertEqual(local.returncode, 0, self.report(local))
        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 0, self.report(remote))

    def test_a_written_number_is_accepted_where_no_number_is_knowable(self):
        """Passes today, and must keep passing: the hook has no oracle for a number."""
        repo, _ = self.branch_off([EARLIER])
        repo.write_changelog(changelog([EARLIER, "- The change this branch makes. (PR #338)"]))
        repo.commit("a new bullet with a number the hook cannot check")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))

    def test_the_placeholder_passes_both_halves(self):
        repo, base = self.branch_off([EARLIER])
        repo.write_changelog(changelog([EARLIER, "- The change this branch makes. (PR #TBD)"]))
        repo.commit("a new bullet carrying the placeholder")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))
        remote = self.ci(repo, 339, base)
        self.assertEqual(remote.returncode, 0, self.report(remote))

    # --- the two skips that remain, each named by a condition ----------------

    def test_a_head_already_contained_in_the_base_is_skipped(self):
        """`git push origin main` after a fast-forward adds nothing to describe."""
        repo = self.repo()
        repo.write_changelog(changelog([EARLIER]))
        repo.commit("the state of main")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))
        self.assertNotIn(
            NO_PULL_REQUEST,
            self.output(local),
            "the absent pull request is no longer a reason to skip; being contained in the base is",
        )

    def test_no_base_ref_degrades_to_requiring_a_bullet_and_says_so(self):
        repo = self.repo(branch="fix/the-thing")
        repo.write_changelog(changelog([EARLIER]))
        repo.commit("a branch with no main to compare against")

        local = self.pre_push(repo)
        self.assertEqual(local.returncode, 0, self.report(local))
        self.assertIn("origin/main", local.stderr, "it must say which refs it tried")
        self.assertIn("git fetch", local.stderr, "and what would let it compare properly")

        repo.write_changelog(changelog([]))
        repo.commit("empty the Unreleased section")
        empty = self.pre_push(repo)
        self.assertNotEqual(empty.returncode, 0, self.report(empty))
        self.assertIn("bullet", empty.stderr)

    # --- where the hook is registered ---------------------------------------

    def test_the_hook_is_registered_at_the_pre_push_stage_only(self):
        """Passes today, and is here so it cannot be moved without a test failing.

        At the `pre-commit` stage it would also fire inside CI's
        `pre-commit run --all-files`, with different arguments - two gates over
        one file, which is the defect (§AR-ci-release.2).
        """
        self.assertEqual(hook_stages("changelog-pr-entry"), ["pre-push"])

    def test_the_lint_jobs_checkout_holds_the_whole_history(self):
        """CI's half is passed a base commit, so its clone has to hold one.

        `actions/checkout` takes one commit by default, which leaves the base of
        every pull request outside the clone (§AR-ci-release.1).
        """
        self.assertEqual(checkout_fetch_depth("lint"), "0")


if __name__ == "__main__":
    unittest.main()
