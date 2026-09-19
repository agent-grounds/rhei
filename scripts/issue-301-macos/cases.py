"""Temporary paired observations of §FS-rhei-validate.5; never a shipping test."""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

MARKER = b"rhei migrate export-priors"
POLL_SECONDS = 0.025
DEADLINE_SECONDS = 10
EXTENDED_SECONDS = 1


def fixture(source):
    """Use the pinned E2E's authored inputs verbatim (§FS-rhei-migrate.5)."""
    text = (source / "tests/e2e/export_prior_migration_implementation_tests.rs").read_text()
    values = {}
    for name, filename in [("PLAN", "plan.rhei.md"), ("MACHINE", "states.yaml")]:
        match = re.search(r'const DIAGNOSTIC_' + name + r': &str = r#"(.*?)"#;', text, re.S)
        if match is None:
            raise ValueError(f"pinned diagnostic {name} fixture missing")
        values[filename] = match[1].encode()
    match = re.search(r'let directory = root.join\(\s*"([^"]+)"', text)
    if match is None:
        raise ValueError("pinned diagnostic directory missing")
    return match[1], values


def run_case(binary, source, case_root, output, mode):
    """Observe original polling separately from the extension (§FS-rhei-validate.5)."""
    output.mkdir(parents=True)
    suffix, authored = fixture(source)
    directory = case_root / suffix
    home_dir = directory / ".home"
    (home_dir / "state").mkdir(parents=True)
    for name, body in authored.items():
        (directory / name).write_bytes(body)
        (output / (name + ".before")).write_bytes(body)
    stderr_path = (directory if mode == "in-root" else case_root) / "watch-stderr.txt"
    trace_path = output / "trace.jsonl"
    trace_path.touch()
    stdout_path = output / "stdout.txt"
    env = os.environ.copy()
    for name in ["FORCE_COLOR", "CLICOLOR_FORCE"]:
        env.pop(name, None)
    overrides = {
        "HOME": str(home_dir), "XDG_STATE_HOME": str(home_dir / "state"),
        "ISSUE_301_TRACE": str(trace_path), "ISSUE_301_STDERR": str(stderr_path),
    }
    env.update(overrides)
    command = [str(binary), "validate", "--watch"]
    metadata = {
        "mode": mode, "command": command, "cwd": str(directory), "env": overrides,
        "stderr": str(stderr_path), "stdout": str(stdout_path), "trace": str(trace_path),
        "discovered_target": str((directory / "plan.rhei.md").resolve()),
        "poll_seconds": POLL_SECONDS, "deadline_seconds": DEADLINE_SECONDS,
        "extended_seconds": EXTENDED_SECONDS, "removed_env": ["FORCE_COLOR", "CLICOLOR_FORCE"],
    }
    (output / "command.json").write_text(json.dumps(metadata, indent=2) + "\n")
    snapshot = b""
    seen = False
    started = time.monotonic()
    try:
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(command, cwd=directory, env=env, stdout=stdout, stderr=stderr)
            try:
                # Preserve first read, 10 s deadline and 25 ms poll; keep the
                # diagnostic extension distinct. §FS-rhei-validate.5
                deadline = time.monotonic() + DEADLINE_SECONDS
                while True:
                    snapshot = stderr_path.read_bytes()
                    if MARKER in snapshot:
                        seen = True
                        break
                    if time.monotonic() >= deadline or process.poll() is not None:
                        break
                    time.sleep(POLL_SECONDS)
                metadata["snapshot_seconds"] = time.monotonic() - started
                if seen:
                    time.sleep(EXTENDED_SECONDS)
            finally:
                metadata["exited_before_kill"] = process.poll() is not None
                if process.poll() is None:
                    process.kill()
                metadata["exit_status"] = process.wait(timeout=5)
                metadata["observation_seconds"] = time.monotonic() - started
    finally:
        # Retain partial evidence even if collection is interrupted. §FS-rhei-validate.5
        (output / "poll-snapshot.stderr").write_bytes(snapshot)
        if stderr_path.exists():
            shutil.copyfile(stderr_path, output / "stderr.txt")
        metadata["seen_at_poll"] = seen
        metadata["authored"] = {}
        for name, body in authored.items():
            path = directory / name
            after = path.read_bytes() if path.exists() else b""
            (output / (name + ".after")).write_bytes(after)
            metadata["authored"][name] = {
                "exists_after": path.exists(), "unchanged": body == after,
                "before_sha256": hashlib.sha256(body).hexdigest(),
                "after_sha256": hashlib.sha256(after).hexdigest(),
            }
        (output / "case.json").write_text(json.dumps(metadata, indent=2) + "\n")
    return metadata
