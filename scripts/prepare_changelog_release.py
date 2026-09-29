#!/usr/bin/env python3
"""Prepare and read changelog release sections. §FS-rhei-distribution.5

`stamp` writes the pull request numbers the contributors did not know onto the
bullets they left, and runs before `prepare` promotes the section, because the
numbers are resolved from the commits the bullets were written in
(§FS-rhei-distribution.5.2).
"""

from __future__ import annotations

import argparse
import datetime as _datetime
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Sequence

import changelog_bullets
from changelog_bullets import Bullet


VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
UNRELEASED_RE = re.compile(r"^## Unreleased\s*$")
RELEASE_RE = re.compile(
    r"^## (?P<number>[0-9]+)\. \[(?P<version>[0-9]+\.[0-9]+\.[0-9]+)\] - (?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})\s*$"
)
OLDER_RE = re.compile(r"^## (?P<number>[0-9]+)\. Older releases\s*$")
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
    if shutil.which("gh") is None:
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
        result = self._run("git", "blame", "--line-porcelain", "-L", f"{first},{last}", "--", self.name)
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
        result = self._run(
            "gh", "api", f"repos/{{owner}}/{{repo}}/commits/{sha}/pulls", "--jq", ".[].number"
        )
        if result is None or result.returncode != 0:
            return None
        return {int(token) for token in result.stdout.split() if token.isdigit()}

    def _run(self, *args: str) -> subprocess.CompletedProcess[str] | None:
        try:
            return subprocess.run(
                list(args), cwd=str(self.cwd), check=False, capture_output=True, text=True
            )
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

    notes = subparsers.add_parser("notes", help="write release notes for the inline release")
    notes.add_argument("version")
    notes.add_argument("--output", type=Path, required=True)

    args = parser.parse_args(argv)
    try:
        if args.command == "prepare":
            prepare_release(args.changelog, args.version, args.date)
        elif args.command == "stamp":
            stamp_pull_requests(args.changelog)
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
