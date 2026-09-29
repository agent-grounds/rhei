#!/usr/bin/env python3
"""One definition of an `Unreleased` changelog bullet. §FS-rhei-distribution.5.1

The pull request check (`check_changelog_pr_entry.py`) and the release stamper
(`prepare_changelog_release.py stamp`) both read `docs/changelog.md` through
this module. The defect they exist to close is two gates disagreeing about one
file, so a bullet the check counted has to be a bullet the stamper can find, and
that only holds while there is a single definition of what a bullet is
(§FS-rhei-distribution.5.2).

A bullet is its `- ` line together with every following line up to the next
bullet or the end of the section; trailing blank lines belong to no bullet, so a
bullet's last line is the last line carrying text. Two bullets are the same
bullet when their text matches once runs of whitespace collapse to single spaces
and a trailing pull-request token is dropped.
"""

from __future__ import annotations

import re
from typing import Sequence

UNRELEASED_RE = re.compile(r"^## Unreleased\s*$")
TOP_LEVEL_RE = re.compile(r"^##(?!#)\s+")
BULLET_RE = re.compile(r"^- ")

PLACEHOLDER = "TBD"

# The token a bullet ends with, which normalisation drops: the number the
# release stamped, or the placeholder that stands in for one until it does.
_TRAILING_TOKEN_RE = re.compile(
    r"(?i)\s*(?:\(\s*(?:PR|pull request)\s*#\s*(?P<parenthesised>[0-9]+|TBD)\s*\)"
    r"|(?:PR|pull request)\s*#\s*(?P<bare>[0-9]+|TBD))\s*$"
)

# Every way this changelog has ever named a pull request, so that a number
# written in any of them is checked rather than only the canonical form.
_NUMBER_RES = (
    re.compile(r"(?i)\bPR\s*#\s*(?P<number>[0-9]+)\b"),
    re.compile(r"(?i)\bpull request\s*#\s*(?P<number>[0-9]+)\b"),
    re.compile(r"/pull/(?P<number>[0-9]+)(?:\b|[/#?)])"),
)

_PLACEHOLDER_PARENTHESISED_RE = re.compile(r"(?i)\(\s*(?:PR|pull request)\s*#\s*TBD\s*\)")
_PLACEHOLDER_BARE_RE = re.compile(r"(?i)\b(?:PR|pull request)\s*#\s*TBD\b")


class ChangelogFormatError(Exception):
    """The file is not shaped like a changelog this module can read."""


class Bullet:
    """One bullet of the `Unreleased` section, and where it sits in the file.

    `first` and `last` are zero-based indices into the lines the bullet was read
    from, `last` being the last line carrying text. §FS-rhei-distribution.5.1
    """

    def __init__(self, first: int, last: int, lines: Sequence[str]) -> None:
        self.first = first
        self.last = last
        self.lines = list(lines)

    @property
    def raw(self) -> str:
        """The bullet as written, one space per line break."""
        return " ".join(line_text(line) for line in self.lines)

    @property
    def text(self) -> str:
        """What decides whether two bullets are the same bullet."""
        return _TRAILING_TOKEN_RE.sub("", _collapse(self.raw)).strip()

    @property
    def token(self) -> str | None:
        """The trailing pull-request token: a number, `TBD`, or nothing."""
        match = _TRAILING_TOKEN_RE.search(_collapse(self.raw))
        if match is None:
            return None
        return match.group("parenthesised") or match.group("bare")

    @property
    def numbers(self) -> frozenset[int]:
        """Every pull request number the bullet names, wherever it names it."""
        raw = self.raw
        return frozenset(
            int(match.group("number")) for pattern in _NUMBER_RES for match in pattern.finditer(raw)
        )

    @property
    def is_stamped(self) -> bool:
        """It already carries a real number, so the release leaves it alone."""
        token = self.token
        return token is not None and token.upper() != PLACEHOLDER

    def stamped(self, number: int) -> list[str]:
        """This bullet's lines with `number` written onto it.

        A placeholder is replaced where it stands rather than followed by a
        second token; with no placeholder the number is appended to the end of
        the bullet's last line. §FS-rhei-distribution.5.2
        """
        replacement = f"(PR #{number})"
        lines = list(self.lines)
        for index in reversed(range(len(lines))):
            substituted, count = _PLACEHOLDER_PARENTHESISED_RE.subn(replacement, lines[index])
            if count == 0:
                substituted, count = _PLACEHOLDER_BARE_RE.subn(f"PR #{number}", lines[index])
            if count:
                lines[index] = substituted
                return lines

        last = len(lines) - 1
        body, ending = _split_ending(lines[last])
        lines[last] = f"{body.rstrip()} (PR #{number}){ending}"
        return lines


def line_text(line: str) -> str:
    """A line without its ending, so callers may pass lines either way."""
    return line.rstrip("\r\n")


def unreleased_range(lines: Sequence[str]) -> tuple[int, int]:
    """The `## Unreleased` body as a half-open range of line indices."""
    start = None
    for index, line in enumerate(lines):
        if UNRELEASED_RE.match(line_text(line)):
            start = index + 1
            break
    if start is None:
        raise ChangelogFormatError("missing ## Unreleased section in docs/changelog.md")

    for index in range(start, len(lines)):
        if TOP_LEVEL_RE.match(line_text(lines[index])):
            return start, index
    return start, len(lines)


def bullets(lines: Sequence[str]) -> list[Bullet]:
    """Every bullet of the `Unreleased` section, in the order it is written."""
    start, end = unreleased_range(lines)
    found: list[Bullet] = []
    opened: int | None = None
    last_with_text = start

    def close() -> None:
        if opened is not None:
            found.append(Bullet(opened, last_with_text, lines[opened : last_with_text + 1]))

    for index in range(start, end):
        text = line_text(lines[index])
        if BULLET_RE.match(text):
            close()
            opened = index
            last_with_text = index
        elif opened is not None and text.strip():
            last_with_text = index
    close()
    return found


def bullets_in(text: str) -> list[Bullet]:
    """The bullets of a changelog held as one string rather than as lines."""
    return bullets(text.splitlines())


def _collapse(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def _split_ending(line: str) -> tuple[str, str]:
    stripped = line_text(line)
    return stripped, line[len(stripped) :]
