#!/usr/bin/env python3
"""Fixtures for the release-notes generator, release-due and shape tests. §FS-rhei-distribution.5

The tests drive the scripts as subprocesses over a throwaway git repository, so
what they pin is the contract a maintainer and a workflow actually meet: the
command line, the exit code, and the message. Nothing here imports the scripts.

`gh` is answered by a stub on `PATH` rather than by the forge, which is the seam
the reproducer for agent-grounds/rhei#341 established and the only one the
generator can be tested through at all. The stub answers
`gh api repos/{owner}/{repo}/commits/<sha>/pulls` with the pull request objects
the forge returns, records every call it is given, and fails for the commits it
is told to.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RELEASE_SCRIPT = REPO_ROOT / "scripts" / "prepare_changelog_release.py"
PRE_COMMIT_CONFIG = REPO_ROOT / ".pre-commit-config.yaml"
CI_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "ci.yml"
AUTO_BUMP_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "auto-bump.yml"
RELEASE_MINOR_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "release-minor.yml"
CHANGELOG = REPO_ROOT / "docs" / "changelog.md"
SCRIPTS = REPO_ROOT / "scripts"

WINDOWS = os.name == "nt"

# Anything that would let the environment the tests run in decide the answer:
# a GitHub event file, pre-commit's push refs, or the caller's own git identity.
_STRIPPED = (
    "GITHUB_EVENT_PATH",
    "GITHUB_REF",
    "GITHUB_BASE_REF",
    "PRE_COMMIT_FROM_REF",
    "PRE_COMMIT_TO_REF",
    "PRE_COMMIT_REMOTE_NAME",
    "PRE_COMMIT_REMOTE_URL",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_CONFIG_DIR",
)


def clean_env(**overrides: str | None) -> dict[str, str]:
    """The ambient environment with everything that could answer for us removed.

    Every `GIT_*` variable goes, not only the ones listed above. Git exports
    `GIT_DIR` into the subprocesses of a hook it invokes, and these tests run as a
    `pre-commit` hook, so a `GIT_DIR` left in place aims `TempRepo`'s own `git
    init`, `checkout -b` and `commit` at the repository being committed to rather
    than at the temporary directory - real branches written, real `HEAD` moved.
    """
    env = {
        key: value
        for key, value in os.environ.items()
        if key not in _STRIPPED and not key.startswith("GIT_")
    }
    env["GIT_CONFIG_GLOBAL"] = os.devnull
    env["GIT_CONFIG_SYSTEM"] = os.devnull
    env["GIT_AUTHOR_NAME"] = env["GIT_COMMITTER_NAME"] = "Changelog Test"
    env["GIT_AUTHOR_EMAIL"] = env["GIT_COMMITTER_EMAIL"] = "test@example.invalid"
    env["GIT_AUTHOR_DATE"] = env["GIT_COMMITTER_DATE"] = "2026-01-01T00:00:00+00:00"
    for key, value in overrides.items():
        if value is None:
            env.pop(key, None)
        else:
            env[key] = value
    return env


GENERATED_NOTE = (
    "*Each release's section is generated from the pull requests merged since the previous "
    "release; nothing here is written by hand.*"
)

RELEASED_ENTRIES = (
    "- [The first release's second change](https://github.com/agent-grounds/rhei/pull/2) (PR #2)",
    "- [The first release's first change](https://github.com/agent-grounds/rhei/pull/1) (PR #1)",
)


def release_changelog(entries: tuple[str, ...] | list[str] = RELEASED_ENTRIES, note: str = GENERATED_NOTE) -> str:
    """A `docs/changelog.md` in the shape `prepare` reads: no `## Unreleased`.

    The header and its note, the inline 0.1.0 release holding `entries`, then
    `Older releases`, which is what the release generates into and what the
    shape test asks of the real file (§FS-rhei-distribution.5.1).
    """
    body = "\n".join(entries)
    if body:
        body += "\n\n"
    return (
        f"# Changelog\n\n{note}\n\n"
        f"## 2. [0.1.0] - 2026-09-20\n\n{body}"
        "## 3. Older releases\n\n- [0.0.9](changelog/0.0.9.md) - 2026-09-01: Before it.\n"
    )


def pull(number: int, title: str | None = None, merged: bool = True) -> dict[str, object]:
    """One pull request as `commits/<sha>/pulls` returns it, trimmed to what the notes read."""
    return {
        "number": number,
        "title": title if title is not None else f"Change number {number}",
        "html_url": f"https://github.com/agent-grounds/rhei/pull/{number}",
        "state": "closed" if merged else "open",
        "merged_at": "2026-10-01T00:00:00Z" if merged else None,
    }


class TempRepo:
    """A git repository with a `docs/changelog.md`, thrown away after the test."""

    def __init__(self, path: Path, branch: str = "main") -> None:
        self.path = path
        (self.path / "docs").mkdir(parents=True, exist_ok=True)
        # Checked before `init`, because a leaked `GIT_DIR` would aim it at a real
        # repository, and the loss is a moved `HEAD` rather than a failed test.
        leaked = sorted(key for key in clean_env() if key.startswith("GIT_DIR"))
        if leaked:
            raise AssertionError(f"clean_env still carries {leaked}; git would write somewhere real")
        self.git("init", "-b", branch)

    def git(self, *args: str) -> str:
        result = subprocess.run(
            ["git", *args],
            cwd=str(self.path),
            capture_output=True,
            text=True,
            env=clean_env(),
        )
        if result.returncode != 0:
            raise AssertionError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
        return result.stdout

    def write_changelog(self, text: str) -> None:
        (self.path / "docs" / "changelog.md").write_text(text, encoding="utf-8")

    def read_changelog(self) -> str:
        return (self.path / "docs" / "changelog.md").read_text(encoding="utf-8")

    def write(self, relative: str, text: str = "changed\n") -> None:
        """Write a file at a `/`-separated path under the repository."""
        path = self.path.joinpath(*relative.split("/"))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self, message: str) -> str:
        self.git("add", "-A")
        self.git("commit", "--no-verify", "-m", message)
        return self.rev("HEAD")

    def rev(self, ref: str) -> str:
        return self.git("rev-parse", ref).strip()


_GH_STUB = r'''
import json, os, re, sys

argv = sys.argv[1:]
calls = os.environ.get("GH_STUB_CALLS")
if calls:
    with open(calls, "a", encoding="utf-8") as handle:
        handle.write(json.dumps(argv) + "\n")
pulls = {}
path = os.environ.get("GH_STUB_PULLS")
if path and os.path.exists(path):
    with open(path, encoding="utf-8") as handle:
        pulls = json.load(handle)

if argv[:1] == ["api"]:
    sha = None
    for arg in argv:
        found = re.search(r"/commits/([0-9a-fA-F]+)/pulls", arg)
        if found:
            sha = found.group(1)
    if sha is None:
        sys.stderr.write("gh stub: no commits/<sha>/pulls path in %r\n" % (argv,))
        sys.exit(1)
    failing = [token for token in os.environ.get("GH_STUB_FAIL", "").split(",") if token]
    if any(sha.startswith(token) or token.startswith(sha) for token in failing):
        sys.stderr.write("HTTP 502: Bad Gateway (https://api.github.com/repos/agent-grounds/rhei/commits/%s/pulls)\n" % sha)
        sys.exit(1)
    if "--jq" in argv:
        sys.stderr.write("gh stub: --jq is not understood; read the JSON\n")
        sys.exit(1)
    sys.stdout.write(json.dumps(pulls.get(sha, pulls.get(sha[:7], []))) + "\n")
    sys.exit(0)

sys.stderr.write("gh stub: unsupported invocation %r\n" % (argv,))
sys.exit(1)
'''


def _install(directory: Path, name: str, python_source: str) -> None:
    """Put a stub `name` in `directory` so a subprocess finds it by that bare name.

    Windows resolves a bare name through `PATHEXT`, so the stub is written under
    every extension a launcher might look for as well as under the bare name.
    Only a tool with no real binary behind it is installed this way: a launcher is
    a shell, and a shell re-parses what it forwards (§REQ-cross-platform.4).
    """
    impl = directory / f"_{name}_stub.py"
    impl.write_text(python_source, encoding="utf-8")
    target = f'"{sys.executable}" "{impl}"'

    posix = directory / name
    posix.write_text(f'#!/bin/sh\nexec {target} "$@"\n', encoding="utf-8")
    posix.chmod(posix.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    if WINDOWS:
        for extension in (".bat", ".cmd"):
            (directory / (name + extension)).write_text(f"@{target} %*\n", encoding="utf-8")


def make_bin(directory: Path, pulls: dict[str, list[object]] | None = None) -> Path:
    """A directory to put first on `PATH`, holding a stub `gh` and nothing else.

    `git` is deliberately not in it. The directory goes *in front of* the ambient
    `PATH`, so the real `git` is already reachable, and a forwarding wrapper would
    only put a shell between the script and the tool (§REQ-cross-platform.4).
    """
    directory.mkdir(parents=True, exist_ok=True)
    _install(directory, "gh", _GH_STUB)
    if pulls is not None:
        (directory / "pulls.json").write_text(json.dumps(pulls), encoding="utf-8")
    return directory


def path_with_no_gh() -> str:
    """The ambient `PATH` with every directory that offers a `gh` taken out.

    How "no `gh` installed" is reached without saying where the real one lives and
    without building a `PATH` of our own around a wrapper
    (§REQ-cross-platform.4). Where `gh` shares a directory with `git` - which is
    what a distribution's `/usr/bin` does - this takes `git` with it, and that is
    sound rather than tolerated: the generator reports an unreachable `gh` before it
    spawns anything, so the path under test never asks for `git`. Do not put a
    `git` back for it.
    """
    kept = [
        entry
        for entry in os.environ.get("PATH", "").split(os.pathsep)
        if entry and shutil.which("gh", path=entry) is None
    ]
    return os.pathsep.join(kept)


def pulls_file(directory: Path) -> Path:
    return directory / "pulls.json"


class ScriptTestCase(unittest.TestCase):
    """A temp directory, a temp repo, and a stub `gh`, for one test."""

    def setUp(self) -> None:
        holder = tempfile.mkdtemp(prefix="changelog-gate-")
        self.tmp = Path(holder)
        self.addCleanup(shutil.rmtree, holder, True)
        self.bin = make_bin(self.tmp / "bin")

    def repo(self, branch: str = "main") -> TempRepo:
        root = self.tmp / "repo"
        root.mkdir(parents=True, exist_ok=True)
        return TempRepo(root, branch=branch)

    def set_pulls(self, pulls: dict[str, list[object]]) -> Path:
        path = pulls_file(self.bin)
        path.write_text(json.dumps(pulls), encoding="utf-8")
        return path

    def run_script(
        self,
        script: Path,
        args: list[str],
        repo: TempRepo,
        path: str | None = None,
        **env: str | None,
    ) -> subprocess.CompletedProcess[str]:
        prefix = path if path is not None else f"{self.bin}{os.pathsep}{os.environ.get('PATH', '')}"
        overrides = dict(env)
        overrides["PATH"] = prefix
        overrides.setdefault("GH_STUB_PULLS", str(pulls_file(self.bin)))
        overrides.setdefault("GH_STUB_CALLS", str(self.calls_file()))
        return subprocess.run(
            [sys.executable, str(script), *args],
            cwd=str(repo.path),
            capture_output=True,
            text=True,
            env=clean_env(**overrides),
        )

    def calls_file(self) -> Path:
        return self.bin / "calls.jsonl"

    def gh_calls(self) -> list[list[str]]:
        """Every argv the stub `gh` was given, in order."""
        path = self.calls_file()
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]

    def output(self, result: subprocess.CompletedProcess[str]) -> str:
        """Both streams, for an assertion that should not pin which one is used."""
        return result.stdout + result.stderr

    def report(self, result: subprocess.CompletedProcess[str]) -> str:
        return f"exit={result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"


def job_lines(workflow: Path, job: str) -> list[str]:
    """The lines of one job of a workflow, its `name:` and `steps:` included.

    Read without PyYAML, because the tests may not assume a YAML library: the
    `lint` job installs `pre-commit` and nothing else (§AR-ci-release.1).
    """
    lines = workflow.read_text(encoding="utf-8").splitlines()
    start = next((index for index, line in enumerate(lines) if re.match(rf"^  {re.escape(job)}:\s*$", line)), None)
    if start is None:
        raise AssertionError(f"no job {job!r} in {workflow}")
    end = next((index for index in range(start + 1, len(lines)) if re.match(r"^  \S", lines[index])), len(lines))
    return lines[start:end]


def workflow_steps(workflow: Path, job: str) -> list[dict[str, str]]:
    """Each step of one job as its top-level keys, with `run:` as its script.

    A step's `name`, `id`, `if` and `uses` are read as written; a block `run: |`
    is the step's script, dedented, and `raw` is the step's every line. Nested
    keys such as `with:` are only in `raw`. (§AR-ci-release.3)
    """
    lines = job_lines(workflow, job)
    at = next((index for index, line in enumerate(lines) if re.match(r"^\s*steps:\s*$", line)), None)
    if at is None:
        raise AssertionError(f"job {job!r} in {workflow} has no steps")

    blocks: list[list[str]] = []
    indent = None
    for line in lines[at + 1 :]:
        item = re.match(r"^(?P<indent> *)- (?P<rest>.*)$", line)
        if item and (indent is None or len(item.group("indent")) == indent):
            indent = len(item.group("indent"))
            blocks.append([" " * (indent + 2) + item.group("rest")])
        elif blocks:
            blocks[-1].append(line)
    return [_step(block, (indent or 0) + 2) for block in blocks]


def _step(block: list[str], indent: int) -> dict[str, str]:
    step: dict[str, str] = {"raw": "\n".join(block)}
    key_re = re.compile(rf"^ {{{indent}}}(?P<key>[A-Za-z_-]+):\s*(?P<value>.*)$")
    index = 0
    while index < len(block):
        found = key_re.match(block[index])
        index += 1
        if found is None:
            continue
        key, value = found.group("key"), found.group("value").strip()
        if value in ("|", ">"):
            body: list[str] = []
            while index < len(block) and (not block[index].strip() or block[index].startswith(" " * (indent + 1))):
                body.append(block[index])
                index += 1
            value = textwrap.dedent("\n".join(body)).strip("\n")
        step[key] = value
    return step
