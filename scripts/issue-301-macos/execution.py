"""Bounded process/provenance records for temporary §AR-ci-release.1 evidence."""

from datetime import datetime, timezone
import hashlib
import json
import os
import re
import signal
import shutil
import subprocess
import time
import zipfile

SETUP_SECONDS = 1800
SETUP_SPENT = 0.0

TEST = "export_prior_migration_implementation_tests::omitted_validate_watch_renders_copyable_migration_help"
ENV_KEYS = [
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "CARGO_TARGET_DIR", "CARGO_TERM_COLOR",
    "CARGO_NET_OFFLINE", "RUSTUP_TOOLCHAIN", "RUST_TEST_THREADS", "RUST_TEST_NOCAPTURE",
    "RUSTFLAGS", "CARGO_BUILD_JOBS", "RHEI_KEEP_TEST_DIRS", "FORCE_COLOR", "CLICOLOR_FORCE",
    "ISSUE_301_CASE_OUTPUT", "ISSUE_301_TRACE_DIR", "ISSUE_301_STDERR",
]

AUTHORIZED_PREDECESSOR = {
    "run_id": "35428542945",
    "attempt": "1",
    "head_sha": "d868c7f97c319790cde3a1893d5a2df56171fc0a",
    "artifact": "issue-301-macos-35428542945-1",
    "historical_revision": "b0e3f86ad7ef37e75732beaa0adfa9f154b60a5b",
}


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


def required_binary(output, name, args, cwd, timeout=300):
    """Retain an API response byte-for-byte within the setup allowance (§AR-ci-release.1)."""
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
    return output / (name + ".stdout")


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


def _json(path):
    return json.loads(path.read_text())


def _extract_artifact(bundle, destination):
    """Preserve the predecessor artifact without trusting archive paths (§AR-ci-release.1)."""
    destination.mkdir()
    root = destination.resolve()
    with zipfile.ZipFile(bundle) as archive:
        for member in archive.infolist():
            target = (destination / member.filename).resolve()
            if not target.is_relative_to(root):
                raise RuntimeError("predecessor artifact contains an unsafe path")
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(member) as source, target.open("wb") as sink:
                shutil.copyfileobj(source, sink)


def _verify_predecessor(output, checkout, repo, run):
    """Prove the sole authorized continuation is the recorded zero-slot setup failure (§AR-ci-release.1)."""
    expected = AUTHORIZED_PREDECESSOR
    if (str(run["id"]), str(run.get("run_attempt")), run["head_sha"]) != (
        expected["run_id"], expected["attempt"], expected["head_sha"]
    ):
        raise RuntimeError("earlier protocol run is not the supervisor-authorized predecessor")
    run_record = json.loads(required(output, "authorized-predecessor-run", [
        "gh", "api", f"repos/{repo}/actions/runs/{expected['run_id']}",
    ], checkout))
    if (
        str(run_record["id"]), str(run_record["run_attempt"]), run_record["head_sha"],
        run_record["status"], run_record["conclusion"],
    ) != (
        expected["run_id"], expected["attempt"], expected["head_sha"], "completed", "failure",
    ):
        raise RuntimeError("authorized predecessor run identity or outcome changed")
    artifacts = json.loads(required(output, "authorized-predecessor-artifacts", [
        "gh", "api", f"repos/{repo}/actions/runs/{expected['run_id']}/artifacts?per_page=100",
    ], checkout))["artifacts"]
    matches = [artifact for artifact in artifacts if artifact["name"] == expected["artifact"]]
    if len(matches) != 1 or matches[0]["expired"]:
        raise RuntimeError("authorized predecessor artifact is missing, ambiguous, or expired")
    artifact = matches[0]
    if str(artifact["workflow_run"]["id"]) != expected["run_id"] or artifact["workflow_run"]["head_sha"] != expected["head_sha"]:
        raise RuntimeError("authorized predecessor artifact provenance does not match")
    bundle = required_binary(output, "authorized-predecessor-artifact", [
        "gh", "api", f"repos/{repo}/actions/artifacts/{artifact['id']}/zip",
    ], checkout)
    digest = "sha256:" + hashlib.sha256(bundle.read_bytes()).hexdigest()
    if artifact.get("digest") != digest:
        raise RuntimeError("authorized predecessor artifact digest does not match its API record")
    retained = output / "authorized-continuation" / "predecessor-artifact"
    retained.parent.mkdir()
    _extract_artifact(bundle, retained)

    ledger = _json(retained / "execution-budget.json")
    result = _json(retained / "result.json")
    environment = _json(retained / "environment.json")
    archive = _json(retained / "historical/archive.command.json")
    archive_stderr = (retained / "historical/archive.stderr").read_text().strip()
    setup_error = (retained / "setup-error.txt").read_text()
    identity = dict(line.split("=", 1) for line in (retained / "hosted-identity.txt").read_text().splitlines())
    historical = expected["historical_revision"]
    if ledger != {
        "protocol": "round-2-full-suite-v1", "introduction_commit": expected["head_sha"],
        "run_id": expected["run_id"], "attempt": expected["attempt"], "bound": 4,
        "seconds_per_suite": 2700, "slots": [], "earlier_protocol_runs": [],
    }:
        raise RuntimeError("authorized predecessor ledger does not prove zero spent slots")
    if result.get("exit_status") != 2 or result.get("outcome") != "setup_or_measurement_failure" or result.get("error") != "archive exited 128; inspect raw streams":
        raise RuntimeError("authorized predecessor result is not the recorded setup failure")
    if environment.get("diagnostic_revision") != expected["head_sha"] or environment.get("hosted", {}).get("GITHUB_RUN_ID") != expected["run_id"] or environment.get("hosted", {}).get("GITHUB_RUN_ATTEMPT") != expected["attempt"]:
        raise RuntimeError("authorized predecessor artifact has inconsistent hosted identity")
    if archive.get("argv", [])[:3] != ["git", "archive", historical] or archive.get("exit_status") != 128 or archive.get("state") != "finished" or archive.get("timed_out"):
        raise RuntimeError("authorized predecessor archive record is not the approved source failure")
    if archive_stderr != f"fatal: not a tree object: {historical}" or "RuntimeError: archive exited 128; inspect raw streams" not in setup_error:
        raise RuntimeError("authorized predecessor raw failure does not match the recorded missing object")
    if identity.get("run") != expected["run_id"] or identity.get("attempt") != expected["attempt"]:
        raise RuntimeError("authorized predecessor hosted identity file is inconsistent")
    proof = {
        "authorization": "supervisor checkpoint 11: one continuation after zero-slot setup failure",
        "predecessor": expected,
        "run_url": run_record["html_url"],
        "artifact_id": artifact["id"],
        "artifact_digest": digest,
        "verified": {
            "completed_failure": True, "exact_diagnostic_head": True, "protocol_introduction": True,
            "zero_spent_slots": True, "missing_historical_object_before_suites": True,
        },
    }
    write_json(output / "authorized-continuation" / "proof.json", proof)
    return proof


def claim_budget(checkout, output, protocol):
    """Refuse every restart except the one proved supervisor-authorized continuation (§AR-ci-release.1)."""
    if os.environ.get("GITHUB_RUN_ATTEMPT") != "1":
        raise RuntimeError("the approved experiment does not authorize workflow reruns")
    intro = required(output, "protocol-introduction", [
        "git", "log", "--reverse", "--format=%H", "-S", f'PROTOCOL = "{protocol}"',
        "--", "scripts/issue-301-macos/run.py",
    ], checkout).splitlines()[0]
    if intro != AUTHORIZED_PREDECESSOR["head_sha"]:
        raise RuntimeError("protocol introduction is not the approved original identity")
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
                previous.append(run)
    if len(previous) != 1:
        raise RuntimeError("expected exactly the one supervisor-authorized predecessor protocol run")
    proof = _verify_predecessor(output, checkout, repo, previous[0])
    prior = [{"id": row["id"], "attempt": row["run_attempt"], "sha": row["head_sha"], "url": row["html_url"]} for row in previous]
    ledger = {
        "protocol": protocol, "introduction_commit": intro,
        "run_id": os.environ["GITHUB_RUN_ID"], "attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "bound": 4, "seconds_per_suite": 2700, "slots": [], "earlier_protocol_runs": prior,
        "authorized_continuation": proof,
    }
    write_json(output / "execution-budget.json", ledger)
    return ledger
