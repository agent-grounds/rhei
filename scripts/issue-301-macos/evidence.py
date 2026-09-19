"""Interpret physical stderr intervals for §FS-rhei-validate.5 and §FS-rhei-errors.1.2."""

import json
from pathlib import Path

MARKER = "rhei migrate export-priors"
HELP = "help: " + MARKER + " "


def help_lines(raw, target):
    """Mirror the exact physical-line/count assertion (§FS-rhei-errors.1.2)."""
    expected = HELP + "'" + target.replace("'", "'\"'\"'") + "'"
    text = raw.decode("utf-8", errors="replace")
    matching = [line for line in text.splitlines() if MARKER in line]
    complete = [line for line in matching if HELP in line and line[line.index(HELP):] == expected]
    return {
        "expected": expected, "physical_lines": matching, "complete_lines": complete,
        "occurrences": text.count(MARKER),
        "exact_assertion": len(complete) >= 1 and text.count(MARKER) == 1,
    }


def analyze_case(output, metadata):
    """Associate bytes with pass IDs and the admitting event (§FS-rhei-validate.5)."""
    raw = (output / "stderr.txt").read_bytes()
    events = [json.loads(line) for line in (output / "trace.jsonl").read_text().splitlines()]
    roots = [item for item in events if item["type"] == "root"]
    by_seq = {item["seq"]: item for item in events}
    begins = [item for item in events if item["type"] == "pass_begin"]
    ends = {item["pass"]: item for item in events if item["type"] == "pass_end"}
    problems = []
    contrary = []
    if not roots or not begins:
        problems.append("missing registered roots or validation passes")
    for key in ["stdout", "trace"] + (["stderr"] if metadata["mode"] == "outside-root" else []):
        if any(Path(metadata[key]).resolve().is_relative_to(Path(root["path"]).resolve()) for root in roots):
            problems.append(f"{key} is inside a registered root")
    if metadata["mode"] == "in-root" and not any(
        Path(metadata["stderr"]).resolve().is_relative_to(Path(root["path"]).resolve()) for root in roots
    ):
        problems.append("in-root capture is outside every registered root")
    if not metadata["seen_at_poll"] or metadata["exited_before_kill"]:
        problems.append("missing first help or watcher exited before diagnostic stop")
    if not all(item["unchanged"] for item in metadata["authored"].values()):
        contrary.append("authored inputs changed")
    if any(item["type"] == "watch_error" for item in events):
        problems.append("watcher reported an error; inspect trace and stderr")
    passes = []
    offset = 0
    for begin in begins:
        end = ends.get(begin["pass"])
        if end is None:
            problems.append(f"pass {begin['pass']} incomplete at diagnostic stop")
            continue
        start, stop = begin["stderr_bytes"], end["stderr_bytes"]
        if start != offset or not start <= stop <= len(raw) or begin["seq"] >= end["seq"]:
            problems.append(f"invalid or noncontiguous stderr interval for pass {begin['pass']}")
        segment = raw[start:stop]
        (output / f"pass-{begin['pass']:03d}.stderr").write_bytes(segment)
        result = help_lines(segment, metadata["discovered_target"])
        admission = by_seq.get(begin["admitting_event"])
        if begin["pass"] != 1 and not (
            admission and admission["type"] == "event" and admission["stage"] == "receive"
            and admission["accepted"] and admission["seq"] < begin["seq"]
        ):
            problems.append(f"pass {begin['pass']} has no recorded admitting event")
        if not result["exact_assertion"]:
            contrary.append(f"pass {begin['pass']} does not contain exactly one complete command")
        passes.append({
            "pass": begin["pass"], "start": start, "end": stop,
            "begin": begin, "finish": end, "help": result, "admitting_event": admission,
        })
        offset = stop
    if offset != len(raw):
        problems.append("stderr contains bytes outside complete pass intervals")
    result = {
        "mode": metadata["mode"], "roots": roots, "passes": passes,
        "poll_snapshot": help_lines((output / "poll-snapshot.stderr").read_bytes(), metadata["discovered_target"]),
        "extended_capture": help_lines(raw, metadata["discovered_target"]),
        "setup_or_measurement_problems": problems, "contrary_evidence": contrary,
    }
    (output / "analysis.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def verdict(baselines, pairs):
    """Keep reproduction separate from a shipping verdict (§FS-rhei-validate.5)."""
    setup = any(item["outcome"] == "other_failure" for item in baselines)
    contrary = False
    supporting = []
    for pair in pairs:
        inside, outside = pair["in-root"], pair["outside-root"]
        setup |= bool(inside["setup_or_measurement_problems"] or outside["setup_or_measurement_problems"])
        contrary |= bool(inside["contrary_evidence"] or outside["contrary_evidence"])
        contrary |= len(outside["passes"]) > 1
        if len(inside["passes"]) > 1 and len(outside["passes"]) == 1:
            supporting.append(pair["attempt"])
    baseline_failure = any(item["outcome"] == "reported_assertion_failure" for item in baselines)
    if setup:
        return 2, "setup_or_measurement_failure", supporting
    if contrary:
        return 4, "contrary_evidence", supporting
    if baseline_failure and supporting:
        return 0, "location_dependent_extra_pass_observed", supporting
    return 3, "non_reproduction_or_incomplete_paired_support", supporting
