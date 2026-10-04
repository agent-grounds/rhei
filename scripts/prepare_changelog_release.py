#!/usr/bin/env python3
"""Generate and read changelog release sections. §FS-rhei-distribution.5

`prepare` writes the release's section itself, from the pull requests merged
since the previous release tag, read from the forge (§FS-rhei-distribution.5.1).
It builds the whole list before it writes anything, so a forge that cannot
answer leaves every file as it was (§FS-rhei-distribution.5.2).

`due` is the scheduled release's gate: the release is due when the list
`prepare` would write is not empty, so it builds that same list
(§FS-rhei-distribution.5.3).
"""

from __future__ import annotations

import argparse
import datetime as _datetime
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Sequence

from tool_lookup import tool


VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
RELEASE_RE = re.compile(
    r"^## (?P<number>[0-9]+)\. \[(?P<version>[0-9]+\.[0-9]+\.[0-9]+)\] - (?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})\s*$"
)
OLDER_RE = re.compile(r"^## (?P<number>[0-9]+)\. Older releases\s*$")
# What a release is not cut for: docs and CI. §FS-rhei-distribution.5.1
DOCS_AND_CI_RE = re.compile(r"^(?:docs/|\.github/)|\.md$|^(?:LICENSE|lychee\.toml)$")
# One generated entry, so an archived section can be summarised by its count. §FS-rhei-distribution.5.1
ENTRY_RE = re.compile(r"^- \[.*\]\(\S+\) \(PR #[0-9]+\)\s*$")
TITLE_ESCAPE_RE = re.compile(r"([\\\[\]])")


class ChangelogError(Exception):
    pass


def prepare_release(changelog: Path, version: str, release_date: str) -> None:
    """Generate the numbered section for `version` and archive the one it displaces.

    Every refusal and every forge read comes before the first write, so a release
    that fails leaves the changelog, the archive and the tree as they were.
    §FS-rhei-distribution.5.2
    """
    _validate_version(version)
    _validate_date(release_date)

    lines = _read_lines(changelog)
    sections = _find_top_level_sections(lines)
    if not sections:
        raise ChangelogError("missing the inline release section")
    latest = sections[0]
    latest_match = RELEASE_RE.match(_line_text(lines[latest]))
    if latest_match is None:
        raise ChangelogError(f"expected the inline release heading, got: {_line_text(lines[latest])}")
    older = _find_section_after(lines, sections, latest, OLDER_RE, "Older releases")
    older_match = OLDER_RE.match(_line_text(lines[older]))
    assert older_match is not None

    if latest_match.group("version") == version:
        raise ChangelogError(f"docs/changelog.md already has {version} as the inline latest release")

    previous_version = latest_match.group("version")
    previous_date = latest_match.group("date")
    archive_path = changelog.parent / "changelog" / f"{previous_version}.md"
    if archive_path.exists():
        raise ChangelogError(f"archive already exists: {archive_path}")

    # Before anything is spawned, git included. §FS-rhei-distribution.5.2
    _require_gh()
    history = _History(changelog)
    previous_tag = f"v{previous_version}"
    tagged = history.tag(previous_tag)
    if tagged is None:
        # The range starts at the tag the inline heading names. §FS-rhei-distribution.5.1
        raise ChangelogError(
            f"the previous release tag {previous_tag} does not exist; fetch the tags (git fetch --tags)"
        )
    pull_requests = _notes_since(history, _Forge(history.cwd), tagged)
    if not pull_requests:
        # A person started this release, so it does not hold; it refuses. §FS-rhei-distribution.5.3
        raise ChangelogError(
            f"no pull request merged since {previous_tag} changed more than docs and CI; nothing to release"
        )

    previous_body = lines[latest + 1 : older]
    archived_body = [_rewrite_relative_links_for_archive(line) for line in previous_body]
    summary = _summary_from(previous_body)
    _write_lines(archive_path, [f"# {previous_version} - {previous_date}\n", *archived_body])

    older_body = _drop_leading_blank_lines(lines[older + 1 :])
    archive_link = f"- [{previous_version}](changelog/{previous_version}.md) - {previous_date}: {summary}\n"
    new_lines = [
        *lines[:latest],
        f"## {latest_match.group('number')}. [{version}] - {release_date}\n",
        "\n",
        *(f"{pull_request.entry()}\n" for pull_request in pull_requests),
        "\n",
        f"## {older_match.group('number')}. Older releases\n",
        "\n",
        archive_link,
        *older_body,
    ]
    _write_lines(changelog, new_lines)


@dataclass
class _PullRequest:
    """One merged pull request, gathered from every commit of the range that belongs to it."""

    number: int
    title: str
    url: str
    # Where its newest commit sits on the first-parent line; 0 is the newest. §FS-rhei-distribution.5.1
    position: int
    paths: set[str] = field(default_factory=set)

    def changed_code(self) -> bool:
        return any(not DOCS_AND_CI_RE.search(path) for path in self.paths)

    def entry(self) -> str:
        """`- [<title>](<url>) (PR #N)`, the title escaped so the link renders. §FS-rhei-distribution.5.1"""
        title = TITLE_ESCAPE_RE.sub(r"\\\1", " ".join(self.title.split()))
        return f"- [{title}]({self.url}) (PR #{self.number})"


def _notes_since(history: _History, forge: _Forge, tagged: str) -> list[_PullRequest]:
    """Every merged pull request since `tagged` that changed more than docs and CI, newest first.

    One forge read per commit; any one that fails fails the whole list, because a
    list missing a pull request looks exactly like a correct one.
    §FS-rhei-distribution.5.1 §FS-rhei-distribution.5.2
    """
    found: dict[int, _PullRequest] = {}
    for sha, position in history.commits_since(tagged):
        paths = history.paths(sha)
        merged = [pull for pull in forge.pulls(sha) if pull.get("merged_at")]
        if not merged:
            if any(not DOCS_AND_CI_RE.search(path) for path in paths):
                _warn(f"commit {sha[:7]} changed code but belongs to no pull request; it is not in the notes")
            continue
        for pull in merged:
            number = int(pull["number"])
            if number not in found:
                found[number] = _PullRequest(number, str(pull.get("title", "")), str(pull.get("html_url", "")), position)
            pull_request = found[number]
            # A rebase-merged pull request is many commits and one entry. §FS-rhei-distribution.5.1
            pull_request.position = min(pull_request.position, position)
            pull_request.paths |= paths
    listed = [pull_request for pull_request in found.values() if pull_request.changed_code()]
    return sorted(listed, key=lambda pull_request: (pull_request.position, -pull_request.number))


class _Forge:
    """The merged pull requests a commit belongs to, asked of `gh`. §FS-rhei-distribution.5.2"""

    def __init__(self, cwd: Path) -> None:
        self.cwd = cwd

    def pulls(self, sha: str) -> list[dict[str, object]]:
        # The objects rather than a `--jq` projection: the notes read the title,
        # the URL and whether it merged.
        result = _spawn(self.cwd, "gh", "api", f"repos/{{owner}}/{{repo}}/commits/{sha}/pulls")
        if result is None:
            raise ChangelogError(f"could not ask the forge about commit {sha[:7]}: gh could not be started")
        if result.returncode != 0:
            raise ChangelogError(f"could not ask the forge about commit {sha[:7]}: {_first_line(result.stderr)}")
        try:
            answer = json.loads(result.stdout)
        except json.JSONDecodeError:
            answer = None
        if not isinstance(answer, list) or not all(isinstance(pull, dict) for pull in answer):
            raise ChangelogError(
                f"could not ask the forge about commit {sha[:7]}: unreadable answer {_first_line(result.stdout)!r}"
            )
        return answer


def release_due(changelog: Path, tag: str, output: Path) -> None:
    """Answer `Auto bump`'s gate step: is a release due since `tag`?

    `ok=true` goes to `output` exactly when the list `prepare` would write is not
    empty. Paths are read with git first, so a week of docs and CI alone asks the
    forge nothing. A hold exits 0 with a notice saying why; a forge that cannot
    answer fails the run, because a list it could not build is not an empty one.
    §FS-rhei-distribution.5.3
    """
    history = _History(changelog)
    tagged = history.commit(tag)
    due = False
    if not history.code_changed_since(tagged):
        print(f"::notice::Only docs/CI changes since {tag}; skipping.")
    else:
        _require_gh()
        if _notes_since(history, _Forge(history.cwd), tagged):
            due = True
        else:
            # The version advance after every release is code outside any pull request.
            print(f"::notice::No pull request that changed code merged since {tag}; skipping.")
    with output.open("a", encoding="utf-8") as handle:
        handle.write(f"ok={'true' if due else 'false'}\n")


class _History:
    """The commits and paths since a release tag, asked of git."""

    def __init__(self, changelog: Path) -> None:
        resolved = changelog.resolve()
        self.cwd = resolved.parent if resolved.parent.is_dir() else Path.cwd()

    def commit(self, ref: str) -> str:
        return self._git("rev-parse", "--verify", f"{ref}^{{commit}}").strip()

    def tag(self, name: str) -> str | None:
        """The commit tag `name` points at, or `None` where there is no such tag."""
        result = _spawn(self.cwd, "git", "rev-parse", "--verify", "--quiet", f"refs/tags/{name}^{{commit}}")
        if result is None:
            raise ChangelogError("`git` is not on PATH")
        return result.stdout.strip() if result.returncode == 0 else None

    def code_changed_since(self, commit: str) -> list[str]:
        """The paths outside docs and CI that differ between `commit` and `HEAD`."""
        paths = self._git("diff", "--name-only", "-z", commit, "HEAD").split("\0")
        return [path for path in paths if path and not DOCS_AND_CI_RE.search(path)]

    def commits_since(self, commit: str) -> list[tuple[str, int]]:
        """Each commit after `commit`, with its place on the first-parent line, newest first.

        A commit a merge brought in takes the place of that merge, so the order
        comes from git and not from timestamps. §FS-rhei-distribution.5.1
        """
        placed: list[tuple[str, int]] = []
        first_parent = self._git("rev-list", "--first-parent", "--parents", f"{commit}..HEAD").splitlines()
        for position, line in enumerate(first_parent):
            child, *parents = line.split()
            placed.append((child, position))
            if len(parents) > 1:
                side = self._git("rev-list", child, f"^{parents[0]}", f"^{commit}").split()
                placed.extend((sha, position) for sha in side if sha != child)
        return placed

    def paths(self, sha: str) -> set[str]:
        """The paths `sha` changed; a merge contributes none, its side commits do."""
        output = self._git("diff-tree", "--no-commit-id", "-r", "--name-only", "-z", sha)
        return {path for path in output.split("\0") if path}

    def _git(self, *args: str) -> str:
        result = _spawn(self.cwd, "git", *args)
        if result is None:
            raise ChangelogError("`git` is not on PATH")
        if result.returncode != 0:
            raise ChangelogError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
        return result.stdout


def _require_gh() -> None:
    if tool("gh") is None:
        # §FS-rhei-distribution.5.2
        raise ChangelogError("gh is not on PATH; a release's notes are read from the forge")


def _spawn(cwd: Path, *args: str) -> subprocess.CompletedProcess[str] | None:
    # The resolved path rather than the bare name: on Windows a bare name
    # reaches only a `.exe`, so a `gh` shim would read as a forge that could
    # not be asked. §REQ-cross-platform.3
    executable = tool(args[0])
    if executable is None:
        return None
    try:
        return subprocess.run([executable, *args[1:]], cwd=str(cwd), check=False, capture_output=True, text=True)
    except FileNotFoundError:
        return None


def _first_line(text: str) -> str:
    return next((line.strip() for line in text.splitlines() if line.strip()), "no message")


def _warn(message: str) -> None:
    print(f"warning: {message}", file=sys.stderr)


def extract_notes(changelog: Path, version: str, output: Path) -> None:
    _validate_version(version)
    lines = _read_lines(changelog)
    sections = _find_top_level_sections(lines)

    for index, section_start in enumerate(sections):
        match = RELEASE_RE.match(_line_text(lines[section_start]))
        if match is None or match.group("version") != version:
            continue
        section_end = sections[index + 1] if index + 1 < len(sections) else len(lines)
        body = _trim_blank_lines(lines[section_start + 1 : section_end])
        if not body:
            raise ChangelogError(f"release {version} has an empty changelog section")
        _write_lines(output, [*body, "\n"])
        return

    raise ChangelogError(f"release {version} is not the inline changelog release")


def _find_top_level_sections(lines: Sequence[str]) -> list[int]:
    return [index for index, line in enumerate(lines) if line.startswith("## ") and not line.startswith("### ")]


def _find_section_after(
    lines: Sequence[str], sections: Sequence[int], after: int, pattern: re.Pattern[str], name: str
) -> int:
    for section in sections:
        if section <= after:
            continue
        if pattern.match(_line_text(lines[section])):
            return section
    raise ChangelogError(f"missing {name} section")


def _read_lines(path: Path) -> list[str]:
    try:
        return path.read_text(encoding="utf-8").splitlines(keepends=True)
    except FileNotFoundError as exc:
        raise ChangelogError(f"missing changelog: {path}") from exc


def _write_lines(path: Path, lines: Sequence[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(lines), encoding="utf-8")


def _line_text(line: str) -> str:
    return line.rstrip("\r\n")


def _trim_blank_lines(lines: Sequence[str]) -> list[str]:
    trimmed = list(lines)
    while trimmed and not trimmed[0].strip():
        trimmed.pop(0)
    while trimmed and not trimmed[-1].strip():
        trimmed.pop()
    if trimmed and not trimmed[-1].endswith(("\n", "\r")):
        trimmed[-1] += "\n"
    return trimmed


def _drop_leading_blank_lines(lines: Sequence[str]) -> list[str]:
    trimmed = list(lines)
    while trimmed and not trimmed[0].strip():
        trimmed.pop(0)
    return trimmed


def _summary_from(lines: Sequence[str]) -> str:
    """The archive line's summary of a section: its count, where it was generated.

    A generated section is a list of entries, so it reads `<N> pull requests.`;
    a hand-written one, from before the release generated its notes, keeps its
    first sentence. §FS-rhei-distribution.5.1
    """
    written = [line for line in lines if line.strip()]
    if written and all(ENTRY_RE.match(_line_text(line)) for line in written):
        return f"{len(written)} pull request{'' if len(written) == 1 else 's'}."

    paragraph: list[str] = []
    for line in lines:
        stripped = line.strip()
        if not stripped:
            if paragraph:
                break
            continue
        if stripped.startswith("#"):
            continue
        paragraph.append(stripped)

    if not paragraph:
        return "release notes."

    text = re.sub(r"\s+", " ", " ".join(paragraph))
    first_sentence = re.match(r"(.+?[.!?])(?:\s|$)", text)
    if first_sentence is not None:
        return first_sentence.group(1)
    return text


def _rewrite_relative_links_for_archive(line: str) -> str:
    def rewrite(match: re.Match[str]) -> str:
        destination = match.group("destination")
        if destination.startswith(("#", "/", "../", "http://", "https://", "mailto:")):
            return match.group(0)
        fragment = match.group("fragment") or ""
        return f"{match.group('prefix')}../{destination}{fragment})"

    return re.sub(
        r"(?P<prefix>\]\()(?P<destination>[^)#][^)#]*)(?P<fragment>#[^)]*)?\)",
        rewrite,
        line,
    )


def _validate_version(version: str) -> None:
    if VERSION_RE.match(version) is None:
        raise ChangelogError(f"version must look like 0.1.0, got {version!r}")


def _validate_date(release_date: str) -> None:
    try:
        _datetime.date.fromisoformat(release_date)
    except ValueError as exc:
        raise ChangelogError(f"date must look like YYYY-MM-DD, got {release_date!r}") from exc


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Prepare or read docs/changelog.md release sections.")
    parser.add_argument("--changelog", type=Path, default=Path("docs/changelog.md"))
    subparsers = parser.add_subparsers(dest="command", required=True)

    prepare = subparsers.add_parser("prepare", help="generate the numbered release from the merged pull requests")
    prepare.add_argument("version")
    prepare.add_argument("--date", default=_datetime.date.today().isoformat())

    due = subparsers.add_parser("due", help="say whether the scheduled release is due, as a step output")
    due.add_argument("tag")
    due.add_argument("--output", type=Path, required=True)

    notes = subparsers.add_parser("notes", help="write release notes for the inline release")
    notes.add_argument("version")
    notes.add_argument("--output", type=Path, required=True)

    args = parser.parse_args(argv)
    try:
        if args.command == "prepare":
            prepare_release(args.changelog, args.version, args.date)
        elif args.command == "due":
            release_due(args.changelog, args.tag, args.output)
        elif args.command == "notes":
            extract_notes(args.changelog, args.version, args.output)
        else:
            raise AssertionError(args.command)
    except ChangelogError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
