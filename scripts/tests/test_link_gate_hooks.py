#!/usr/bin/env python3
"""A commit depends on no outside host except through a URL it stages. §AR-ci-release.2.1

Real commits through a real `pre-commit install` of the checkout's own hooks,
with a loopback stand-in as lychee's proxy, so nothing here reaches the network.
They need lychee and pre-commit, which of the CI jobs only `lint` installs, and
`lint` runs them through the `gate-script-tests` hook. `test_link_gate_scope.py`
proves the scoping itself on all three platforms.
"""

from __future__ import annotations

import os
import unittest

from scripts.tests.link_gate_test_support import (
    ADDED,
    CRAWLED,
    EVERY_LINK,
    LinkGateCase,
    OTHER,
    STEVE,
    needs_lychee_and_pre_commit,
)

COMPARISON = "docs/functional-spec/comparison.md"
RENAMED = "docs/functional-spec/renamed.md"
UNRELATED = "docs/unrelated.md"


@needs_lychee_and_pre_commit
class ControlTests(LinkGateCase):
    """What every other test here leans on, so a green one below means something."""

    def test_lychee_reaches_the_stand_in_and_reports_its_504(self):
        self.stand_in.answer(504, STEVE)
        result = self.run_in_repo("lychee", "--no-progress", COMPARISON)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(f"[504] {STEVE}", result.stdout)
        self.assertIn(STEVE, self.stand_in.asked())

    def test_a_commit_lands_with_every_link_answering(self):
        self.append(UNRELATED, "a line with no link")
        self.assert_landed(self.commit())


@needs_lychee_and_pre_commit
class AddedLinkTests(LinkGateCase):
    """Guards, passing today: an added link is still checked, against the repair that checks nothing."""

    def assert_refused_naming(self, result, url: str, path: str) -> None:
        self.assert_refused(result)
        # A proxy is sent no fragment, so the request names the page.
        self.assertIn(url.split("#")[0], self.stand_in.asked(), "the stand-in was never asked")
        self.assertIn(url, result.stdout)
        self.assertIn(path, result.stdout)

    def test_an_added_link_answering_504_refuses_and_is_named(self):
        self.stand_in.answer(504, ADDED)
        self.append(UNRELATED, f"[added]({ADDED})")
        result = self.commit()
        self.assert_refused_naming(result, ADDED, UNRELATED)
        self.assertIn("504", result.stdout)

    def test_an_added_link_answering_404_refuses_and_is_named(self):
        self.stand_in.answer(404, ADDED)
        self.append(UNRELATED, f"[added]({ADDED})")
        result = self.commit()
        self.assert_refused_naming(result, ADDED, UNRELATED)
        self.assertIn("404", result.stdout)

    def test_a_link_added_by_commit_all_is_asked_about(self):
        """`git commit -a` stages into an index of its own, which the hook is handed in `GIT_INDEX_FILE`."""
        self.stand_in.answer(504, ADDED)
        self.repo.write(UNRELATED, f"# Unrelated\n\n[added]({ADDED})\n")
        self.assert_refused_naming(self.commit("-a"), ADDED, UNRELATED)

    def test_the_hooks_write_no_bytecode_into_the_tree(self):
        """Passes today and binds once the hook runs Python. The filed reproducer's fixture tracks `scripts/`
        with no `.gitignore`, so a rewritten `.pyc` there is `files were modified by this hook`, and a refusal."""
        self.append(UNRELATED, f"[added]({ADDED})")
        self.assert_landed(self.commit())
        self.assertIn(ADDED, self.stand_in.asked(), "the stand-in was never asked")
        self.assertEqual(sorted(self.repo.path.joinpath("scripts").rglob("*.pyc")), [])

    def test_a_link_added_to_a_name_with_a_space_refuses(self):
        self.stand_in.answer(504, ADDED)
        self.append("docs/with space.md", f"[added]({ADDED})")
        self.assert_refused_naming(self.commit(), ADDED, "docs/with space.md")

    def test_an_added_link_is_fetched_and_not_crawled(self):
        """lychee reads a URL on its command line as a page and asks for the page's links too."""
        self.append(UNRELATED, f"[added]({ADDED})")
        self.assert_landed(self.commit())
        self.assertIn(ADDED, self.stand_in.asked(), "the stand-in was never asked")
        self.assertNotIn(CRAWLED, self.stand_in.asked())

    def test_an_added_link_to_a_fragment_the_page_has_lands(self):
        self.append(UNRELATED, f"[added]({ADDED}#there)")
        self.assert_landed(self.commit())
        self.assertIn(ADDED, self.stand_in.asked(), "the stand-in was never asked")

    def test_an_added_link_to_a_fragment_the_page_lacks_refuses(self):
        """The flags the sweep uses, so a link gets one verdict (§AR-ci-release.5)."""
        self.append(UNRELATED, f"[added]({ADDED}#nowhere)")
        self.assert_refused_naming(self.commit(), f"{ADDED}#nowhere", UNRELATED)

    def test_a_url_in_a_code_fence_on_an_added_line_is_not_asked_about(self):
        fenced = "http://github.com/agent-grounds/rhei/in-a-code-fence"
        self.append(UNRELATED, f"[added]({ADDED})\n\n```text\n{fenced}\n```")
        self.assert_landed(self.commit())
        self.assertIn(ADDED, self.stand_in.asked(), "the stand-in was never asked")
        self.assertNotIn(fenced, self.stand_in.asked())

    def test_a_link_added_outside_the_inputs_is_not_asked_about(self):
        outside = "http://github.com/agent-grounds/rhei/outside-the-inputs"
        self.append(UNRELATED, f"[added]({ADDED})")
        self.append("crates/notes.md", f"[outside]({outside})")
        self.assert_landed(self.commit())
        self.assertIn(ADDED, self.stand_in.asked(), "the stand-in was never asked")
        self.assertNotIn(outside, self.stand_in.asked())


@needs_lychee_and_pre_commit
class InternalLinkTests(LinkGateCase):
    """Guards, passing today: a change can break an internal link from another file."""

    LINKER = "docs/linker.md"
    TARGET = "docs/target.md"

    def setUp(self) -> None:
        super().setUp()
        self.repo.write(self.LINKER, "# Linker\n\n[target](target.md#the-anchor)\n")
        self.repo.write(self.TARGET, "# Target\n\n## The anchor\n\nText.\n")
        self.repo.commit("links between two files")

    def assert_names_the_broken_link(self, result, broken: str) -> None:
        self.assertIn(self.LINKER, result.stdout, "the checker did not report the file that links it")
        self.assertIn(broken, result.stdout)

    def test_removing_a_heading_refuses_through_the_file_that_links_it(self):
        self.repo.write(self.TARGET, "# Target\n\nText.\n")
        self.repo.git("add", "-A")
        result = self.commit()
        self.assert_refused(result)
        self.assert_names_the_broken_link(result, "target.md#the-anchor")

    def test_removing_a_file_refuses_through_the_file_that_links_it(self):
        self.repo.git("rm", self.TARGET)
        result = self.commit()
        self.assert_refused(result)
        self.assert_names_the_broken_link(result, "target.md")

    def test_an_all_files_run_refuses_a_broken_internal_link(self):
        """The required job keeps every internal link and anchor (§AR-ci-release.1)."""
        self.repo.write(self.TARGET, "# Target\n\nText.\n")
        self.repo.commit("broke the anchor, past the hooks")
        result = self.run_all_files()
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assert_names_the_broken_link(result, "target.md#the-anchor")

    @unittest.skipUnless(os.environ.get("RHEI_RED_562"), "red until #562: the refusal asks every outside host first; implement removes this")
    def test_the_refusal_asks_no_outside_host(self):
        self.repo.git("rm", self.TARGET)
        result = self.commit()
        self.assert_refused(result)
        self.assertEqual(self.stand_in.asked(), [])


@unittest.skipUnless(os.environ.get("RHEI_RED_562"), "red until #562: a commit depends on every outside host; implement removes this")
@needs_lychee_and_pre_commit
class UntouchedLinkTests(LinkGateCase):
    """What the ticket reports: every link of the fixture answers 504, and none it leaves alone may refuse."""

    def setUp(self) -> None:
        super().setUp()
        self.stand_in.answer(504, *EVERY_LINK)

    def assert_landed_asking_nothing(self, result) -> None:
        self.assert_landed(result)
        self.assert_link_hooks_ran(result)
        self.assertEqual(self.stand_in.asked(), [])

    def test_a_link_free_commit_lands_and_asks_nothing(self):
        self.append(UNRELATED, "a line with no link")
        self.assert_landed_asking_nothing(self.commit())

    def test_an_edit_beside_a_link_does_not_ask_for_it(self):
        self.append(COMPARISON, "A line beside the links, holding none.")
        self.assert_landed_asking_nothing(self.commit())

    def test_an_edit_of_a_links_line_asks_for_that_link_alone(self):
        """`OTHER` is on the next line of this file and answers 504."""
        self.stand_in.answer(200, STEVE)
        path = self.repo.path.joinpath(*COMPARISON.split("/"))
        text = path.read_text(encoding="utf-8")
        self.assertIn(OTHER, text)
        self.repo.write(COMPARISON, text.replace("[steveyegge/beads]", "[beads, by Steve]"))
        self.repo.git("add", "-A")
        result = self.commit()
        self.assert_landed(result)
        self.assertEqual(self.stand_in.asked(), [STEVE])

    def test_a_renamed_file_asks_for_none_of_its_links(self):
        self.repo.git("mv", COMPARISON, RENAMED)
        self.assert_landed_asking_nothing(self.commit())

    def test_a_renamed_file_asks_only_for_what_the_move_adds(self):
        self.repo.git("mv", COMPARISON, RENAMED)
        self.append(RENAMED, f"[added]({ADDED})")
        result = self.commit()
        self.assert_landed(result)
        self.assertEqual(self.stand_in.asked(), [ADDED])

    def test_an_all_files_run_with_nothing_staged_asks_nothing(self):
        """The required job and `main`'s push run, on a tree whose every link is down (§AR-ci-release.1)."""
        result = self.run_all_files()
        self.assertEqual(result.returncode, 0, f"the run went red:\n{result.stdout}")
        self.assert_link_hooks_ran(result)
        self.assertEqual(self.stand_in.asked(), [])


if __name__ == "__main__":
    unittest.main()
