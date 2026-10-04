#!/usr/bin/env python3
"""One definition of an `Unreleased` changelog bullet. §FS-rhei-distribution.5.1

The release stamper (`prepare_changelog_release.py stamp`,
§FS-rhei-distribution.5.2), the scheduled release's gate (`due`, which counts a
commit as writing the section only when it changed the bullets,
§FS-rhei-distribution.5.3) and the test that checks the repository's own
changelog has the shape the release reads all read `docs/changelog.md` through
this module, so a bullet the write-up passed that check with is a bullet the
stamper can find and the gate can see.

A bullet is its `- ` line together with every following line up to the next
bullet or the end of the section; trailing blank lines belong to no bullet, so a
bullet's last line is the last line carrying text.
"""

from __future__ import annotations

import re
from typing import Sequence

UNRELEASED_RE = re.compile(r"^## Unreleased\s*$")
TOP_LEVEL_RE = re.compile(r"^##(?!#)\s+")
BULLET_RE = re.compile(r"^- ")

PLACEHOLDER = "TBD"

# The token a bullet ends with, a stamped number or the placeholder, matched over
# its lines joined by line breaks so a split token is still the bullet's own and
# its span says where it stands. §FS-rhei-distribution.5.2
_TRAILING_TOKEN_RE = re.compile(
    r"(?i)(?P<token>\(\s*(?:PR|pull\s+request)\s*#\s*(?P<parenthesised>[0-9]+|TBD)\s*\)"
    r"|(?:PR|pull\s+request)\s*#\s*(?P<bare>[0-9]+|TBD))\s*$"
)


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
    def token(self) -> str | None:
        """The trailing pull-request token: a number, `TBD`, or nothing."""
        match = self._trailing_token()
        if match is None:
            return None
        return match.group("parenthesised") or match.group("bare")

    @property
    def is_stamped(self) -> bool:
        """It already carries a real number, so the release leaves it alone."""
        token = self.token
        return token is not None and token.upper() != PLACEHOLDER

    def stamped(self, number: int) -> list[str]:
        """This bullet's lines with `number` written onto it, as many as it was given.

        The placeholder the bullet ends with, the token `token` reads, is replaced
        where it stands and in its own form rather than followed by a second
        token; a placeholder quoted in the prose is the bullet's text and is left
        as written. With no trailing placeholder the number is appended to the
        end of the bullet's last line. §FS-rhei-distribution.5.2
        """
        lines = list(self.lines)
        match = self._trailing_token()
        if match is None or (match.group("parenthesised") or match.group("bare")).upper() != PLACEHOLDER:
            last = len(lines) - 1
            body, ending = _split_ending(lines[last])
            lines[last] = f"{body.rstrip()} (PR #{number}){ending}"
            return lines

        replacement = f"(PR #{number})" if match.group("parenthesised") else f"PR #{number}"
        start_line, start_column = self._position(match.start("token"))
        end_line, end_column = self._position(match.end("token"))
        if start_line == end_line:
            body, ending = _split_ending(lines[start_line])
            lines[start_line] = f"{body[:start_column]}{replacement}{body[end_column:]}{ending}"
            return lines

        # A token split across lines is written whole on the line it ends on, at
        # that line's indentation, so the bullet keeps its line count.
        body, ending = _split_ending(lines[start_line])
        lines[start_line] = f"{body[:start_column].rstrip()}{ending}"
        for index in range(start_line + 1, end_line):
            lines[index] = _split_ending(lines[index])[1]
        body, ending = _split_ending(lines[end_line])
        indent = body[: len(body) - len(body.lstrip())]
        lines[end_line] = f"{indent}{replacement}{body[end_column:]}{ending}"
        return lines

    def _text(self) -> str:
        return "\n".join(line_text(line) for line in self.lines)

    def _trailing_token(self) -> re.Match[str] | None:
        return _TRAILING_TOKEN_RE.search(self._text())

    def _position(self, offset: int) -> tuple[int, int]:
        """The line and column of an offset into `_text`."""
        before = self._text()[:offset]
        line = before.count("\n")
        return line, offset - (before.rfind("\n") + 1)


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


def _split_ending(line: str) -> tuple[str, str]:
    stripped = line_text(line)
    return stripped, line[len(stripped) :]
