#!/usr/bin/env python3
"""The release writes its own notes from the merged pull requests. §FS-rhei-distribution.5.1

`prepare_changelog_release.py prepare <version>` lists the commits since the
previous release tag, maps each to its pull requests through the forge, and
writes the numbered section from their titles, newest first. Nobody writes
anything first, so there is no `## Unreleased` to promote. A forge that cannot
answer fails the release before anything is written (§FS-rhei-distribution.5.2).

Every history is a `TempRepo` under the test's own temporary directory
(§REQ-test-isolation.1), with every inherited `GIT_*` variable stripped
(§REQ-test-isolation.3), and `gh` is the stub on `PATH`, so the tests run
unchanged on all three platforms (§REQ-cross-platform.3).
"""

from __future__ import annotations

import unittest

from scripts.tests.changelog_test_support import (
    GENERATED_NOTE,
    RELEASE_SCRIPT,
    RELEASED_ENTRIES,
    ScriptTestCase,
    path_with_no_gh,
    pull,
    release_changelog,
)

TAG = "v0.1.0"
VERSION = "0.1.1"
DATE = "2026-10-05"
SOURCE = "crates/rhei-cli/src/main.rs"
OTHER_SOURCE = "crates/rhei-core/src/lib.rs"


def entry(number: int, title: str) -> str:
    return f"- [{title}](https://github.com/agent-grounds/rhei/pull/{number}) (PR #{number})"


def generated(entries: list[str], summary: str = "2 pull requests.") -> str:
    """The changelog `prepare 0.1.1` should leave over `release_changelog()`."""
    return (
        f"# Changelog\n\n{GENERATED_NOTE}\n\n"
        f"## 2. [{VERSION}] - {DATE}\n\n" + "\n".join(entries) + "\n\n"
        "## 3. Older releases\n\n"
        f"- [0.1.0](changelog/0.1.0.md) - 2026-09-20: {summary}\n"
        "- [0.0.9](changelog/0.0.9.md) - 2026-09-01: Before it.\n"
    )


class GeneratorTestCase(ScriptTestCase):
    def setUp(self) -> None:
        super().setUp()
        self.pulls: dict[str, list[object]] = {}

    def released(self):
        """A repo whose tag `v0.1.0` sits on the release commit of the inline 0.1.0 section."""
        repo = self.repo()
        repo.write_changelog(release_changelog())
        repo.write("docs/changelog/0.0.9.md", "# 0.0.9 - 2026-09-01\n\nBefore it.\n")
        repo.write("Cargo.toml", 'version = "0.1.0"\n')
        repo.write(SOURCE, "released\n")
        repo.commit("Release v0.1.0")
        repo.git("tag", TAG)
        return repo

    def land(self, repo, pr: dict[str, object] | None, *commits: list[str]) -> list[str]:
        """Commit each list of paths in turn, every commit belonging to `pr` (or to none)."""
        shas = []
        for index, paths in enumerate(commits):
            for path in paths:
                repo.write(path, f"{path} changed by {pr and pr['number']} ({index})\n")
            sha = repo.commit(f"commit {index} of {pr and pr['number']}")
            self.pulls[sha] = [pr] if pr is not None else []
            shas.append(sha)
        return shas

    def prepare(self, repo, path: str | None = None, **env: str | None):
        self.set_pulls(self.pulls)
        return self.run_script(RELEASE_SCRIPT, ["prepare", VERSION, "--date", DATE], repo, path=path, **env)

    def snapshot(self, repo) -> dict[str, bytes]:
        """Every file under `docs/`, by path, as bytes: what a failed release must leave alone."""
        docs = repo.path / "docs"
        return {
            path.relative_to(docs).as_posix(): path.read_bytes()
            for path in sorted(docs.rglob("*"))
            if path.is_file()
        }


class GenerateTests(GeneratorTestCase):
    def test_two_merged_pull_requests_become_the_section_newest_first(self):
        """The ticket's case: nobody wrote a line, and the release writes the notes itself."""
        repo = self.released()
        older = pull(7, "Let a plan name its own account")
        newer = pull(8, "Report what a terminal printed when its wait runs out")
        self.land(repo, older, [SOURCE])
        self.land(repo, newer, [OTHER_SOURCE])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(
            repo.read_changelog(),
            generated([entry(8, newer["title"]), entry(7, older["title"])]),
            self.report(result),
        )
        archive = (repo.path / "docs" / "changelog" / "0.1.0.md").read_text(encoding="utf-8")
        self.assertTrue(archive.startswith("# 0.1.0 - 2026-09-20\n"), archive)
        for line in RELEASED_ENTRIES:
            self.assertIn(line, archive, "the previous section is archived as it stood")

    def test_a_pull_request_merged_as_several_commits_is_listed_once(self):
        """Rebase merges: the endpoint maps every commit of a pull request to it."""
        repo = self.released()
        rebased = pull(7, "Split the runner into two passes")
        self.land(repo, rebased, [SOURCE], [OTHER_SOURCE], ["crates/rhei-cli/src/run.rs"])
        single = pull(8, "Name the account a plan charges")
        self.land(repo, single, ["crates/rhei-cli/src/account.rs"])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(
            repo.read_changelog(),
            generated([entry(8, single["title"]), entry(7, rebased["title"])]),
            self.report(result),
        )

    def test_order_is_the_first_parent_position_of_each_pull_requests_newest_commit(self):
        repo = self.released()
        first, second, third = pull(12, "Third by number, first merged"), pull(5, "Lowest number"), pull(9, "Last")
        self.land(repo, first, [SOURCE])
        self.land(repo, second, [OTHER_SOURCE])
        self.land(repo, third, ["crates/rhei-cli/src/last.rs"])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(
            repo.read_changelog(),
            generated(
                [entry(9, "Last"), entry(5, "Lowest number"), entry(12, "Third by number, first merged")]
            ),
        )

    def test_a_docs_and_ci_only_pull_request_is_left_out(self):
        repo = self.released()
        docs = pull(7, "Reword the guide")
        self.land(repo, docs, ["docs/guide.md", ".github/workflows/ci.yml", "README.md", "LICENSE", "lychee.toml"])
        code = pull(8, "Fix the runner")
        self.land(repo, code, [SOURCE])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        notes = repo.read_changelog()
        self.assertIn(entry(8, "Fix the runner"), notes, self.report(result))
        self.assertNotIn("PR #7", notes, "a pull request that touched only docs and CI is not a release note")

    def test_a_pull_request_with_one_code_commit_among_docs_commits_is_listed(self):
        """A pull request's paths are the union of its commits' paths."""
        repo = self.released()
        mixed = pull(7, "Document and fix the runner")
        self.land(repo, mixed, ["docs/guide.md"], [SOURCE])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertIn(entry(7, mixed["title"]), repo.read_changelog(), self.report(result))

    def test_only_a_merged_pull_request_counts(self):
        """`commits/<sha>/pulls` also returns open pull requests that contain the commit."""
        repo = self.released()
        merged, still_open = pull(7, "The merged one"), pull(9, "Still open", merged=False)
        sha = self.land(repo, merged, [SOURCE])[0]
        self.pulls[sha] = [still_open, merged]

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        notes = repo.read_changelog()
        self.assertIn(entry(7, "The merged one"), notes, self.report(result))
        self.assertNotIn("PR #9", notes)

    def test_a_code_commit_outside_any_pull_request_is_warned_about_and_not_listed(self):
        repo = self.released()
        dev = self.land(repo, None, ["Cargo.toml"])[0]
        self.land(repo, pull(7, "Fix the runner"), [SOURCE])
        direct = self.land(repo, None, [OTHER_SOURCE])[0]

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        for sha in (dev, direct):
            self.assertIn(
                f"warning: commit {sha[:7]} changed code but belongs to no pull request; it is not in the notes",
                result.stderr,
                self.report(result),
            )
        section = repo.read_changelog().split("## 3. Older releases")[0]
        self.assertEqual(section.count("(PR #"), 1, section)

    def test_brackets_and_backslashes_in_a_title_are_escaped(self):
        repo = self.released()
        self.land(repo, pull(7, r"Read [x] and a\b as text"), [SOURCE])

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertIn(
            r"- [Read \[x\] and a\\b as text](https://github.com/agent-grounds/rhei/pull/7) (PR #7)",
            repo.read_changelog(),
            self.report(result),
        )

    def test_stamp_is_gone(self):
        """Nothing is left to stamp: the entries carry their numbers from the start."""
        repo = self.released()
        result = self.run_script(RELEASE_SCRIPT, ["stamp"], repo)
        self.assertEqual(result.returncode, 2, self.report(result))
        self.assertIn("invalid choice", result.stderr)


class ForgeFailureTests(GeneratorTestCase):
    """A release that cannot ask the forge writes nothing. §FS-rhei-distribution.5.2"""

    def test_one_unanswered_commit_fails_the_release_and_writes_nothing(self):
        repo = self.released()
        self.land(repo, pull(7, "Answered"), [SOURCE])
        unanswered = self.land(repo, pull(8, "Unanswered"), [OTHER_SOURCE])[0]
        before = self.snapshot(repo)

        result = self.prepare(repo, GH_STUB_FAIL=unanswered)
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(
            f"error: could not ask the forge about commit {unanswered[:7]}: HTTP 502: Bad Gateway",
            result.stderr,
            self.report(result),
        )
        self.assertEqual(self.snapshot(repo), before, "the changelog and docs/changelog/ are byte-for-byte unchanged")

    def test_no_gh_on_path_fails_the_release_and_writes_nothing(self):
        repo = self.released()
        self.land(repo, pull(7, "Fix the runner"), [SOURCE])
        before = self.snapshot(repo)

        result = self.prepare(repo, path=path_with_no_gh())
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn("error: gh is not on PATH; a release's notes are read from the forge", result.stderr)
        self.assertEqual(self.snapshot(repo), before)


class PrepareRefusalTests(GeneratorTestCase):
    """What `Release minor` meets when there is nothing to list. §FS-rhei-distribution.5.2"""

    def test_only_docs_and_the_dev_advance_since_the_tag_is_nothing_to_release(self):
        repo = self.released()
        self.land(repo, None, ["Cargo.toml"])
        self.land(repo, pull(7, "Reword the guide"), ["docs/guide.md"])
        before = self.snapshot(repo)

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(
            "error: no pull request merged since v0.1.0 changed more than docs and CI; nothing to release",
            result.stderr,
            self.report(result),
        )
        self.assertEqual(self.snapshot(repo), before, "a refusal writes nothing, nor archives anything")

    def test_a_previous_tag_that_does_not_exist_is_refused(self):
        repo = self.released()
        repo.git("tag", "-d", TAG)
        self.land(repo, pull(7, "Fix the runner"), [SOURCE])
        before = self.snapshot(repo)

        result = self.prepare(repo)
        self.assertEqual(result.returncode, 1, self.report(result))
        self.assertIn(TAG, result.stderr, "the refusal names the tag it looked for")
        self.assertNotIn("Unreleased", result.stderr)
        self.assertEqual(self.snapshot(repo), before)


if __name__ == "__main__":
    unittest.main()
