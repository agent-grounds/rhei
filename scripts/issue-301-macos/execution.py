"""Bounded process/provenance records for temporary §AR-ci-release.1 evidence."""

from datetime import datetime, timezone
import json
import os
import re
import signal
import subprocess
import time

SETUP_SECONDS = 1800
SETUP_SPENT = 0.0

TEST = "export_prior_migration_implementation_tests::omitted_validate_watch_renders_copyable_migration_help"
ENV_KEYS = [
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "CARGO_TARGET_DIR", "CARGO_TERM_COLOR",
    "CARGO_NET_OFFLINE", "RUSTUP_TOOLCHAIN", "RUST_TEST_THREADS", "RUST_TEST_NOCAPTURE",
    "RUSTFLAGS", "CARGO_BUILD_JOBS", "RHEI_KEEP_TEST_DIRS", "FORCE_COLOR", "CLICOLOR_FORCE",
    "ISSUE_301_CASE_OUTPUT", "ISSUE_301_TRACE_DIR", "ISSUE_301_STDERR",
]


def write_json(path, value):
    """Atomically retain progress even on interruption (§AR-ci-release.1)."""
    staging = path.with_suffix(path.suffix + ".partial")
    staging.write_text(json.dumps(value, indent=2) + "\n")
    staging.replace(path)


def command(output, name, args, cwd, timeout=120, env=None):
    """Preserve raw streams and kill the command group at its cap (§AR-ci-release.1)."""
    env = os.environ.copy() if env is None else env
    record = {
        "argv": [str(arg) for arg in args], "cwd": str(cwd),
        "started_utc": datetime.now(timezone.utc).isoformat(), "timeout_seconds": timeout,
        "environment": {key: env.get(key) for key in ENV_KEYS}, "state": "starting",
    }
    path = output / (name + ".command.json")
    write_json(path, record)
    with (output / (name + ".stdout")).open("wb") as stdout, (output / (name + ".stderr")).open("wb") as stderr:
        process = subprocess.Popen(record["argv"], cwd=cwd, env=env, stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        record.update(pid=process.pid, state="running")
        write_json(path, record)
        try:
            record["exit_status"] = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            record["exit_status"] = process.wait()
            record["timed_out"] = True
    record.update(finished_utc=datetime.now(timezone.utc).isoformat(), state="finished")
    write_json(path, record)
    return record


def required(output, name, args, cwd, timeout=120):
    """Do not interpret dependency/access failures as reproduction (§AR-ci-release.1)."""
    global SETUP_SPENT
    remaining = SETUP_SECONDS - SETUP_SPENT
    if remaining <= 0:
        raise RuntimeError("aggregate 30-minute setup budget exhausted")
    started = time.monotonic()
    try:
        record = command(output, name, args, cwd, min(timeout, remaining))
    finally:
        SETUP_SPENT += time.monotonic() - started
        write_json(output / "setup-budget.json", {"limit_seconds": SETUP_SECONDS, "spent_seconds": SETUP_SPENT})
    if record["exit_status"]:
        raise RuntimeError(f"{name} exited {record['exit_status']}; inspect raw streams")
    return (output / (name + ".stdout")).read_text().strip()


def record_toolchain(output, cwd):
    """Enforce the pinned compiler in each actual source directory (§AR-ci-release.1)."""
    rustc = required(output, "rustc", ["rustc", "-Vv"], cwd)
    cargo = required(output, "cargo", ["cargo", "-V"], cwd)
    required(output, "toolchain", ["rustup", "show", "active-toolchain"], cwd)
    host = re.search(r"(?m)^host: (\S+)$", rustc)
    if not host or not re.search(r"(?m)^release: 1\.82\.0$", rustc) or not cargo.startswith("cargo 1.82.0 "):
        raise RuntimeError("every source must use Rust/Cargo 1.82.0 with a recorded host")
    return host.group(1)


def named_outcome(output, record):
    """Gate on this test's failure block, never another test's assertion (§FS-rhei-validate.5)."""
    stdout = (output / "suite.stdout").read_text(errors="replace")
    stderr = (output / "suite.stderr").read_text(errors="replace")
    statuses = re.findall(r"(?m)^test " + re.escape(TEST) + r" \.\.\. (ok|FAILED|ignored)\s*$", stdout)
    blocks = re.findall(r"(?ms)^---- " + re.escape(TEST) + r" stdout ----\n(.*?)(?=^---- |^failures:|^test result:|\Z)", stdout)
    block = blocks[0] if len(blocks) == 1 else ""
    (output / "named-test-failure.txt").write_text(block)
    reported = (
        statuses == ["FAILED"] and len(blocks) == 1
        and f"thread '{TEST}' panicked at " in block
        and "the recovery command should be rendered exactly once" in block
        and re.search(r"(?m)^\s*left:\s*2\s*\n\s*right:\s*1\s*$", block) is not None
        and record["exit_status"] == 101 and not record.get("timed_out")
    )
    outcome = "reported_assertion_failure" if reported else (
        "pass" if statuses == ["ok"] else "other_failure_or_missing"
    )
    infrastructure = bool(record.get("timed_out") or record["exit_status"] not in (0, 101)
                          or "could not compile" in stderr or not statuses)
    return {
        "named_test": TEST, "outcome": outcome, "statuses": statuses,
        "exit_status": record["exit_status"], "infrastructure_failure": infrastructure,
        "timed_out": record.get("timed_out", False),
        "suite_failures": re.findall(r"(?m)^test (.*?) \.\.\. FAILED\s*$", stdout),
        "test_results": re.findall(r"(?m)^test result:.*$", stdout),
        "fixture_path_limit": "Unchanged baselines expose paths only in their raw failure output; no fixture probe is injected.",
    }


def claim_budget(checkout, output, protocol):
    """Refuse reruns and later heads of this approved experiment (§AR-ci-release.1)."""
    if os.environ.get("GITHUB_RUN_ATTEMPT") != "1":
        raise RuntimeError("the approved experiment does not authorize workflow reruns")
    intro = required(output, "protocol-introduction", [
        "git", "log", "--reverse", "--format=%H", "-S", f'PROTOCOL = "{protocol}"',
        "--", "scripts/issue-301-macos/run.py",
    ], checkout).splitlines()[0]
    repo = os.environ["GITHUB_REPOSITORY"]
    raw = required(output, "prior-runs", [
        "gh", "api", "--paginate", "--slurp",
        f"repos/{repo}/actions/workflows/issue-301-macos-evidence.yml/runs?branch=fix%2Fissue-301&event=pull_request&per_page=100",
    ], checkout)
    previous = []
    for page in json.loads(raw):
        for run in page["workflow_runs"]:
            if int(run["id"]) >= int(os.environ["GITHUB_RUN_ID"]):
                continue
            sha = run["head_sha"]
            # Look up commit ancestry through the API; old heads need not be local. §AR-ci-release.1
            comparison = json.loads(required(output, f"prior-{run['id']}", [
                "gh", "api", f"repos/{repo}/compare/{intro}...{sha}",
            ], checkout))
            if comparison["status"] in ("ahead", "identical"):
                previous.append({"id": run["id"], "sha": sha, "url": run["html_url"]})
    ledger = {
        "protocol": protocol, "introduction_commit": intro,
        "run_id": os.environ["GITHUB_RUN_ID"], "attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "bound": 4, "seconds_per_suite": 2700, "slots": [], "earlier_protocol_runs": previous,
    }
    write_json(output / "execution-budget.json", ledger)
    if previous:
        raise RuntimeError("earlier round-2 workflow exists; inspect its spent slots, do not restart the budget")
    return ledger
