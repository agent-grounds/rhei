#!/usr/bin/env python3
"""The staged pass of the link check: only the external URLs a commit adds. §AR-ci-release.2.1

Run from the repository root with the inputs of the whole-tree pass as its
arguments. It reads the lines `git diff --cached` adds under them, keeps the
URLs `lychee --dump --verbose` reports for those files and an added line carries,
and asks lychee about those alone, with the flags the sweep uses
(§AR-ci-release.5). So a commit that adds no URL asks no host, and neither does
`pre-commit run --all-files`, which stages nothing (§AR-ci-release.1).

The URLs go to lychee on stdin, because it fetches a URL on its command line as a
page and asks for that page's links too. It names a stdin URL's source `[stdin]`,
so a refusal prints the file and line itself. The environment is left alone:
`git commit -a` hands the hook its index in `GIT_INDEX_FILE`.

Tools resolve through `shutil.which`, because a bare name misses a `.bat` on
Windows (§REQ-cross-platform.3). `tool_lookup` says the same, and importing it
would write bytecode into a tree that may track `scripts/`.
"""

from __future__ import annotations

import html
import os
import posixpath
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Sequence
from urllib.parse import urlsplit

HUNK = re.compile(r"^@@ -\d+(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
DUMPED = re.compile(r"^(\S+) \((.+)\)( \[excluded\])?$")
SCHEME_AND_HOST = re.compile(r"https?://[\w.:-]*", re.IGNORECASE)


def _tool(name: str) -> str:
    found = shutil.which(name)
    if found is None:
        raise SystemExit(f"check_staged_links: {name} is not on PATH")
    return found


def _read(repo_root: Path | str, name: str, *args: str) -> str:
    """What a command that asks no host prints. One that fails refuses: failing is not answering nothing."""
    done = subprocess.run([_tool(name), *args], cwd=repo_root, capture_output=True, check=False)
    if done.returncode != 0:
        said = done.stderr.decode("utf-8", "replace")
        raise SystemExit(f"check_staged_links: {name} {' '.join(args)} exited {done.returncode}:\n{said}")
    return done.stdout.decode("utf-8", "replace")


def staged_diff(repo_root: Path | str, inputs: Sequence[str]) -> str:
    """The index against `HEAD` under `inputs`, with every option that shapes the text pinned.

    Renames are detected whatever the configuration says, because moving a file touches none of
    its links (§AR-ci-release.2.1). The rest is so no one's git configuration changes what is parsed.
    """
    return _read(
        repo_root, "git", "-c", "core.quotepath=off", "diff", "--cached", "--no-color", "--no-ext-diff",
        "--find-renames", "--src-prefix=a/", "--dst-prefix=b/", "--unified=0", "--inter-hunk-context=0",
        "--", *inputs,
    )


def added_lines(diff_text: str) -> dict[str, dict[int, str]]:
    """`{path: {line_number: text}}` for each line the diff adds, with `/`-separated paths.

    Each hunk's lines are counted rather than recognised, because an added line reading
    `++ text` is `+++ text` in the diff. Git ends a `+++` name holding whitespace with a tab.
    """
    added: dict[str, dict[int, str]] = {}
    path = None
    number = removed_left = added_left = 0
    for line in diff_text.split("\n"):
        if removed_left or added_left:
            if line.startswith("+"):
                if path is not None:
                    added.setdefault(path, {})[number] = line[1:].removesuffix("\r")
                number += 1
                added_left -= 1
            elif line.startswith("-"):
                removed_left -= 1
        elif line.startswith("+++ "):
            target = line[4:].removesuffix("\t")
            path = None if target == "/dev/null" else target.removeprefix("b/")
        elif hunk := HUNK.match(line):
            removed_left = 1 if hunk[1] is None else int(hunk[1])
            number = int(hunk[2])
            added_left = 1 if hunk[3] is None else int(hunk[3])
    return added


def _carries(line: str, url: str) -> bool:
    """Whether `line` writes `url`, as lychee reports it: scheme and host in lower case, and maybe without the
    slash it adds to a bare host. The path keeps its case, because `/Foo` and `/foo` are different URLs.

    The raw line counts as well as the entity-decoded one: `html.unescape` reads the `&sect` of `&section=` as
    an entity, and lychee does not. Any other form lychee rewrites is left to the sweep (§AR-ci-release.2.1).
    """
    texts = {SCHEME_AND_HOST.sub(lambda written: written[0].lower(), text) for text in (line, html.unescape(line))}
    forms = {url, url.rstrip("/")}
    return any(re.search(re.escape(form) + r"(?![\w\-/%~+=&?#]|\.\w)", text) for form in forms for text in texts)


def staged_urls(dump_text: str, added: dict[str, dict[int, str]]) -> dict[str, list[str]]:
    """`{url: ["path:line", ...]}` for each external URL lychee reports for a file and an added line of it carries.

    A URL in a code fence is on the line but not in the dump, so it stays out.
    """
    urls: dict[str, list[str]] = {}
    for found in map(DUMPED.match, dump_text.splitlines()):
        if not found or found[3] or urlsplit(found[1]).scheme not in ("http", "https"):
            continue
        source = posixpath.normpath(found[2].replace(os.sep, "/"))
        for number, text in added.get(source, {}).items():
            if _carries(text, found[1]):
                urls.setdefault(found[1], []).append(f"{source}:{number}")
    return urls


def main(inputs: Sequence[str]) -> int:
    # A name git or lychee prints may not fit a Windows console's encoding, and a refusal must still say it.
    sys.stdout.reconfigure(errors="backslashreplace")
    repo_root = Path.cwd()
    added = added_lines(staged_diff(repo_root, inputs))
    if not added:
        return 0
    urls = staged_urls(_read(repo_root, "lychee", "--dump", "--verbose", *inputs), added)
    if not urls:
        return 0
    asked = "".join(f"{url}\n" for url in urls).encode("utf-8")
    command = [_tool("lychee"), "--no-progress", "--include-fragments", "-"]
    if subprocess.run(command, cwd=repo_root, input=asked, check=False).returncode == 0:
        return 0
    print("The URLs this commit adds, where lychee was asked about them:")
    for url, places in urls.items():
        print(f"  {url}: {', '.join(places)}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
