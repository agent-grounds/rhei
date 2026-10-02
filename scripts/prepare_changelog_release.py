#!/usr/bin/env python3
"""Prepare and read changelog release sections. §FS-rhei-distribution.5

`stamp` writes the pull request numbers the contributors did not know onto the
bullets they left, and runs before `prepare` promotes the section, because the
numbers are resolved from the commits the bullets were written in
(§FS-rhei-distribution.5.2).

`due` is the scheduled release's gate: it holds while code has merged since
`Unreleased` was last written, so no release ships code its section does not
describe (§FS-rhei-distribution.5.3).
"""

from __future__ import annotations

import argparse
import datetime as _datetime
import re
import subprocess
import sys
from pathlib import Path
from typing import Sequence

import changelog_bullets
from changelog_bullets import Bullet
from tool_lookup import tool


VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
UNRELEASED_RE = re.compile(r"^## Unreleased\s*$")
RELEASE_RE = re.compile(
    r"^## (?P<number>[0-9]+)\. \[(?P<version>[0-9]+\.[0-9]+\.[0-9]+)\] - (?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})\s*$"
)
OLDER_RE = re.compile(r"^## (?P<number>[0-9]+)\. Older releases\s*$")
# What a release is not cut for: docs and CI. §FS-rhei-distribution.5.1
DOCS_AND_CI_RE = re.compile(r"^(?:docs/|\.github/)|\.md$|^(?:LICENSE|lychee\.toml)$")
BLAME_HEADER_RE = re.compile(r"^(?P<sha>[0-9a-f]{40,64}) [0-9]+ [0-9]+")
UNCOMMITTED_RE = re.compile(r"^0+$")


class ChangelogError(Exception):
    pass


def prepare_release(changelog: Path, version: str, release_date: str) -> None:
    _validate_version(version)
    _validate_date(release_date)

    lines = _read_lines(changelog)
    sections = _find_top_level_sections(lines)
    unreleased = _find_section(lines, sections, UNRELEASED_RE, "## Unreleased")
    latest = _next_section_after(sections, unreleased, "latest release")
    older = _find_section_after(lines, sections, latest, OLDER_RE, "Older releases")

    latest_match = RELEASE_RE.match(_line_text(lines[latest]))
    if latest_match is None:
        raise ChangelogError(f"expected latest release heading after ## Unreleased, got: {_line_text(lines[latest])}")

    if latest_match.group("version") == version:
        raise ChangelogError(f"docs/changelog.md already has {version} as the inline latest release")

    unreleased_body = _trim_blank_lines(lines[unreleased + 1 : latest])
    if not _has_bullet(unreleased_body):
        raise ChangelogError("## Unreleased has no bullet entries to promote")

    previous_version = latest_match.group("version")
    previous_date = latest_match.group("date")
    previous_body = lines[latest + 1 : older]
    archived_body = [_rewrite_relative_links_for_archive(line) for line in previous_body]
    summary = _summary_from(previous_body)

    archive_path = changelog.parent / "changelog" / f"{previous_version}.md"
    if archive_path.exists():
        raise ChangelogError(f"archive already exists: {archive_path}")

    archive_lines = [f"# {previous_version} - {previous_date}\n", *archived_body]
    _write_lines(archive_path, archive_lines)

    older_body = lines[older + 1 :]
    older_body = _drop_leading_blank_lines(older_body)
    archive_link = f"- [{previous_version}](changelog/{previous_version}.md) - {previous_date}: {summary}\n"

    new_lines = [
        *lines[: unreleased + 1],
        "\n",
        f"## 2. [{version}] - {release_date}\n",
        "\n",
        *unreleased_body,
        "\n",
        "## 3. Older releases\n",
        "\n",
        archive_link,
        *older_body,
    ]
    _write_lines(changelog, new_lines)


def stamp_pull_requests(changelog: Path) -> None:
    """Write `PR #<n>` onto every Unreleased bullet that resolves to one.

    It is allowed to achieve nothing. A bullet it cannot resolve to exactly one
    pull request is left as written and reported, and so is a forge it cannot
    reach at all: a release that could not be cut over a changelog annotation
    would cost more than the missing annotation does. §FS-rhei-distribution.5.2
    """
    lines = _read_lines(changelog)
    try:
        unstamped = [bullet for bullet in changelog_bullets.bullets(lines) if not bullet.is_stamped]
    except changelog_bullets.ChangelogFormatError as exc:
        raise ChangelogError(str(exc)) from exc

    if not unstamped:
        return
    if tool("gh") is None:
        _warn("`gh` is not on PATH, so no pull request could be resolved; nothing stamped")
        return

    forge = _Forge(changelog)
    stamped = False
    for bullet in unstamped:
        number, reason = forge.pull_request_for(bullet)
        if number is None:
            _warn(
                f"docs/changelog.md ## Unreleased: bullet at line {bullet.first + 1} "
                f"left unstamped ({reason})"
            )
            continue
        # Stamping never changes a bullet's line count, so the ranges the
        # remaining bullets carry stay valid as this writes through them.
        lines[bullet.first : bullet.last + 1] = bullet.stamped(number)
        stamped = True

    if stamped:
        _write_lines(changelog, lines)


class _Forge:
    """Which pull request a bullet's lines were written in, asked of git and gh."""

    def __init__(self, changelog: Path) -> None:
        resolved = changelog.resolve()
        self.cwd = resolved.parent if resolved.parent.is_dir() else Path.cwd()
        self.name = resolved.name
        self._cache: dict[str, set[int] | None] = {}

    def pull_request_for(self, bullet: Bullet) -> tuple[int | None, str]:
        shas = self._blame(bullet.first + 1, bullet.last + 1)
        if shas is None:
            return None, "git blame could not read its lines"
        if not shas:
            return None, "it has no committed lines to resolve"
        if any(UNCOMMITTED_RE.match(sha) for sha in shas):
            return None, "one of its lines is not committed yet"

        numbers: set[int] = set()
        for sha in shas:
            resolved = self._pulls(sha)
            if resolved is None:
                return None, f"the forge could not be asked about commit {sha[:7]}"
            if not resolved:
                return None, f"commit {sha[:7]} belongs to no pull request"
            numbers |= resolved
        if len(numbers) != 1:
            named = ", ".join(f"#{number}" for number in sorted(numbers))
            return None, f"its lines belong to more than one pull request ({named})"
        return numbers.pop(), ""

    def _blame(self, first: int, last: int) -> list[str] | None:
        result = _spawn(self.cwd, "git", "blame", "--line-porcelain", "-L", f"{first},{last}", "--", self.name)
        if result is None or result.returncode != 0:
            return None
        shas: list[str] = []
        sha = None
        for line in result.stdout.splitlines():
            header = BLAME_HEADER_RE.match(line)
            if header:
                sha = header.group("sha")
            elif line.startswith("\t") and line[1:].strip() and sha is not None:
                shas.append(sha)
        return list(dict.fromkeys(shas))

    def _pulls(self, sha: str) -> set[int] | None:
        if sha not in self._cache:
            self._cache[sha] = self._ask(sha)
        return self._cache[sha]

    def _ask(self, sha: str) -> set[int] | None:
        result = _spawn(
            self.cwd, "gh", "api", f"repos/{{owner}}/{{repo}}/commits/{sha}/pulls", "--jq", ".[].number"
        )
        if result is None or result.returncode != 0:
            return None
        return {int(token) for token in result.stdout.split() if token.isdigit()}


def release_due(changelog: Path, tag: str, output: Path) -> None:
    """Answer `Auto bump`'s gate step: is a release due since `tag`?

    `ok=true` goes to `output` only when code has merged since the tag and
    `Unreleased` was last written after all of it. Otherwise the step holds with
    a notice that says why, and a hold exits 0 like a release does: it is the
    expected answer between a merge and its write-up, and only an error fails
    the run. §FS-rhei-distribution.5.3
    """
    history = _History(changelog)
    tagged = history.commit(tag)
    due = False
    if not history.code_changed_since(tagged):
        print(f"::notice::Only docs/CI changes since {tag}; skipping.")
    else:
        # The tag sits on the release commit, which empties the section, so
        # with no write since, the tag is the last write.
        written = history.last_write_after(tagged) or tagged
        waiting = history.code_changed_since(written)
        if waiting:
            print(
                f"::notice::Changes merged since ## Unreleased was last written ({written[:7]}) "
                "wait for their release section; skipping."
            )
            for path in waiting:
                print(f"  {path}")
        else:
            due = True
    with output.open("a", encoding="utf-8") as handle:
        handle.write(f"ok={'true' if due else 'false'}\n")


class _History:
    """What changed since a commit, and when `Unreleased` was last written, asked of git."""

    def __init__(self, changelog: Path) -> None:
        resolved = changelog.resolve()
        self.cwd = resolved.parent if resolved.parent.is_dir() else Path.cwd()
        self.name = resolved.name
        self._bullets: dict[str, tuple[str, ...]] = {}

    def commit(self, ref: str) -> str:
        return self._git("rev-parse", "--verify", f"{ref}^{{commit}}").strip()

    def code_changed_since(self, commit: str) -> list[str]:
        """The paths outside docs and CI that differ between `commit` and `HEAD`."""
        paths = self._git("diff", "--name-only", "-z", commit, "HEAD").split("\0")
        return [path for path in paths if path and not DOCS_AND_CI_RE.search(path)]

    def last_write_after(self, commit: str) -> str | None:
        """The newest first-parent commit after `commit` that changed `Unreleased`'s bullets.

        A bullet is read as §FS-rhei-distribution.5.1 reads it, so an edit to the
        note above the section, or to the blank lines between bullets, is not a
        write. §FS-rhei-distribution.5.3
        """
        for line in self._git("rev-list", "--first-parent", "--parents", f"{commit}..HEAD").splitlines():
            child, *parents = line.split()
            before = self._unreleased(parents[0]) if parents else ()
            if self._unreleased(child) != before:
                return child
        return None

    def _unreleased(self, commit: str) -> tuple[str, ...]:
        """The bullets of `Unreleased` at `commit`; none where it has no such section."""
        if commit not in self._bullets:
            # `./` makes the path relative to `cwd`, which is the changelog's own directory.
            result = _spawn(self.cwd, "git", "show", f"{commit}:./{self.name}")
            if result is None:
                raise ChangelogError("`git` is not on PATH")
            lines = result.stdout.splitlines(keepends=True) if result.returncode == 0 else []
            try:
                found = changelog_bullets.bullets(lines)
            except changelog_bullets.ChangelogFormatError:
                found = []
            self._bullets[commit] = tuple(bullet.raw for bullet in found)
        return self._bullets[commit]

    def _git(self, *args: str) -> str:
        result = _spawn(self.cwd, "git", *args)
        if result is None:
            raise ChangelogError("`git` is not on PATH")
        if result.returncode != 0:
            raise ChangelogError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
        return result.stdout


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


def _find_section(lines: Sequence[str], sections: Sequence[int], pattern: re.Pattern[str], name: str) -> int:
    for section in sections:
        if pattern.match(_line_text(lines[section])):
            return section
    raise ChangelogError(f"missing {name} section")


def _find_section_after(
    lines: Sequence[str], sections: Sequence[int], after: int, pattern: re.Pattern[str], name: str
) -> int:
    for section in sections:
        if section <= after:
            continue
        if pattern.match(_line_text(lines[section])):
            return section
    raise ChangelogError(f"missing {name} section")


def _next_section_after(sections: Sequence[int], after: int, name: str) -> int:
    for section in sections:
        if section > after:
            return section
    raise ChangelogError(f"missing {name}")


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


def _has_bullet(lines: Sequence[str]) -> bool:
    return any(line.lstrip().startswith("- ") for line in lines)


def _summary_from(lines: Sequence[str]) -> str:
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

    prepare = subparsers.add_parser("prepare", help="promote Unreleased into a numbered release")
    prepare.add_argument("version")
    prepare.add_argument("--date", default=_datetime.date.today().isoformat())

    subparsers.add_parser("stamp", help="write resolved pull request numbers onto Unreleased bullets")

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
        elif args.command == "stamp":
            stamp_pull_requests(args.changelog)
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
