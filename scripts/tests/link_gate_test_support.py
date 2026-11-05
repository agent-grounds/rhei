#!/usr/bin/env python3
"""Fixtures for the link gate's tests. §AR-ci-release.2.1

Ported from the reproducer agent-grounds/rhei#562 was filed with: the real hook
entries read from `.pre-commit-config.yaml`, a throwaway repository with a real
`pre-commit install`, and a loopback stand-in as lychee's proxy. The fixture's
links are `http://` because a proxy cannot give a status through a TLS tunnel,
and the stand-in refuses a tunnel, so nothing here reaches the network.

Every subprocess goes through `clean_env`: these tests run inside a `pre-commit`
hook, and a leaked `GIT_DIR` or `GIT_INDEX_FILE` aims the fixture's git at the
real repository. `clean_env` also drops `GITHUB_TOKEN`, with which lychee would
ask GitHub's API rather than the stand-in.
"""

from __future__ import annotations

import http.server
import re
import shutil
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path

from scripts.tests.changelog_test_support import (
    PRE_COMMIT_CONFIG,
    REPO_ROOT,
    SCRIPTS,
    TempRepo,
    clean_env,
    workflow_steps,
)

SWEEP_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "links-nightly.yml"
STAGED_SCRIPT = SCRIPTS / "check_staged_links.py"
WHOLE_TREE_HOOK = "lychee"
STAGED_HOOK = "lychee-staged"

STEVE = "http://github.com/steveyegge/beads"
GASTOWN = "http://github.com/gastownhall/beads"
PULL = "http://github.com/agent-grounds/rhei/pull/419"
OTHER = "http://docs.cline.bot/features"
EVERY_LINK = (STEVE, GASTOWN, PULL, OTHER)
ADDED = "http://github.com/agent-grounds/rhei/added-by-this-commit"
# On every page the stand-in serves, so a URL lychee was handed as a page to crawl asks for this too.
CRAWLED = "http://github.com/agent-grounds/rhei/crawled-from-the-page"

# The hook's four inputs exist, and only three files under `docs/` link out.
FIXTURE_TREE = {
    "README.md": "# Fixture\n\nStands in for rhei's tree.\n",
    "docs/unrelated.md": "# Unrelated\n\nThe file the commit changes. It holds no link at all.\n",
    "docs/changelog/0.6.0.md": f"# 0.6.0\n\n- [Refuse a flow-style collection]({PULL}) (PR #419)\n",
    "docs/functional-spec/comparison.md": (
        f"# Comparison\n\nCompared here:\n\n- [steveyegge/beads]({STEVE})\n- [not GitHub]({OTHER})\n\nThat is all.\n"
    ),
    "docs/functional-spec/rhei-vs-beads.md": f"# Rhei vs beads\n\nA comparison with [beads]({GASTOWN}).\n",
    "examples/README.md": "# Examples\n\nNo external links here.\n",
    "packages/README.md": "# Packages\n\nNo external links here.\n",
}

needs_lychee_and_pre_commit = unittest.skipUnless(
    shutil.which("lychee") and shutil.which("pre-commit"),
    "needs lychee and pre-commit, which of the CI jobs only `lint` installs",
)


def read_hooks() -> dict[str, dict[str, str]]:
    """Each hook's keys as written, by id. Read without PyYAML, as the workflow helpers are."""
    hooks: dict[str, dict[str, str]] = {}
    current: dict[str, str] | None = None
    for line in PRE_COMMIT_CONFIG.read_text(encoding="utf-8").splitlines():
        hook = re.match(r"^\s*- id:\s*(\S+)\s*$", line)
        if hook:
            current = hooks.setdefault(hook.group(1), {})
            continue
        key = re.match(r"^\s+([a-z_]+):\s*(.*?)\s*$", line)
        if key and current is not None:
            current[key.group(1)] = key.group(2)
    return hooks


def named_inputs(command: str) -> tuple[str, ...]:
    """The paths a link command names: what follows its last flag or script, up to any redirect."""
    words = re.split(r"[;|&<>]", command)[0].split()
    if words and words[-1].isdigit():
        words.pop()
    kept: list[str] = []
    for word in reversed(words):
        if word.startswith("-") or word == "lychee" or word.endswith(".py"):
            break
        kept.append(word)
    return tuple(reversed(kept))


def sweep_job() -> str:
    """The sweep's one job, whatever its key."""
    lines = SWEEP_WORKFLOW.read_text(encoding="utf-8").splitlines()
    start = lines.index("jobs:")
    jobs = [found.group(1) for line in lines[start + 1 :] if (found := re.match(r"^  ([\w-]+):\s*$", line))]
    if len(jobs) != 1:
        raise AssertionError(f"the sweep should have one job, found {jobs}")
    return jobs[0]


def sweep_steps() -> list[dict[str, str]]:
    return workflow_steps(SWEEP_WORKFLOW, sweep_job())


def sweep_step_running_lychee() -> dict[str, str]:
    """The one step that runs the tree's link check, which holds both of its passes."""
    steps = [step for step in sweep_steps() if "--include-fragments" in step.get("run", "")]
    if len(steps) != 1:
        raise AssertionError(f"one step should run the sweep's two passes, found {len(steps)}")
    return steps[0]


class StandIn:
    """lychee's HTTP proxy for one test: a listed URL gets its status, and any other gets 200."""

    def __init__(self) -> None:
        self.statuses: dict[str, int] = {}
        self.requests: list[str] = []
        record = self.requests
        statuses = self.statuses

        class Handler(http.server.BaseHTTPRequestHandler):
            def answer(self) -> None:
                record.append(self.path)
                body = f'<html><body><h1 id="there">x</h1><a href="{CRAWLED}">page</a></body></html>'.encode()
                self.send_response(statuses.get(self.path, 200))
                self.send_header("Content-Type", "text/html")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                if self.command != "HEAD":
                    self.wfile.write(body)

            do_GET = do_HEAD = answer

            def do_CONNECT(self) -> None:
                # A tunnel is what an `https://` link would ask for. Recorded, and it reaches nothing.
                record.append(self.path)
                self.send_error(502)

            def log_message(self, *args: object) -> None:
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def answer(self, status: int, *urls: str) -> None:
        for url in urls:
            self.statuses[url] = status

    def asked(self) -> list[str]:
        """Each URL asked for, once, however many requests it took."""
        return sorted(set(self.requests))

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()


class LinkGateCase(unittest.TestCase):
    """The checkout's own hook configuration and scripts, installed in a throwaway repository."""

    def setUp(self) -> None:
        holder = tempfile.mkdtemp(prefix="link-gate-")
        self.tmp = Path(holder)
        self.addCleanup(shutil.rmtree, holder, True)
        self.stand_in = StandIn()
        self.addCleanup(self.stand_in.stop)

        root = self.tmp / "repo"
        root.mkdir()
        self.repo = TempRepo(root)
        for relative, text in FIXTURE_TREE.items():
            self.repo.write(relative, text)
        shutil.copy2(PRE_COMMIT_CONFIG, root / PRE_COMMIT_CONFIG.name)
        for config in ("lychee.toml", ".lychee.toml", ".lycheeignore"):
            if (REPO_ROOT / config).exists():
                shutil.copy2(REPO_ROOT / config, root / config)
        shutil.copytree(SCRIPTS, root / "scripts", ignore=shutil.ignore_patterns("__pycache__"))
        self.repo.commit("baseline")
        installed = self.run_in_repo("pre-commit", "install")
        self.assertEqual(installed.returncode, 0, installed.stdout)

    def env(self) -> dict[str, str]:
        proxy = self.stand_in.url
        return clean_env(
            HTTP_PROXY=proxy,
            http_proxy=proxy,
            HTTPS_PROXY=proxy,
            https_proxy=proxy,
            ALL_PROXY=proxy,
            all_proxy=proxy,
            NO_PROXY=None,
            no_proxy=None,
            PRE_COMMIT_HOME=str(self.tmp / "pre-commit-home"),
            # Every hook that is not a link check, since the others need the whole workspace.
            SKIP=",".join(hook for hook in read_hooks() if "lychee" not in hook),
        )

    def run_in_repo(self, *argv: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [shutil.which(argv[0]) or argv[0], *argv[1:]],
            cwd=str(self.repo.path),
            env=self.env(),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            encoding="utf-8",
            errors="replace",
            timeout=120,
        )

    def append(self, relative: str, line: str) -> None:
        """Add a line to a file and stage it."""
        path = self.repo.path.joinpath(*relative.split("/"))
        existing = path.read_text(encoding="utf-8") if path.exists() else ""
        self.repo.write(relative, f"{existing}{line}\n")
        self.repo.git("add", "-A")

    def commit(self, *flags: str) -> subprocess.CompletedProcess[str]:
        """A real `git commit` through the installed hook, with `self.before` the head it started on."""
        self.before = self.repo.rev("HEAD")
        return self.run_in_repo("git", "commit", "-m", "the change", *flags)

    def run_all_files(self) -> subprocess.CompletedProcess[str]:
        """The command the required job and `main`'s push run give the hooks (§AR-ci-release.1)."""
        return self.run_in_repo("pre-commit", "run", "--all-files", "--show-diff-on-failure")

    def assert_landed(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertEqual(result.returncode, 0, f"the commit was refused:\n{result.stdout}")
        self.assertNotEqual(self.repo.rev("HEAD"), self.before, "the head did not move")

    def assert_refused(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertNotEqual(result.returncode, 0, f"the commit landed:\n{result.stdout}")
        self.assertEqual(self.repo.rev("HEAD"), self.before, "the head moved")

    def assert_link_hooks_ran(self, result: subprocess.CompletedProcess[str]) -> None:
        """Both hooks reported on this run, so a commit that landed was not simply unchecked."""
        hooks = read_hooks()
        for hook in (WHOLE_TREE_HOOK, STAGED_HOOK):
            self.assertIn(hook, hooks, f"no hook {hook} in the configuration")
            status = rf"(?m)^{re.escape(hooks[hook]['name'])}\.+(\(no files to check\))?(Passed|Skipped)$"
            self.assertRegex(result.stdout, status)
