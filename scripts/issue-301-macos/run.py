#!/usr/bin/env python3
"""Prepare hosted evidence for §FS-rhei-validate.5, without changing the contract."""

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import traceback

from cases import run_case
from evidence import analyze_case, verdict

BASELINE = "c841d36fe5f650ba3352c89724df59a2509e8f0a"
TEST = "export_prior_migration_implementation_tests::omitted_validate_watch_renders_copyable_migration_help"
ATTEMPTS = 12
HERE = Path(__file__).resolve().parent


def write_json(path, value):
    """Retain observations as evidence for §FS-rhei-validate.5."""
    path.write_text(json.dumps(value, indent=2) + "\n")


def command(output, name, args, cwd, timeout=900):
    """Keep exact commands, raw streams and statuses (§AR-ci-release.1)."""
    record = {
        "argv": [str(arg) for arg in args], "cwd": str(cwd),
        "started_utc": datetime.now(timezone.utc).isoformat(), "timeout_seconds": timeout,
        "environment": {key: os.environ.get(key) for key in [
            "PATH", "TMPDIR", "CARGO_TARGET_DIR", "CARGO_TERM_COLOR", "RUSTUP_TOOLCHAIN",
        ]},
    }
    write_json(output / (name + ".command.json"), record)
    with (output / (name + ".stdout")).open("wb") as stdout, (output / (name + ".stderr")).open("wb") as stderr:
        process = subprocess.Popen(record["argv"], cwd=cwd, stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            record["exit_status"] = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            record["exit_status"] = process.wait()
            record["timed_out"] = True
    record["finished_utc"] = datetime.now(timezone.utc).isoformat()
    write_json(output / (name + ".command.json"), record)
    return record["exit_status"]


def required(output, name, args, cwd, timeout=900):
    """Separate setup failures from test outcomes (§AR-ci-release.1)."""
    status = command(output, name, args, cwd, timeout)
    if status:
        raise RuntimeError(f"{name} exited {status}; inspect its raw streams")
    return (output / (name + ".stdout")).read_text().strip()


def archive(output, checkout, scratch, name):
    """Build only the pinned uncorrected source (§FS-rhei-validate.5)."""
    path = scratch / (name + ".tar")
    required(output, name + "-archive", ["git", "archive", BASELINE, "-o", path], checkout)
    source = scratch / name
    source.mkdir()
    with tarfile.open(path) as bundle:
        bundle.extractall(source, filter="data")
    return source


def record_toolchain(output, prefix, cwd):
    """Record and enforce the observation toolchain (§AR-ci-release.1)."""
    rustc = required(output, prefix + "-rustc", ["rustc", "-Vv"], cwd)
    cargo = required(output, prefix + "-cargo", ["cargo", "-V"], cwd)
    required(output, prefix + "-toolchain", ["rustup", "show", "active-toolchain"], cwd)
    host = re.search(r"(?m)^host: (\S+)$", rustc)
    if not host:
        raise RuntimeError(f"cannot determine {prefix} host target from rustc -Vv")
    if not re.search(r"(?m)^release: 1\.82\.0$", rustc) or not cargo.startswith("cargo 1.82.0 "):
        raise RuntimeError(f"{prefix} must use Rust and Cargo 1.82.0")
    return host.group(1)


def collect(checkout, output):
    """Run the bounded unchanged baseline and disposable traces (§FS-rhei-validate.5)."""
    scratch_parent = Path.home() / "ag/tmp"
    scratch_parent.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="issue-301-source-", dir=scratch_parent)).resolve()
    temp = scratch / "temp"
    temp.mkdir()
    os.environ["TMPDIR"] = str(temp)
    os.environ["CARGO_TERM_COLOR"] = "never"
    environment = {
        "baseline_revision": BASELINE, "diagnostic_revision": os.environ.get("ISSUE_301_HEAD"),
        "os": platform.platform(), "python": sys.version, "scratch": str(scratch),
        "baseline_attempt_bound": ATTEMPTS, "paired_attempt_bound": ATTEMPTS,
        "test": TEST, "run_url": f"{os.environ.get('GITHUB_SERVER_URL')}/{os.environ.get('GITHUB_REPOSITORY')}"
        f"/actions/runs/{os.environ.get('GITHUB_RUN_ID')}/attempts/{os.environ.get('GITHUB_RUN_ATTEMPT')}",
        "hosted": {key: os.environ.get(key) for key in [
            "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB", "GITHUB_WORKFLOW", "GITHUB_SHA",
            "RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion",
        ]},
    }
    write_json(output / "environment.json", environment)
    if platform.system() != "Darwin":
        raise RuntimeError("this evidence job requires macOS")
    head = required(output, "diagnostic-revision", ["git", "rev-parse", "HEAD"], checkout)
    if head != environment["diagnostic_revision"]:
        raise RuntimeError("checkout does not match the declared diagnostic revision")
    required(output, "baseline-revision", ["git", "cat-file", "-e", BASELINE + "^{commit}"], checkout)
    for name, args in [("os", ["sw_vers"]), ("kernel", ["uname", "-a"]),
                       ("rustc", ["rustc", "-Vv"]), ("cargo", ["cargo", "-V"]),
                       ("toolchain", ["rustup", "show", "active-toolchain"])]:
        required(output, name, args, checkout)
    source = archive(output, checkout, scratch, "baseline")
    instrumented = archive(output, checkout, scratch, "instrumented")
    host = record_toolchain(output, "baseline", source)
    environment["cargo_fetch_target"] = host
    environment["observation_toolchain"] = "1.82.0"
    write_json(output / "environment.json", environment)
    hashes = {}
    for path in ["Cargo.lock", "tests/e2e/export_prior_migration_implementation_tests.rs",
                 "crates/rhei-cli/src/lib.rs", "crates/rhei-cli/src/cli/states_render.rs"]:
        hashes[path] = hashlib.sha256((source / path).read_bytes()).hexdigest()
    write_json(output / "baseline-source-sha256.json", hashes)
    # Fetch only the observed host graph: an unrestricted fetch selects non-host
    # packages whose manifests Cargo 1.82 cannot parse. §AR-ci-release.1
    required(output, "fetch", ["cargo", "fetch", "--locked", "--target", host], source)
    target = scratch / "baseline-target"
    required(output, "baseline-cli-build", ["cargo", "build", "--offline", "--locked", "--package", "rhei-cli",
                                          "--bin", "rhei", "--target-dir", target], source)
    args = ["cargo", "test", "--offline", "--locked", "--package", "rhei-e2e-tests", "--test", "e2e",
            "--target-dir", target]
    required(output, "baseline-build", args + ["--no-run"], source)
    baselines = []
    for attempt in range(1, ATTEMPTS + 1):
        name = f"baseline-{attempt:02d}"
        status = command(output, name, args + [TEST, "--", "--exact", "--nocapture", "--test-threads=1"], source, 180)
        raw = (output / (name + ".stdout")).read_text() + (output / (name + ".stderr")).read_text()
        outcome = "other_failure"
        if status == 0 and "1 passed; 0 failed" in raw and TEST in raw:
            outcome = "pass"
        elif (status == 101 and "the recovery command should be rendered exactly once" in raw
              and re.search(r"left:\s*2\s+right:\s*1", raw) and "1 failed" in raw):
            outcome = "reported_assertion_failure"
        baselines.append({"attempt": attempt, "exit_status": status, "outcome": outcome})
        write_json(output / "baseline-outcomes.json", baselines)
    required(output, "diagnostic-init", ["git", "init", "-q"], instrumented)
    required(output, "apply-trace", ["git", "apply", HERE / "watch-trace.patch"], instrumented)
    shutil.copyfile(HERE / "trace.rs", instrumented / "crates/rhei-cli/src/cli/issue_301_trace.rs")
    for name in ["watch-trace.patch", "trace.rs"]:
        shutil.copyfile(HERE / name, output / name)
    instrumented_host = record_toolchain(output, "instrumented", instrumented)
    if instrumented_host != host:
        raise RuntimeError(f"instrumented host {instrumented_host} differs from baseline host {host}")
    trace_target = scratch / "trace-target"
    required(output, "trace-build", ["cargo", "build", "--offline", "--locked", "--package", "rhei-cli",
                                   "--bin", "rhei", "--target-dir", trace_target], instrumented)
    pairs = []
    for attempt in range(1, ATTEMPTS + 1):
        pair = {"attempt": attempt}
        order = ["in-root", "outside-root"] if attempt % 2 else ["outside-root", "in-root"]
        pair["order"] = order
        for slot, mode in zip(["a", "b"], order):
            case_output = output / f"pair-{attempt:02d}" / mode
            case_root = scratch / "cases" / f"pair-{attempt:02d}" / slot
            metadata = run_case(trace_target / "debug/rhei", source, case_root, case_output, mode)
            pair[mode] = analyze_case(case_output, metadata)
        pairs.append(pair)
        write_json(output / "paired-outcomes.json", pairs)
    code, outcome, supporting = verdict(baselines, pairs)
    # Actual numeric job IDs come from the attempt-specific Actions endpoint.
    # Missing identity is an access gap, not a negative reproduction. §AR-ci-release.1
    jobs = json.loads((output / "jobs.json").read_text())
    job = next((job for job in jobs["jobs"] if job["name"] == "issue-301 paired macOS trace"), None)
    if not job:
        raise RuntimeError("hosted numeric job identity unavailable; inspect jobs-api artifacts")
    result = {
        "exit_status": code, "outcome": outcome, "baseline_revision": BASELINE, "diagnostic_revision": head,
        "job_id": job["id"], "job_url": job["html_url"], "run_url": environment["run_url"],
        "baselines": baselines, "paired_attempts_with_location_difference": supporting,
        "shipping_verdict": "not evaluated; supervisor acceptance required",
    }
    write_json(output / "result.json", result)
    lines = [f"Diagnostic outcome: {outcome} (exit {code}).", f"Baseline: {BASELINE}",
             f"Diagnostic: {head}", f"Job: {job['html_url']}",
             "The original E2E uses its unchanged timing; paired probes add a separate 1 s observation window."]
    lines += [f"Baseline {row['attempt']:02d}: {row['outcome']} (exit {row['exit_status']})" for row in baselines]
    for pair in pairs:
        for mode in ["in-root", "outside-root"]:
            case = pair[mode]
            lines.append(f"Pair {pair['attempt']:02d} {mode}: {len(case['passes'])} complete passes; "
                         f"poll assertion={case['poll_snapshot']['exact_assertion']}; "
                         f"problems={case['setup_or_measurement_problems']}; contrary={case['contrary_evidence']}")
            for item in case["passes"][1:]:
                lines.append(f"  Pass {item['pass']} admitted by {json.dumps(item['admitting_event'])}")
    lines.append("Diagnostic completion is not a shipping verdict. Review raw per-pass stderr and event traces.")
    (output / "summary.txt").write_text("\n".join(lines) + "\n")
    return code


def main():
    """Leave a readable artifact on setup failure (§AR-ci-release.1)."""
    checkout, output = [Path(arg).resolve() for arg in sys.argv[1:]]
    output.mkdir(parents=True, exist_ok=True)
    try:
        return collect(checkout, output)
    except Exception as error:
        (output / "setup-error.txt").write_text(traceback.format_exc())
        write_json(output / "result.json", {"exit_status": 2, "outcome": "setup_or_measurement_failure", "error": str(error)})
        (output / "summary.txt").write_text(f"Setup/measurement failure (exit 2): {error}\nNo shipping verdict. Raw partial evidence retained.\n")
        return 2


if __name__ == "__main__":
    sys.exit(main())
