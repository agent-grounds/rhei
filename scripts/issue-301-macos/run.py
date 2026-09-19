#!/usr/bin/env python3
"""Prepare bounded full-suite evidence for §FS-rhei-validate.5; never claim a fix."""

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import tempfile
import traceback

from cases import HERE, SOURCE_FILES, acquire, outside_capture, prepare
from evidence import analyze_case, verdict
from execution import ENV_KEYS, TEST, claim_budget, command, named_outcome, record_toolchain, required, write_json

PROTOCOL = "round-2-full-suite-v1"
HISTORICAL = "b0e3f86ad7ef37e75732beaa0adfa9f154b60a5b"
BASELINE = "c841d36fe5f650ba3352c89724df59a2509e8f0a"
HISTORICAL_TREE = "cc582280de5b0521df4570d2a12537c61dbb05a1"
BASELINE_TREE = "fd8c4cf1eccc54685dff8fd80ff3e75e9957d963"


def suite(source, target, output, ledger, artifact, name, revision, measured=False):
    """Spend a slot before launch; a crash never refunds it (§AR-ci-release.1)."""
    if len(ledger["slots"]) >= 4:
        raise RuntimeError("full-suite execution budget exhausted")
    hashes = {path: hashlib.sha256((source / path).read_bytes()).hexdigest() for path in SOURCE_FILES}
    write_json(output / "prepared-source-sha256.json", hashes)
    slot = {"slot": len(ledger["slots"]) + 1, "name": name, "revision": revision,
            "state": "spent_before_launch", "output": str(output)}
    ledger["slots"].append(slot)
    write_json(artifact / "execution-budget.json", ledger)
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(target), CARGO_NET_OFFLINE="true", CARGO_TERM_COLOR="never")
    if measured:
        measurement = output / "measurement"
        measurement.mkdir()
        env["ISSUE_301_CASE_OUTPUT"] = str(measurement)
    args = ["cargo", "test", "--workspace", "--all-targets", "--locked", "--no-fail-fast",
            "--offline", "--target-dir", target]
    record = command(output, "suite", args, source, timeout=2700, env=env)
    outcome = named_outcome(output, record)
    after = {path: hashlib.sha256((source / path).read_bytes()).hexdigest() for path in SOURCE_FILES}
    write_json(output / "after-source-sha256.json", after)
    if hashes != after:
        outcome["infrastructure_failure"] = True
        outcome["source_mutated"] = True
    outcome.update(name=name, revision=revision, slot=slot["slot"])
    slot.update(state="finished", outcome=outcome)
    write_json(output / "outcome.json", outcome)
    write_json(artifact / "execution-budget.json", ledger)
    return outcome


def collect(checkout, output):
    """Run historical/current once, opening the pair only on current 2 != 1 (§FS-rhei-validate.5)."""
    parent = Path.home() / "ag/tmp"
    parent.mkdir(parents=True, exist_ok=True)
    if not output.is_relative_to(parent.resolve()):
        raise RuntimeError("diagnostic artifacts must be under ~/ag/tmp")
    environment = {
        "protocol": PROTOCOL,
        "historical_revision": HISTORICAL,
        "historical_tree": HISTORICAL_TREE,
        "baseline_revision": BASELINE,
        "baseline_tree": BASELINE_TREE,
        "diagnostic_revision": os.environ.get("ISSUE_301_HEAD"), "os": platform.platform(),
        "python": sys.version, "native_temp_dir": tempfile.gettempdir(),
        "inherited_environment": {key: os.environ.get(key) for key in ENV_KEYS}, "test": TEST,
        "hosted": {key: os.environ.get(key) for key in [
            "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB", "GITHUB_WORKFLOW", "GITHUB_SHA",
            "RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion",
        ]},
    }
    write_json(output / "environment.json", environment)
    if platform.system() != "Darwin":
        raise RuntimeError("hosted evidence requires macOS")
    # Never relocate TMPDIR. Unexpected collection/test overrides make this setup inconclusive. §AR-ci-release.1
    for key in ["RUST_TEST_THREADS", "RUST_TEST_NOCAPTURE", "RHEI_KEEP_TEST_DIRS",
                "ISSUE_301_CASE_OUTPUT", "ISSUE_301_TRACE_DIR", "ISSUE_301_STDERR"]:
        if key in os.environ:
            raise RuntimeError(f"unexpected inherited override: {key}")
    if Path(tempfile.gettempdir()).resolve().is_relative_to(parent.resolve()):
        raise RuntimeError("native test temp directory was redirected into scratch")
    head = required(output, "diagnostic-revision", ["git", "rev-parse", "HEAD"], checkout)
    if head != environment["diagnostic_revision"]:
        raise RuntimeError("checkout does not match declared diagnostic revision")
    jobs = json.loads((output / "jobs.json").read_text())
    job = next((row for row in jobs["jobs"] if row["name"] == "issue-301 full-suite macOS evidence"), None)
    if not job:
        raise RuntimeError("hosted numeric job identity unavailable")
    environment.update(job_id=job["id"], job_url=job["html_url"])
    write_json(output / "environment.json", environment)
    if (output / "execution-budget.json").exists():
        raise RuntimeError("this artifact already owns an execution ledger; refusing restart")
    ledger = claim_budget(checkout, output, PROTOCOL)
    scratch = Path(tempfile.mkdtemp(prefix="issue-301-source-", dir=parent)).resolve()
    environment["scratch"] = str(scratch)
    write_json(output / "environment.json", environment)
    for name, args in [("os", ["sw_vers"]), ("kernel", ["uname", "-a"]), ("space", ["df", "-h", scratch])]:
        required(output, name, args, checkout)
    acquisition = output / "source-acquisition"
    acquisition.mkdir()
    acquire(checkout, acquisition / "historical", HISTORICAL, HISTORICAL_TREE)
    acquire(checkout, acquisition / "current", BASELINE, BASELINE_TREE)
    collector = output / "collector"
    collector.mkdir()
    shutil.copyfile(checkout / ".github/workflows/issue-301-macos-evidence.yml", collector / "workflow.yml")
    for path in HERE.iterdir():
        if path.is_file():
            shutil.copyfile(path, collector / path.name)
    # A shared target reuses dependency builds, while Cargo fingerprints each source copy. §AR-ci-release.1
    target = scratch / "target"
    baselines, pair = [], {}
    host = None
    for name, revision, tree in [
        ("historical", HISTORICAL, HISTORICAL_TREE), ("current", BASELINE, BASELINE_TREE),
    ]:
        case_output = output / name
        source, host = prepare(checkout, scratch, case_output, name, revision, tree, host)
        row = suite(source, target, case_output, ledger, output, name, revision)
        baselines.append(row)
        write_json(output / "baseline-outcomes.json", baselines)
        if row["infrastructure_failure"]:
            return finish(output, environment, ledger, baselines, pair)
    eligible = baselines[1]["outcome"] == "reported_assertion_failure"
    write_json(output / "pair-condition.json", {
        "eligible": eligible, "current_named_outcome": baselines[1],
        "reason": "Only the current named test's exact recovery-command 2 != 1 assertion opens slots 3 and 4.",
    })
    if eligible:
        case_output = output / "in-root"
        source, host = prepare(
            checkout, scratch, case_output, "instrumented", BASELINE, BASELINE_TREE, host, True,
        )
        for mode in ["in-root", "outside-root"]:
            case_output = output / mode
            if mode == "outside-root":
                case_output.mkdir()
                outside_capture(source, case_output)
                if record_toolchain(case_output, source) != host:
                    raise RuntimeError("outside-root source toolchain changed")
            row = suite(source, target, case_output, ledger, output, mode, BASELINE, True)
            case = analyze_case(case_output, mode)
            case["suite"] = row
            if row["infrastructure_failure"]:
                case["problems"].append("suite build/access/timeout failure")
            pair[mode] = case
            write_json(output / "paired-outcomes.json", pair)
            if row["infrastructure_failure"] or case["problems"] or case["contrary"]:
                break
    return finish(output, environment, ledger, baselines, pair)


def finish(output, environment, ledger, baselines, pair):
    """Keep all suite statuses independent of this diagnostic verdict (§AR-ci-release.1)."""
    code, outcome = verdict(baselines, pair)
    result = {
        "exit_status": code, "outcome": outcome, "environment": environment,
        "spent_slots": len(ledger["slots"]), "slot_bound": 4, "baselines": baselines,
        "pair": pair, "shipping_verdict": "not evaluated; supervisor and revised contract required",
    }
    write_json(output / "result.json", result)
    lines = [f"Diagnostic outcome: {outcome} (exit {code}).",
             f"Job: {environment['job_url']}", f"Protocol: {PROTOCOL}; spent slots: {len(ledger['slots'])}/4.",
             "No retries, timing extension, product fix or shipping verdict."]
    for slot in ledger["slots"]:
        row = slot["outcome"]
        lines.append(f"Slot {slot['slot']} {slot['name']}: named test={row['outcome']}; suite exit={row['exit_status']}.")
    for mode, case in pair.items():
        lines.append(f"{mode}: complete passes={len(case['passes'])}; problems={case['problems']}; contrary={case['contrary']}.")
    lines.append("Inspect raw suite streams, snapshot/pass bytes, admissions, overhead and incomplete intervals.")
    (output / "summary.txt").write_text("\n".join(lines) + "\n")
    return code


def main():
    """Preserve partial observations on setup failure without retrying (§AR-ci-release.1)."""
    checkout, output = [Path(arg).resolve() for arg in sys.argv[1:]]
    output.mkdir(parents=True, exist_ok=True)
    try:
        return collect(checkout, output)
    except Exception as error:
        (output / "setup-error.txt").write_text(traceback.format_exc())
        write_json(output / "result.json", {
            "exit_status": 2, "outcome": "setup_or_measurement_failure", "error": str(error),
            "partial_evidence": "Read execution-budget.json, individual outcome.json and raw streams; nothing was retried.",
        })
        (output / "summary.txt").write_text(f"Inconclusive setup/measurement failure (exit 2): {error}\nRaw partial evidence retained; no shipping verdict.\n")
        return 2


if __name__ == "__main__":
    sys.exit(main())
