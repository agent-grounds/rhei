#!/usr/bin/env python3
"""Which URLs a commit asks about, read from its staged diff. §AR-ci-release.2.1

The `test` job runs these on Linux, macOS and Windows and installs neither lychee
nor pre-commit, so this file is what proves the line scoping on all three
(§REQ-cross-platform.3): canned `git diff` and `lychee --dump --verbose` text for
the two parsers, real git for the diff itself, and a stub lychee for the script.
The canned text is what git and lychee 0.23.0 print.
"""

from __future__ import annotations

import importlib
import json
import os
import sys
import unittest
from unittest import mock

from scripts.tests.changelog_test_support import SCRIPTS, ScriptTestCase, TempRepo, _install, clean_env
from scripts.tests.link_gate_test_support import STAGED_SCRIPT

INPUTS = ("README.md", "docs", "examples", "packages")
OLD = "http://github.com/agent-grounds/rhei/old"
NEW = "http://github.com/agent-grounds/rhei/new"


def staged_links():
    """`scripts/check_staged_links.py` as a module, with `scripts/` on the path as a hook has it."""
    with mock.patch.object(sys, "path", [str(SCRIPTS), *sys.path]):
        return importlib.import_module("check_staged_links")


def diff_of(path: str, *hunks: str) -> str:
    return f"diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n" + "".join(hunks)


class AddedLinesTests(unittest.TestCase):
    def added(self, diff: str):
        return staged_links().added_lines(diff)

    def test_each_hunk_gives_its_new_lines_their_numbers(self):
        diff = diff_of("docs/a.md", "@@ -3 +3 @@ # A\n-old text\n+new text\n@@ -9,0 +10,2 @@\n+first\n+second\n")
        self.assertEqual(self.added(diff), {"docs/a.md": {3: "new text", 10: "first", 11: "second"}})

    def test_a_removed_line_adds_nothing(self):
        self.assertEqual(self.added(diff_of("docs/a.md", f"@@ -3 +2,0 @@\n-[old]({OLD})\n")), {})

    def test_nothing_staged_adds_nothing(self):
        self.assertEqual(self.added(""), {})

    def test_a_new_file_adds_every_line(self):
        diff = (
            "diff --git a/docs/a.md b/docs/a.md\nnew file mode 100644\nindex 0000000..0d77a70\n"
            "--- /dev/null\n+++ b/docs/a.md\n@@ -0,0 +1,2 @@\n+one\n+two\n"
        )
        self.assertEqual(self.added(diff), {"docs/a.md": {1: "one", 2: "two"}})

    def test_a_deleted_file_adds_nothing(self):
        diff = (
            "diff --git a/docs/a.md b/docs/a.md\ndeleted file mode 100644\nindex 81bee59..0000000\n"
            f"--- a/docs/a.md\n+++ /dev/null\n@@ -1 +0,0 @@\n-[old]({OLD})\n"
        )
        self.assertEqual(self.added(diff), {})

    def test_a_pure_rename_adds_nothing(self):
        diff = "diff --git a/docs/a.md b/docs/b.md\nsimilarity index 100%\nrename from docs/a.md\nrename to docs/b.md\n"
        self.assertEqual(self.added(diff), {})

    def test_a_rename_with_an_edit_adds_only_the_edit_under_the_new_name(self):
        diff = (
            "diff --git a/docs/a.md b/docs/b.md\nsimilarity index 80%\nrename from docs/a.md\nrename to docs/b.md\n"
            "index 1111111..2222222 100644\n--- a/docs/a.md\n+++ b/docs/b.md\n@@ -4,0 +5 @@\n+the edit\n"
        )
        self.assertEqual(self.added(diff), {"docs/b.md": {5: "the edit"}})

    def test_a_name_with_a_space_loses_the_tab_git_ends_it_with(self):
        """Git ends a `+++` name holding whitespace with a tab; keeping it loses the file silently."""
        diff = (
            "diff --git a/docs/a b.md b/docs/a b.md\nnew file mode 100644\nindex 0000000..0d77a70\n"
            "--- /dev/null\n+++ b/docs/a b.md\t\n@@ -0,0 +1 @@\n+one\n"
        )
        self.assertEqual(self.added(diff), {"docs/a b.md": {1: "one"}})

    def test_an_added_line_that_reads_like_a_header_is_a_line(self):
        """Text starting `++ ` is `+++ ` in the diff, and the hunk's next line stays in this file."""
        hunk = f"@@ -1,0 +2,2 @@\n+++ {NEW}\n+next\n"
        self.assertEqual(self.added(diff_of("docs/a.md", hunk)), {"docs/a.md": {2: f"++ {NEW}", 3: "next"}})

    def test_the_no_newline_marker_is_not_a_line(self):
        hunk = "@@ -1,0 +2 @@\n+last\n\\ No newline at end of file\n"
        self.assertEqual(self.added(diff_of("docs/a.md", hunk)), {"docs/a.md": {2: "last"}})


def located(found: dict[str, list[str]]) -> dict[str, list[str]]:
    """Locations with `/`, whichever separator the platform's lychee printed."""
    return {url: [where.replace("\\", "/") for where in places] for url, places in found.items()}


class StagedUrlsTests(unittest.TestCase):
    PATH = os.path.join("docs", "a.md")

    def urls(self, dump: str, added: dict[str, dict[int, str]]):
        return located(staged_links().staged_urls(dump, added))

    def test_a_url_on_an_added_line_is_located_there(self):
        found = self.urls(f"{NEW} ({self.PATH})\n", {"docs/a.md": {5: f"[new]({NEW})"}})
        self.assertEqual(found, {NEW: ["docs/a.md:5"]})

    def test_a_url_on_an_unchanged_line_of_that_file_is_left_out(self):
        dump = f"{OLD} ({self.PATH})\n{NEW} ({self.PATH})\n"
        self.assertEqual(self.urls(dump, {"docs/a.md": {5: f"[new]({NEW})"}}), {NEW: ["docs/a.md:5"]})

    def test_a_url_lychee_reports_for_another_file_is_left_out(self):
        """It is in a code fence on the added line here, and a real link in the file left alone."""
        dump = f"{NEW} ({os.path.join('docs', 'b.md')})\n"
        self.assertEqual(self.urls(dump, {"docs/a.md": {5: NEW}}), {})

    def test_a_url_lychee_would_not_check_is_left_out(self):
        """A code fence: the line carries the URL, and lychee's dump does not report it."""
        self.assertEqual(self.urls("", {"docs/a.md": {5: NEW}}), {})

    def test_an_excluded_url_is_left_out(self):
        """lychee excludes `example.com` by itself, and its dump says so at the end of the line."""
        url = "http://example.com/"
        self.assertEqual(self.urls(f"{url} ({self.PATH}) [excluded]\n", {"docs/a.md": {5: f"<{url}>"}}), {})

    def test_a_local_link_is_left_out(self):
        url = "file:///repo/docs/other.md"
        self.assertEqual(self.urls(f"{url} ({self.PATH})\n", {"docs/a.md": {5: f"[other]({url})"}}), {})

    def test_a_longer_url_does_not_carry_its_prefix(self):
        """The dump lists both, and the added line writes only the longer one."""
        dump = f"{NEW} ({self.PATH})\n{NEW}er ({self.PATH})\n"
        self.assertEqual(self.urls(dump, {"docs/a.md": {5: f"[new]({NEW}er)"}}), {f"{NEW}er": ["docs/a.md:5"]})

    def test_a_bare_host_is_carried_without_the_slash_lychee_adds(self):
        url = "http://github.com/"
        self.assertEqual(self.urls(f"{url} ({self.PATH})\n", {"docs/a.md": {5: "<http://github.com>"}}), {url: ["docs/a.md:5"]})

    def test_an_escaped_ampersand_is_carried_as_lychee_decodes_it(self):
        url = f"{NEW}?x=1&y=2"
        self.assertEqual(self.urls(f"{url} ({self.PATH})\n", {"docs/a.md": {5: f"[q]({NEW}?x=1&amp;y=2)"}}), {url: ["docs/a.md:5"]})

    def test_a_url_carried_twice_is_one_url_with_both_locations(self):
        other = os.path.join("docs", "b.md")
        added = {"docs/a.md": {5: f"[new]({NEW})"}, "docs/b.md": {7: f"[new]({NEW})"}}
        self.assertEqual(self.urls(f"{NEW} ({self.PATH})\n{NEW} ({other})\n", added), {NEW: ["docs/a.md:5", "docs/b.md:7"]})

    def test_a_name_with_a_space_matches_the_dump(self):
        path = os.path.join("docs", "a b.md")
        self.assertEqual(self.urls(f"{NEW} ({path})\n", {"docs/a b.md": {1: f"[new]({NEW})"}}), {NEW: ["docs/a b.md:1"]})


# Everything a user's configuration could change about the text the parser reads, each on its own:
# together, `diff.noprefix` hides `diff.mnemonicPrefix`, and a parser that only drops `b/` passes.
HOSTILE_GIT_CONFIG = (
    ("core", "quotepath", "true"),
    ("color", "ui", "always"),
    ("diff", "noprefix", "true"),
    ("diff", "mnemonicPrefix", "true"),
    ("diff", "srcPrefix", "x/"),
    ("diff", "dstPrefix", "y/"),
    ("diff", "renames", "false"),
    ("diff", "external", "false"),
    ("diff", "context", "9"),
)


class StagedDiffTests(ScriptTestCase):
    """Real git and no lychee, so all three platforms read the diff the hook reads."""

    def seeded(self) -> TempRepo:
        repo = self.repo()
        repo.write("docs/a.md", "# A\n\nline\n")
        repo.write("docs/moved.md", f"# Moved\n\nfirst\nsecond\nthird\n\n[old]({OLD})\n")
        repo.write("crates/notes.md", "# Notes\n")
        repo.commit("baseline")
        return repo

    def added(self, repo: TempRepo, **env: str | None) -> dict[str, dict[int, str]]:
        module = staged_links()
        with mock.patch.dict(os.environ, clean_env(**env), clear=True):
            return module.added_lines(module.staged_diff(repo.path, INPUTS))

    def test_nothing_staged_is_nothing_added(self):
        self.assertEqual(self.added(self.seeded()), {})

    def test_it_reads_the_index_and_not_the_work_tree(self):
        repo = self.seeded()
        repo.write("docs/a.md", f"# A\n\nline\n[new]({NEW})\n")
        repo.git("add", "-A")
        repo.write("docs/a.md", f"# A\n\nline\n[new]({NEW})\n[unstaged]({OLD})\n")
        self.assertEqual(self.added(repo), {"docs/a.md": {4: f"[new]({NEW})"}})

    def test_it_reads_only_the_inputs(self):
        repo = self.seeded()
        repo.write("crates/notes.md", f"# Notes\n[new]({NEW})\n")
        repo.git("add", "-A")
        self.assertEqual(self.added(repo), {})

    def test_a_hostile_git_configuration_changes_nothing(self):
        repo = self.seeded()
        repo.write("docs/a.md", "# A\n\nline\nedited\n")
        repo.write("docs/日本語.md", "新しい\n")
        repo.git("mv", "docs/moved.md", "docs/renamed.md")
        repo.git("add", "-A")
        expected = {"docs/a.md": {4: "edited"}, "docs/日本語.md": {1: "新しい"}}
        config = self.tmp / "gitconfig"
        for section, key, value in HOSTILE_GIT_CONFIG:
            with self.subTest(f"{section}.{key}={value}"):
                config.write_text(f"[{section}]\n    {key} = {value}\n", encoding="utf-8")
                self.assertEqual(self.added(repo, GIT_CONFIG_GLOBAL=str(config)), expected)


LYCHEE_STUB = r"""
import json, os, sys

argv = sys.argv[1:]
if "--dump" in argv:
    with open(os.environ["LYCHEE_STUB_DUMPS"], "a", encoding="utf-8") as handle:
        handle.write(json.dumps(argv) + "\n")
    with open(os.environ["LYCHEE_STUB_DUMP"], encoding="utf-8") as handle:
        sys.stdout.write(handle.read())
    sys.exit(int(os.environ.get("LYCHEE_STUB_DUMP_EXIT", "0")))
stdin = sys.stdin.read() if "-" in argv else ""
with open(os.environ["LYCHEE_STUB_CALLS"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({"argv": argv, "stdin": stdin}) + "\n")
sys.exit(int(os.environ.get("LYCHEE_STUB_EXIT", "0")))
"""


class StagedScriptTests(ScriptTestCase):
    """The script's wiring under a stub lychee, on the platforms that have no real one."""

    def setUp(self) -> None:
        super().setUp()
        _install(self.bin, "lychee", LYCHEE_STUB)
        self.calls = self.bin / "lychee-calls.jsonl"
        self.dumps = self.bin / "lychee-dumps.jsonl"
        self.git_repo = self.repo()
        self.git_repo.write("README.md", "# R\n")
        self.git_repo.write("docs/a.md", f"# A\n\n[old]({OLD})\n")
        self.git_repo.commit("baseline")

    def staged(self, line: str) -> None:
        self.git_repo.write("docs/a.md", f"# A\n\n[old]({OLD})\n{line}\n")
        self.git_repo.git("add", "-A")

    def run_staged(self, **env: str):
        dump = self.bin / "lychee-dump.txt"
        path = os.path.join("docs", "a.md")
        dump.write_text(f"{OLD} ({path})\n{NEW} ({path})\n", encoding="utf-8")
        return self.run_script(
            STAGED_SCRIPT,
            list(INPUTS),
            self.git_repo,
            LYCHEE_STUB_DUMP=str(dump),
            LYCHEE_STUB_DUMPS=str(self.dumps),
            LYCHEE_STUB_CALLS=str(self.calls),
            **env,
        )

    def asks(self) -> list[dict[str, object]]:
        """What lychee was asked to check: every call but the dump, which asks no host."""
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text(encoding="utf-8").splitlines()]

    def test_nothing_staged_asks_nothing(self):
        result = self.run_staged()
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.asks(), [])

    def test_a_staged_line_with_no_link_asks_nothing(self):
        self.staged("a line with no link")
        result = self.run_staged()
        self.assertEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.asks(), [])

    def test_a_staged_url_goes_on_stdin_and_never_on_the_command_line(self):
        """lychee fetches and crawls a URL on its command line as a page to read."""
        self.staged(f"[new]({NEW})")
        result = self.run_staged()
        self.assertEqual(result.returncode, 0, self.report(result))
        asks = self.asks()
        self.assertEqual(len(asks), 1, asks)
        argv = asks[0]["argv"]
        for flag in ("--no-progress", "--include-fragments", "-"):
            self.assertIn(flag, argv)
        self.assertNotIn("--offline", argv)
        self.assertEqual([word for word in argv if "://" in word], [])
        self.assertIn(NEW, asks[0]["stdin"])
        self.assertNotIn(OLD, asks[0]["stdin"])

    def test_a_refused_url_refuses_with_its_file_and_line(self):
        """lychee's own source for it is `[stdin]`, which sends nobody anywhere."""
        self.staged(f"[new]({NEW})")
        result = self.run_staged(LYCHEE_STUB_EXIT="2")
        self.assertNotEqual(result.returncode, 0, self.report(result))
        self.assertIn(NEW, self.output(result))
        self.assertIn("docs/a.md:4", self.output(result).replace("\\", "/"))

    def test_a_dump_that_fails_refuses_rather_than_asking_nothing(self):
        """A dump that failed is not a line with no URL on it."""
        self.staged(f"[new]({NEW})")
        result = self.run_staged(LYCHEE_STUB_DUMP_EXIT="1")
        self.assertTrue(self.dumps.exists(), f"lychee's dump was never asked for:\n{self.report(result)}")
        self.assertNotEqual(result.returncode, 0, self.report(result))
        self.assertEqual(self.asks(), [])


if __name__ == "__main__":
    unittest.main()
