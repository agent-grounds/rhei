"""Pause the original shared-file open; never replace its write implementation.

Atomic publication no longer opens this destination in truncate mode, so this
instrumentation becomes inactive when the fixture is repaired. All waits fail
diagnostically. The C observer reads the real fd, not substituted loader bytes.
"""
_original_open = pathlib.Path.open
_gate = pathlib.Path(__file__).parent


def _wait(name):
    deadline = time.monotonic() + 20
    while not (_gate / name).exists():
        if time.monotonic() > deadline:
            raise RuntimeError('barrier timeout: ' + name)
        time.sleep(0.01)


def _controlled_open(self, mode='r', *args, **kwargs):
    if self.name == '01-shared.md' and mode == 'w':
        (_gate / 'truncate-ready').touch()
        _wait('permit-truncate')
        handle = _original_open(self, mode, *args, **kwargs)
        (_gate / 'truncated').touch()
        _wait('empty-captured')
        return handle
    return _original_open(self, mode, *args, **kwargs)


pathlib.Path.open = _controlled_open
