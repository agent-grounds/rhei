"""Inject faults into d's reads of c's spawn record, without replacing a fixture.

Prepended to STARTUP_SCHEDULER after the Rust regression sets `_fault_site`
('scheduler', 'poll' or 'snapshot') and `_fault_kind` ('transient', 'permanent'
or 'unrelated'). The site is the phase of d's interpreter: before, during or
after the wrapper's runpy of the agent. Controlled faults pin harness behavior,
not the unobserved Windows exception. §FS-rhei-agent-visit-pairing.5.1
"""
import errno as _errno
import json as _json
import os as _os
import pathlib as _pathlib
import runpy as _runpy
import sys as _sys
import time as _time

_real_read_text = _pathlib.Path.read_text
_real_run_path = _runpy.run_path
_c_record_name = 'task-plan.1-work-mock-mock-c-2.json'
_events = _pathlib.Path(__file__).parent / 'read-fault-events.jsonl'
_phase = 'scheduler'
_injected = False
_elapsed = 0.0
_reads = 0


def _record(event):
    with _events.open('a', encoding='utf-8') as handle:
        handle.write(_json.dumps(event) + '\n')


def _run_path(*args, **kwargs):
    global _phase
    _phase = 'poll'
    try:
        return _real_run_path(*args, **kwargs)
    finally:
        _phase = 'snapshot'


def _read(path, *args, **kwargs):
    global _injected, _reads
    if path.name != _c_record_name or _os.environ.get('RHEI_TARGET') != 'mock:mock:d':
        return _real_read_text(path, *args, **kwargs)
    if _phase == _fault_site:
        _reads += 1
        if _reads > 100000:
            raise RuntimeError('fault injector: unbounded reads of c record')
        if _fault_kind == 'permanent' or (_fault_kind == 'transient' and not _injected):
            _injected = True
            _record({'event': 'refused', 'site': _phase})
            error = PermissionError(_errno.EACCES, 'rhei-695 controlled refusal', str(path))
            error.winerror = 32
            raise error
        if _fault_kind == 'unrelated' and not _injected:
            _injected = True
            _record({'event': 'unrelated', 'site': _phase})
            raise OSError(_errno.EIO, 'rhei-695 unrelated I/O fault', str(path))
    return _real_read_text(path, *args, **kwargs)


def _escaped(kind, value, traceback):
    # SystemExit never reaches an excepthook, so this sees only an escape.
    _record({'event': 'escaped', 'error': kind.__name__ + ': ' + str(value)})
    _sys.__excepthook__(kind, value, traceback)


_pathlib.Path.read_text = _read
_runpy.run_path = _run_path
_sys.excepthook = _escaped

# Focused permanent/unrelated cases run the actual fixtures directly. Only
# their private interpreter gets a virtual clock, so they never wait in real
# time. §FS-rhei-agent-visit-pairing.5.1
if _fault_kind in ('permanent', 'unrelated'):
    import atexit as _atexit

    def _clock():
        return _elapsed

    def _sleep(seconds):
        global _elapsed
        _elapsed += seconds
        if _elapsed > 12:
            raise RuntimeError('fault injector: exceeded the fixture bound')

    def _finished():
        _record({'event': 'finished', 'elapsed': _elapsed, 'reads': _reads})

    _time.monotonic = _clock
    _time.sleep = _sleep
    _atexit.register(_finished)
