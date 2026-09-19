"""Interpret physical stderr intervals for §FS-rhei-validate.5 and §FS-rhei-errors.1.2."""

import json
from pathlib import Path

from execution import write_json

MARKER = b"rhei migrate export-priors"


def help_lines(raw, target):
    """Count complete physical lines without joining renderer wraps (§FS-rhei-errors.1.2)."""
    expected = ("help: rhei migrate export-priors '" + target.replace("'", "'\"'\"'") + "'").encode()
    lines = [line for line in raw.splitlines(keepends=True) if MARKER in line]
    complete = [line for line in lines if b"help: " in line and line.endswith(b"\n")
                and line[line.index(b"help: "):].rstrip(b"\r\n") == expected]
    return {
        "expected": expected.decode(), "physical_lines": [line.decode(errors="replace") for line in lines],
        "complete_lines": [line.decode(errors="replace") for line in complete],
        "occurrences": raw.count(MARKER), "exact_assertion": len(complete) == 1 and raw.count(MARKER) == 1,
    }


def analyze_case(suite_output, mode):
    """Keep incomplete probes distinct from contrary per-pass help (§FS-rhei-validate.5)."""
    result = {"mode": mode, "passes": [], "problems": [], "contrary": [], "roots": []}
    problems, contrary = result["problems"], result["contrary"]
    directories = list((suite_output / "measurement").glob("case-*"))
    if len(directories) != 1:
        problems.append(f"expected exactly one measured E2E fixture; found {len(directories)}")
    else:
        output = directories[0]
        try:
            analyze_files(output, result)
        except (OSError, ValueError, KeyError, TypeError) as error:
            problems.append(f"incomplete measurement: {error}")
    if mode == "outside-root" and len(result["passes"]) > 1:
        contrary.append("extra outside-root passes")
    write_json(suite_output / "analysis.json", result)
    return result


def analyze_files(output, result):
    """Associate unchanged stderr bytes with process, pass and admission (§FS-rhei-validate.5)."""
    problems, contrary = result["problems"], result["contrary"]
    metadata = json.loads((output / "case.json").read_text())
    result["metadata"] = metadata
    if metadata.get("retention_errors") or not metadata["normal_stop_completed"] or not metadata["snapshot_available"]:
        problems.append("fixture retention or original poll/stop did not complete normally")
    raw = (output / "stderr.txt").read_bytes()
    snapshot = (output / "poll-snapshot.stderr").read_bytes()
    if not raw.startswith(snapshot):
        problems.append("poll snapshot is not a prefix of final raw stderr")
    result["snapshot"] = help_lines(snapshot, metadata["discovered_target"])
    result["final_capture"] = help_lines(raw, metadata["discovered_target"])
    result["snapshot_bytes"] = len(snapshot)
    events = []
    trace_files = list(output.glob("trace-*.jsonl"))
    if len(trace_files) != 1 or trace_files[0].name != f"trace-{metadata['child_pid']}.jsonl":
        problems.append("trace identity does not match the one watched child")
        return
    for index, line in enumerate(trace_files[0].read_bytes().splitlines(), 1):
        try:
            events.append(json.loads(line))
        except (ValueError, UnicodeDecodeError):
            problems.append(f"incomplete/invalid trace record {index}")
    if [item["seq"] for item in events] != list(range(1, len(events) + 1)):
        problems.append("noncontiguous trace sequence")
    if any(item["pid"] != metadata["child_pid"] for item in events):
        problems.append("trace contains a different process")
    roots = [item for item in events if item["type"] == "root"]
    result["roots"] = roots
    result["events"] = [item for item in events if item["type"] in ("event", "debounce_end", "debounce_wait")]
    result["observed_trace_overhead_ns_lower_bound"] = max(
        (int(item["overhead_before_ns"]) for item in events), default=0)
    result["overhead_limit"] = "Excludes final record I/O, JSON construction, first initialization and pre/post-fixture I/O."
    if not roots:
        problems.append("no successful root registrations")
    def inside(path):
        """Check aliases without rewriting recorded paths (§FS-rhei-validate.5)."""
        return any(Path(path).resolve().is_relative_to(Path(root["canonical_path"] or root["path"]).resolve())
                   for root in roots)
    if inside(metadata["output_canonical"]):
        problems.append("measurement output is inside a registered root")
    if inside(metadata["stderr_canonical"]) != (result["mode"] == "in-root"):
        problems.append("stderr location does not match requested mode")
    for name in ["plan.rhei.md", "states.yaml"]:
        if (output / (name + ".before")).read_bytes() != (output / (name + ".after")).read_bytes():
            contrary.append(f"authored {name} changed")
    if any(item["type"] == "watch_error" for item in events):
        problems.append("watcher error; inspect raw trace and stderr")
    by_seq = {item["seq"]: item for item in events}
    begins = [item for item in events if item["type"] == "pass_begin"]
    end_records = [item for item in events if item["type"] == "pass_end"]
    ends = {item["pass"]: item for item in end_records}
    if not begins or [item["pass"] for item in begins] != list(range(1, len(begins) + 1)):
        problems.append("missing/nonsequential validation passes")
    if len(ends) != len(end_records) or set(ends) - {item["pass"] for item in begins}:
        problems.append("duplicate or orphan pass-end record")
    offset = 0
    for begin in begins:
        end = ends.get(begin["pass"])
        if end is None:
            problems.append(f"pass {begin['pass']} incomplete at original child stop")
            continue
        start, stop = begin["stderr_bytes"], end["stderr_bytes"]
        valid = start == offset and start <= stop <= len(raw) and begin["seq"] < end["seq"]
        if not valid:
            problems.append(f"invalid/noncontiguous interval for pass {begin['pass']}")
        segment = raw[start:stop]
        (output / f"pass-{begin['pass']:03d}.stderr").write_bytes(segment)
        help_result = help_lines(segment, metadata["discovered_target"])
        admission = by_seq.get(begin["admitting_event"])
        if begin["pass"] > 1 and not (
            admission and admission["type"] == "event" and admission["stage"] == "receive"
            and admission["accepted"] and admission["paths"] and admission["kind"]
            and admission["seq"] < begin["seq"] and admission["seq"] > ends.get(begin["pass"] - 1, {"seq": begin["seq"]})["seq"]
        ):
            problems.append(f"pass {begin['pass']} has no distinct recorded admitting event")
        if valid and not help_result["exact_assertion"]:
            contrary.append(f"complete pass {begin['pass']} lacks exactly one complete command")
        result["passes"].append({
            "pass": begin["pass"], "start": start, "end": stop, "begin": begin, "finish": end,
            "help": help_result, "admitting_event": admission, "wholly_in_snapshot": stop <= len(snapshot),
        })
        offset = stop
    if offset != len(raw):
        problems.append("stderr bytes outside complete pass intervals")


def verdict(baselines, pair):
    """Only current failure and fully attributed snapshot passes are candidate support (§FS-rhei-validate.5)."""
    contrary = any(case["contrary"] for case in pair.values())
    incomplete = any(row["infrastructure_failure"] for row in baselines)
    incomplete |= any(case["problems"] for case in pair.values())
    if contrary:
        return 4, "contrary_evidence"
    if incomplete:
        return 2, "setup_timeout_or_measurement_failure"
    if len(baselines) != 2 or baselines[1]["outcome"] != "reported_assertion_failure":
        return 3, "bounded_non_reproduction_or_incomplete_support"
    if len(pair) == 2:
        inside, outside = pair["in-root"], pair["outside-root"]
        if (inside["suite"]["outcome"] == "reported_assertion_failure"
                and outside["suite"]["outcome"] == "pass"
                and len(inside["passes"]) > 1 and len(outside["passes"]) == 1
                and sum(item["wholly_in_snapshot"] for item in inside["passes"]) >= 2
                and outside["snapshot"]["exact_assertion"]):
            return 0, "candidate_location_dependent_extra_pass"
    return 3, "bounded_non_reproduction_or_incomplete_support"
