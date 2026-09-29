#!/usr/bin/env python3
"""Fixtures for the changelog gate and stamper tests. §FS-rhei-distribution.5

The tests drive the scripts as subprocesses over a throwaway git repository, so
what they pin is the contract a contributor and a workflow actually meet: the
command line, the exit code, and the message. Nothing here imports the scripts.

`gh` is answered by a stub on `PATH` rather than by the forge, which is the seam
the reproducer for agent-grounds/rhei#341 established and the only one the
stamper can be tested through at all.
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
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
GATE = REPO_ROOT / "scripts" / "check_changelog_pr_entry.py"
STAMPER = REPO_ROOT / "scripts" / "prepare_changelog_release.py"
PRE_COMMIT_CONFIG = REPO_ROOT / ".pre-commit-config.yaml"
CI_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "ci.yml"

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
    """The ambient environment with everything that could answer for us removed."""
    env = {key: value for key, value in os.environ.items() if key not in _STRIPPED}
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


def changelog(unreleased: list[str], released: str = "- Something released. (PR #2)") -> str:
    """A `docs/changelog.md` whose `## Unreleased` holds exactly these bullets."""
    body = "\n\n".join(unreleased)
    if body:
        body += "\n"
    return f"# Changelog\n\n## Unreleased\n\n{body}\n## 0.5.1 - 2026-09-20\n\n{released}\n"


class TempRepo:
    """A git repository with a `docs/changelog.md`, thrown away after the test."""

    def __init__(self, path: Path, branch: str = "main") -> None:
        self.path = path
        (self.path / "docs").mkdir(parents=True, exist_ok=True)
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

    def touch_source(self, name: str, text: str = "the change itself\n") -> None:
        (self.path / name).write_text(text, encoding="utf-8")

    def commit(self, message: str) -> str:
        self.git("add", "-A")
        self.git("commit", "--no-verify", "-m", message)
        return self.rev("HEAD")

    def rev(self, ref: str) -> str:
        return self.git("rev-parse", ref).strip()


_GH_STUB = r'''
import json, os, re, sys

argv = sys.argv[1:]
pulls = {}
path = os.environ.get("GH_STUB_PULLS")
if path and os.path.exists(path):
    with open(path, encoding="utf-8") as handle:
        pulls = json.load(handle)

if argv[:1] == ["pr"]:
    number = os.environ.get("GH_STUB_PR", "")
    if not number:
        sys.stderr.write('no pull requests found for branch "fix/the-thing"\n')
        sys.exit(1)
    sys.stdout.write(number + "\n")
    sys.exit(0)

if argv[:1] == ["api"]:
    sha = None
    for arg in argv:
        found = re.search(r"/commits/([0-9a-fA-F]+)/pulls", arg)
        if found:
            sha = found.group(1)
    if sha is None:
        sys.stderr.write("gh stub: no commits/<sha>/pulls path in %r\n" % (argv,))
        sys.exit(1)
    numbers = pulls.get(sha, pulls.get(sha[:7], []))
    if "--jq" in argv:
        sys.stdout.write("".join("%d\n" % n for n in numbers))
    else:
        sys.stdout.write(json.dumps([{"number": n} for n in numbers]) + "\n")
    sys.exit(0)

sys.stderr.write("gh stub: unsupported invocation %r\n" % (argv,))
sys.exit(1)
'''


def _install(directory: Path, name: str, python_source: str | None, forward: str | None) -> None:
    """Put `name` in `directory` so a subprocess finds it by that bare name.

    Windows resolves a bare name through `PATHEXT`, so the stub is written under
    every extension a launcher might look for as well as under the bare name.
    """
    if python_source is not None:
        impl = directory / f"_{name}_stub.py"
        impl.write_text(python_source, encoding="utf-8")
        target = f'"{sys.executable}" "{impl}"'
    else:
        assert forward is not None
        target = f'"{forward}"'

    posix = directory / name
    posix.write_text(f'#!/bin/sh\nexec {target} "$@"\n', encoding="utf-8")
    posix.chmod(posix.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    if WINDOWS:
        for extension in (".bat", ".cmd"):
            (directory / (name + extension)).write_text(f"@{target} %*\n", encoding="utf-8")


def make_bin(directory: Path, pulls: dict[str, list[int]] | None = None, gh: bool = True) -> Path:
    """A directory to put first on `PATH`: a stub `gh`, and a real `git` behind it.

    With `gh=False` the directory is the *whole* `PATH`, which is how "no `gh`
    installed" is reached without depending on where the real one lives.
    """
    directory.mkdir(parents=True, exist_ok=True)
    if gh:
        _install(directory, "gh", _GH_STUB, None)
    real_git = shutil.which("git")
    if real_git is None:
        raise unittest.SkipTest("git is not on PATH")
    _install(directory, "git", None, real_git)
    if pulls is not None:
        (directory / "pulls.json").write_text(json.dumps(pulls), encoding="utf-8")
    return directory


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

    def set_pulls(self, pulls: dict[str, list[int]]) -> Path:
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
        return subprocess.run(
            [sys.executable, str(script), *args],
            cwd=str(repo.path),
            capture_output=True,
            text=True,
            env=clean_env(**overrides),
        )

    def output(self, result: subprocess.CompletedProcess[str]) -> str:
        """Both streams, for an assertion that should not pin which one is used."""
        return result.stdout + result.stderr

    def report(self, result: subprocess.CompletedProcess[str]) -> str:
        return f"exit={result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"


def hook_stages(hook_id: str) -> list[str]:
    """The `stages:` of a hook in `.pre-commit-config.yaml`, read without PyYAML.

    The tests may not assume a YAML library: the `lint` job installs
    `pre-commit` and nothing else (§AR-ci-release.1).
    """
    lines = PRE_COMMIT_CONFIG.read_text(encoding="utf-8").splitlines()
    start = None
    for index, line in enumerate(lines):
        if re.match(rf"^\s*-\s+id:\s+{re.escape(hook_id)}\s*$", line):
            start = index
            break
    if start is None:
        raise AssertionError(f"no hook with id {hook_id!r} in {PRE_COMMIT_CONFIG}")
    for line in lines[start + 1 :]:
        if re.match(r"^\s*-\s+id:\s", line):
            break
        found = re.match(r"^\s*stages:\s*\[(?P<items>[^\]]*)\]\s*$", line)
        if found:
            return [item.strip() for item in found.group("items").split(",") if item.strip()]
    return []


def checkout_fetch_depth(job: str) -> str | None:
    """The `fetch-depth:` of a CI job's `actions/checkout`, read without PyYAML.

    Same reason as `hook_stages`: the `lint` job installs `pre-commit` and
    nothing else (§AR-ci-release.1).
    """
    lines = CI_WORKFLOW.read_text(encoding="utf-8").splitlines()
    start = None
    for index, line in enumerate(lines):
        if re.match(rf"^  {re.escape(job)}:\s*$", line):
            start = index
            break
    if start is None:
        raise AssertionError(f"no job {job!r} in {CI_WORKFLOW}")

    end = next((index for index in range(start + 1, len(lines)) if re.match(r"^  \S", lines[index])), len(lines))
    within = lines[start:end]
    for index, line in enumerate(within):
        if "actions/checkout" not in line:
            continue
        for following in within[index + 1 :]:
            if re.match(r"^\s*-\s", following):
                return None
            found = re.match(r"^\s*fetch-depth:\s*(?P<depth>\S+)\s*$", following)
            if found:
                return found.group("depth")
        return None
    raise AssertionError(f"job {job!r} has no actions/checkout step")
