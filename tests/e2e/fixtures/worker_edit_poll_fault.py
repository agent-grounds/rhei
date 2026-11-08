"""Observe/inject reads in ONE_FILE_COVER, without replacing its publisher.

Prepended after `_fault_mode` is set by the Rust regression. Controlled faults
pin harness behavior, not the original Windows exception. §FS-rhei-run.3.7.3.1
"""
import atexit
import errno
import json

_real_read_text = pathlib.Path.read_text
_read_target = pathlib.Path(__file__).parent / 'ws/tasks/01-shared.md'
_events = pathlib.Path(__file__).parent / 'read-events.jsonl'
_editing = False
_injected = False
_elapsed = 0.0
_reads = 0


def _record(event):
    with _events.open('a', encoding='utf-8') as handle:
        handle.write(json.dumps(event) + '\n')


def _before_shared_edit():
    global _editing
    _editing = True
    _record({'event': 'edit'})
    if _fault_mode == 'diagnostics':
        raise RuntimeError('rhei-623 independent diagnostic fault')


def _wait_for_finalized_sibling(path):
    # This barrier belongs to the injector, not to the polling code under test.
    deadline = time.monotonic() + 15
    while True:
        try:
            image = _real_read_text(path, encoding='utf-8')
        except PermissionError:
            image = ''
        if '> **Result:** [ws.2](runtime/results/ws.2.md)' in image:
            return
        if time.monotonic() >= deadline:
            raise RuntimeError('fault injector: sibling never finalized')
        time.sleep(0.005)


def _read(path, *args, **kwargs):
    global _injected, _reads
    if path != _read_target or env('RHEI_TASK_ID_LOCAL') != '1':
        return _real_read_text(path, *args, **kwargs)
    if not _editing:
        _reads += 1
        if _reads > 10000:
            raise RuntimeError('fault injector: unbounded polling reads')
        if _fault_mode == 'permanent' or (_fault_mode == 'transient' and not _injected):
            if _fault_mode == 'transient':
                _wait_for_finalized_sibling(path)
            _injected = True
            _record({'event': 'poll-refused'})
            error = PermissionError(errno.EACCES, 'rhei-623 controlled polling refusal', str(path))
            error.winerror = 32
            raise error
        if _fault_mode == 'unrelated':
            _record({'event': 'poll-unrelated'})
            raise OSError(errno.EIO, 'rhei-623 unrelated I/O fault', str(path))
    image = _real_read_text(path, *args, **kwargs)
    _record({'event': 'edit-read' if _editing else 'poll-read'})
    return image


pathlib.Path.read_text = _read

# Focused refusal/error cases execute the actual fixture directly. Only their
# private interpreter gets a virtual clock, so they do not wait 15 real seconds
# or change the engine's/sibling's timing. §FS-rhei-run.3.7.3.1
if _fault_mode in ('permanent', 'unrelated'):
    def _clock():
        return _elapsed

    def _sleep(seconds):
        global _elapsed
        _elapsed += seconds
        if _elapsed > 16:
            raise RuntimeError('fault injector: exceeded polling deadline')

    def _finished():
        _record({'event': 'finished', 'elapsed': _elapsed, 'reads': _reads})

    time.monotonic = _clock
    time.sleep = _sleep
    atexit.register(_finished)
