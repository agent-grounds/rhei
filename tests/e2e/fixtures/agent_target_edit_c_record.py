"""Reads of the successful sibling c's current-visit spawn record.

Shared by d's agent and the startup scheduler, after shared_access_refused.py,
so that all three reads wait out a refusal by one rule within the fixture's
5-second monotonic bound. §FS-rhei-agent-visit-pairing.5.1
"""
import json
import pathlib
import sys
import time

_C_RECORD = pathlib.Path('runtime/spawns/task-plan.1-work-mock-mock-c-2.json')


def _c_record_timed_out(waiting, path, observed):
    """End at the fixture's prerequisite-timeout exit. §FS-rhei-agent-visit-pairing.5.1"""
    print(waiting + ' timed out: ' + str(path) + ': ' + observed, file=sys.stderr)
    sys.exit(97)


def _read_c_record(root, deadline, waiting):
    """`(record, None)`, or `(None, why)` when the record is absent or partial.

    A refusal is read again until `deadline`, and never stands for an absent
    record; other I/O errors propagate. §FS-rhei-agent-visit-pairing.5.1
    """
    path = pathlib.Path(root) / _C_RECORD
    while True:
        try:
            return json.loads(path.read_text(encoding='utf-8')), None
        except (FileNotFoundError, json.JSONDecodeError) as error:
            return None, error
        except OSError as error:
            if not _shared_access_refused(error):
                raise
            refusal = error
        if time.monotonic() >= deadline:
            _c_record_timed_out(waiting, path, 'last refusal: ' + str(refusal))
        time.sleep(0.005)


def _await_c_success(root, waiting):
    """Return once c has completed plan.1's work visit 2, within 5 seconds.

    The bound leaves room within the fixture's 10s agent timeout.
    §FS-rhei-agent-visit-pairing.5 §FS-rhei-agent-visit-pairing.5.1
    """
    deadline = time.monotonic() + 5
    while True:
        record, absent = _read_c_record(root, deadline, waiting)
        if absent is None and (
                record.get('task') == 'plan.1' and record.get('state') == 'work'
                and record.get('moves') == 2 and record.get('ending') == 'exited'
                and record.get('code') == 0):
            return
        observed = repr(record) if absent is None else str(absent)
        if time.monotonic() >= deadline:
            _c_record_timed_out(waiting, pathlib.Path(root) / _C_RECORD, observed)
        time.sleep(0.005)
