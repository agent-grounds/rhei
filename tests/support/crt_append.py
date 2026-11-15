# §REQ-cross-platform.4.2
"""Windows' append, on any host: seek to the end, then write (rhei#723).

POSIX appends in one atomic step: a descriptor opened with `O_APPEND` moves to
the end and writes in the same call, so two processes appending to one file at
once never share an offset. The Windows C runtime takes two steps instead - it
seeks to the end, then writes - so two writers that seek together write at one
offset, and the line that landed first is overwritten. That is why a race in a
fixture's append showed only on `windows-latest`.

`install` gives every append-mode open in this process those two steps:
`builtins.open`, `io.open` and `pathlib.Path.open` with an `a` mode, and
`os.open` with `O_APPEND` (with `os.write` on what it returns, and `os.fdopen`
over it). Each descriptor is opened without `O_APPEND`, and each write seeks to
the end, waits at a rendezvous until every other writer has sought too (or the
timeout passes), and writes where it sought. The rendezvous forces the instant
two writers reach together only sometimes on a real runner, so the loss is
certain rather than likely.

File locks are left alone. A writer that appends under an exclusive lock makes
the next one wait before it can seek: the holder times out at the rendezvous,
writes and lets go, and the next writer seeks past what it wrote. On Windows the
emulation is what the platform does already, made to happen every time.
"""
import builtins
import io
import os
import pathlib
import time

_real_open = io.open
_real_os_open = os.open
_real_os_write = os.write
_real_os_close = os.close

# Descriptors `os.open` opened for append, with `O_APPEND` taken off.
_appending = set()
_meet = None
_name = None
_writers = 0
_timeout = 0.0


def arrive(directory, name, writers, timeout):
    """Mark `name` arrived in `directory` and wait until `writers` have, or
    `timeout` seconds pass. Says whether they all came."""
    _real_os_close(_real_os_open(os.path.join(directory, name), os.O_CREAT | os.O_WRONLY, 0o666))
    until = time.monotonic() + timeout
    while len(os.listdir(directory)) < writers:
        if time.monotonic() >= until:
            return False
        time.sleep(0.005)
    return True


def _seek_then_write(fd, write, data):
    os.lseek(fd, 0, os.SEEK_END)            # the C runtime seeks to the end ...
    arrive(_meet, _name, _writers, _timeout)  # ... every other writer seeks too ...
    return write(data)                       # ... and each writes where it sought


def _without_append(path, flags, mode=0o666):
    return _real_os_open(path, flags & ~os.O_APPEND, mode)


class _SeekThenWrite(io.FileIO):
    """A file opened in an append mode whose every write takes two steps."""

    def write(self, data):
        return _seek_then_write(self.fileno(), super().write, data)


def _open(file, mode='r', buffering=-1, encoding=None, errors=None, newline=None,
          closefd=True, opener=None):
    if 'a' not in mode or (isinstance(file, int) and file not in _appending):
        return _real_open(file, mode, buffering, encoding, errors, newline, closefd, opener)
    raw_mode = 'a+' if '+' in mode else 'a'
    if isinstance(file, int):
        raw = _SeekThenWrite(file, raw_mode, closefd=closefd)
    else:
        def strip(path, flags):
            return (opener or _without_append)(path, flags & ~os.O_APPEND)
        raw = _SeekThenWrite(file, raw_mode, opener=strip)
    if buffering == 0:
        return raw
    buffered = (io.BufferedRandom if '+' in mode else io.BufferedWriter)(raw)
    if 'b' in mode:
        return buffered
    return io.TextIOWrapper(buffered, encoding, errors, newline, line_buffering=buffering == 1)


def _path_open(self, mode='r', buffering=-1, encoding=None, errors=None, newline=None):
    return _open(self, mode, buffering, encoding, errors, newline)


def _os_open(path, flags, mode=0o777, *, dir_fd=None):
    if not flags & os.O_APPEND:
        return _real_os_open(path, flags, mode, dir_fd=dir_fd)
    fd = _real_os_open(path, flags & ~os.O_APPEND, mode, dir_fd=dir_fd)
    _appending.add(fd)
    return fd


def _os_write(fd, data):
    if fd in _appending:
        return _seek_then_write(fd, lambda chunk: _real_os_write(fd, chunk), data)
    return _real_os_write(fd, data)


def _os_close(fd):
    _appending.discard(fd)
    _real_os_close(fd)


def install(meet, name, writers, timeout):
    """Give every append in this process Windows' two steps, holding each write
    between them until `writers` writers have sought (marked in `meet`, this one
    as `name`) or `timeout` seconds pass."""
    global _meet, _name, _writers, _timeout
    _meet, _name, _writers, _timeout = meet, name, writers, timeout
    builtins.open = _open
    io.open = _open
    pathlib.Path.open = _path_open
    os.open = _os_open
    os.write = _os_write
    os.close = _os_close
