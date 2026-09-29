#!/usr/bin/env python3
"""Require a pull request to be represented in the Unreleased changelog.

Two rules over one file, run by the pre-push hook (§FS-rhei-distribution.6) and
by CI (§AR-ci-release.1) through this one implementation, because two gates
disagreeing about `docs/changelog.md` is the defect this exists to close:

R1  `Unreleased` must hold at least one bullet the same section does not already
    hold at the merge base of the branch's base and head. It needs no pull
    request number, which is why the hook can run before one exists.
R2  Where a number is known, a bullet R1 counted may not carry a different
    pull request's number. The placeholder is not a number.

§FS-rhei-distribution.5.1
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Sequence

from changelog_bullets import Bullet, ChangelogFormatError, bullets, bullets_in

ZERO_REF_RE = re.compile(r"^0+$")

ADD_A_BULLET = "add a bullet under `## Unreleased` describing this change."
NO_NUMBER_NEEDED = "You do not need the pull request number"
SKIP_LINE = (
    "Pushing a branch that is not becoming a pull request: SKIP=changelog-pr-entry git push"
)


class ChangelogPrError(Exception):
    pass


def check_changelog_pr_entry(
    changelog: Path,
    pr_number: int | None,
    base_ref: str | None,
    to_ref: str | None,
    local: bool,
) -> None:
    """Hold the changelog to R1 and R2, or explain which skip applied."""
    git = _Git(changelog)
    head_ref = to_ref or "HEAD"
    head_lines = _head_lines(changelog, git, to_ref)

    base_label = base_ref or _resolve_base(git)
    if base_label is None:
        _check_without_a_base(head_lines, git)
        return

    merge_base = git.merge_base(base_label, head_ref)
    if merge_base is None:
        _check_without_a_base(head_lines, git, tried=[base_label])
        return
    if merge_base == git.commit(head_ref):
        # `git push origin main` after a fast-forward: the head adds nothing to
        # the base, so there is nothing for a bullet to describe. §FS-rhei-distribution.5.1
        print(f"{head_ref} is already contained in {base_label}; nothing added to describe")
        return

    added = _added_bullets(head_lines, git.show(merge_base))
    if not added:
        raise ChangelogPrError(_no_bullet_message(base_label, local))
    if pr_number is not None:
        _check_the_number(added, pr_number)


def _added_bullets(head_lines: Sequence[str], base_text: str | None) -> list[Bullet]:
    """The head's bullets that the base's section does not already hold. R1"""
    try:
        base = bullets_in(base_text) if base_text is not None else []
    except ChangelogFormatError:
        # The base had no `Unreleased` section. That is history rather than the
        # state being gated, so every bullet in the head counts as added.
        base = []

    remaining = Counter(bullet.text for bullet in base)
    added = []
    for bullet in _head_bullets(head_lines):
        if remaining[bullet.text] > 0:
            remaining[bullet.text] -= 1
        else:
            added.append(bullet)
    return added


def _check_the_number(added: Sequence[Bullet], pr_number: int) -> None:
    """A bullet this pull request added may not name another one. R2"""
    for bullet in added:
        wrong = sorted(number for number in bullet.numbers if number != pr_number)
        if wrong:
            named = ", ".join(f"PR #{number}" for number in wrong)
            raise ChangelogPrError(
                f"docs/changelog.md ## Unreleased: a bullet added by this pull request "
                f"names {named}, not PR #{pr_number}; remove the number or correct it - "
                f"leaving it out is fine, the release fills it in."
            )


def _check_without_a_base(head_lines: Sequence[str], git: _Git, tried: Sequence[str] | None = None) -> None:
    """No base to compare against: require a bullet, and say so.

    A refusal a contributor cannot act on is worse than the CI failure it is
    preventing, so this degrades rather than refuses. R2 is not applied: it is
    scoped to the bullets R1 counted, and without a base there is no such set.
    §FS-rhei-distribution.6
    """
    names = list(tried) if tried else git.base_candidates()
    remote = names[0].split("/")[0] if "/" in names[0] else "origin"
    print(
        f"warning: docs/changelog.md: no base to compare against (tried {', '.join(names)}); "
        f"checking only that ## Unreleased holds a bullet\n"
        f"  run `git fetch {remote} main` so the check can compare this branch against its base",
        file=sys.stderr,
    )
    if not _head_bullets(head_lines):
        raise ChangelogPrError(f"docs/changelog.md ## Unreleased holds no bullet; {ADD_A_BULLET}")


def _no_bullet_message(base_label: str, local: bool) -> str:
    lines = [
        f"docs/changelog.md ## Unreleased has no new or changed bullet against {base_label}",
        f"  {ADD_A_BULLET}",
        f"  {NO_NUMBER_NEEDED} - the release fills it in.",
    ]
    if local:
        lines.append(f"  {SKIP_LINE}")
    return "\n".join(lines)


def _head_bullets(head_lines: Sequence[str]) -> list[Bullet]:
    try:
        return bullets(head_lines)
    except ChangelogFormatError as exc:
        raise ChangelogPrError(str(exc)) from exc


def _head_lines(changelog: Path, git: _Git, to_ref: str | None) -> list[str]:
    """The section as it is being pushed: a named revision, else the working tree."""
    if to_ref is not None:
        text = git.show(to_ref)
        if text is None:
            raise ChangelogPrError(f"docs/changelog.md does not exist at {to_ref}")
        return text.splitlines()
    try:
        return changelog.read_text(encoding="utf-8").splitlines()
    except FileNotFoundError as exc:
        raise ChangelogPrError(f"missing changelog: {changelog}") from exc


def _resolve_base(git: _Git) -> str | None:
    for candidate in git.base_candidates():
        if git.commit(candidate) is not None:
            return candidate
    return None


class _Git:
    """Git reads about the changelog, rooted wherever the changelog lives."""

    def __init__(self, changelog: Path) -> None:
        resolved = changelog.resolve()
        self.cwd = resolved.parent if resolved.parent.is_dir() else Path.cwd()
        self.root = self._root()
        self.relative = self._relative(resolved)

    def base_candidates(self) -> list[str]:
        """The branch's base, in the order the hook and CI agree on."""
        remote = os.environ.get("PRE_COMMIT_REMOTE_NAME")
        names = [f"{remote}/main"] if remote else []
        names += ["origin/main", "main"]
        return list(dict.fromkeys(names))

    def commit(self, ref: str) -> str | None:
        result = self._run("rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}")
        return result.stdout.strip() or None if result.returncode == 0 else None

    def merge_base(self, base: str, head: str) -> str | None:
        result = self._run("merge-base", base, head)
        return result.stdout.strip() or None if result.returncode == 0 else None

    def show(self, rev: str) -> str | None:
        if self.relative is None:
            return None
        result = self._run("show", f"{rev}:{self.relative}")
        return result.stdout if result.returncode == 0 else None

    def _root(self) -> Path | None:
        result = self._run("rev-parse", "--show-toplevel")
        if result.returncode != 0:
            return None
        top = result.stdout.strip()
        return Path(top).resolve() if top else None

    def _relative(self, resolved: Path) -> str | None:
        if self.root is None:
            return None
        try:
            return resolved.relative_to(self.root).as_posix()
        except ValueError:
            return None

    def _run(self, *args: str) -> subprocess.CompletedProcess[str]:
        try:
            return subprocess.run(
                ["git", *args], cwd=str(self.cwd), check=False, capture_output=True, text=True
            )
        except FileNotFoundError:
            return subprocess.CompletedProcess(args, 1, "", "git is not on PATH")


def pr_number_from_event(event_path: Path) -> int | None:
    try:
        event = json.loads(event_path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ChangelogPrError(f"missing GitHub event file: {event_path}") from exc
    except json.JSONDecodeError as exc:
        raise ChangelogPrError(f"invalid GitHub event JSON: {event_path}: {exc}") from exc

    pull_request = event.get("pull_request")
    if isinstance(pull_request, dict):
        number = pull_request.get("number")
    else:
        number = event.get("number") if event.get("pull_request") is not None else None

    if number is None:
        return None
    if not isinstance(number, int) or number <= 0:
        raise ChangelogPrError(f"invalid pull request number in event: {number!r}")
    return number


def pr_number_from_current_branch() -> int | None:
    try:
        result = subprocess.run(
            ["gh", "pr", "view", "--json", "number", "--jq", ".number"],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return None

    if result.returncode != 0:
        return None
    output = result.stdout.strip()
    if not output:
        return None
    try:
        number = int(output)
    except ValueError as exc:
        raise ChangelogPrError(f"invalid pull request number from gh: {output!r}") from exc
    if number <= 0:
        raise ChangelogPrError(f"invalid pull request number from gh: {number!r}")
    return number


def _resolve_pr_number(args: argparse.Namespace) -> int | None:
    if args.pr_number is not None:
        return args.pr_number

    event_path = args.event_path
    if event_path is None:
        raw = os.environ.get("GITHUB_EVENT_PATH")
        event_path = Path(raw) if raw else None
    number = pr_number_from_event(event_path) if event_path is not None else None

    if number is None and args.local_pr:
        number = pr_number_from_current_branch()
    return number


def _named_ref(value: str | None, variable: str | None = None) -> str | None:
    """A revision given on the command line or by pre-commit, if it names one."""
    if value is None and variable is not None:
        value = os.environ.get(variable)
    if value is None:
        return None
    value = value.strip()
    if not value or ZERO_REF_RE.match(value):
        return None
    return value


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check that this pull request adds an Unreleased changelog bullet."
    )
    parser.add_argument("--changelog", type=Path, default=Path("docs/changelog.md"))
    parser.add_argument("--pr-number", type=int)
    parser.add_argument("--event-path", type=Path, default=None)
    parser.add_argument(
        "--base-ref",
        default=None,
        help="the commit to compare the section against; resolved from the branch when absent",
    )
    parser.add_argument(
        "--to-ref",
        default=None,
        help="the revision being pushed; defaults to $PRE_COMMIT_TO_REF, else the working tree",
    )
    parser.add_argument(
        "--local-pr",
        action="store_true",
        help="resolve the current branch PR with gh, and name SKIP= in a refusal",
    )
    args = parser.parse_args(argv)

    try:
        pr_number = _resolve_pr_number(args)
        if pr_number is not None and pr_number <= 0:
            raise ChangelogPrError(f"invalid pull request number: {pr_number}")
        check_changelog_pr_entry(
            args.changelog,
            pr_number,
            _named_ref(args.base_ref),
            _named_ref(args.to_ref, "PRE_COMMIT_TO_REF"),
            args.local_pr,
        )
    except ChangelogPrError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
