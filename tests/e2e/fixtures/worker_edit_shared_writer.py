"""Publish only this fixture's shared task image under the engine's sidecar.

The authoritative pathname read happens after writer acquisition, and the
sidecar stays held through same-directory replacement. This preserves the
sibling's terminal finalization as well as its state. §FS-rhei-run.3.7.3
§AR-agent-orchestrator-workflow.3.3.1
"""
import contextlib
import errno
import tempfile


def _try_shared_writer_lock(handle):
    """Overlap fs2's exclusive lock on the permanent sidecar, never the plan.

    Unix uses flock, as fs2 does. Windows's CRT byte-range lock overlaps fs2's
    LockFileEx range at byte zero, including on an empty sidecar; no dummy byte
    or second lock identity is needed. §AR-agent-orchestrator-workflow.3.3.1
    """
    try:
        if os.name == 'nt':
            import msvcrt
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        return True
    except OSError as error:
        contended = error.errno in (errno.EAGAIN, errno.EWOULDBLOCK)
        if os.name == 'nt':
            contended = contended or error.errno in (errno.EACCES, errno.EDEADLK)
        if not contended:
            raise
        return False


@contextlib.contextmanager
def _shared_writer(plan):
    """Bound acquisition; closing the handle releases the OS lock on all exits.

    Only this task file is edited, so no metadata or ledger locks are needed.
    Keep the empty sidecar permanently. §AR-agent-orchestrator-workflow.3.3.1
    """
    sidecar = plan.parent.resolve() / (plan.name + '.lock')
    with sidecar.open('a+b') as handle:
        deadline = time.monotonic() + 10
        while not _try_shared_writer_lock(handle):
            if time.monotonic() >= deadline:
                raise RuntimeError('shared fixture writer lock timed out: ' + str(sidecar))
            time.sleep(0.02)
        yield


def _publish_shared_image(plan, text):
    """Replace a complete staged image while the caller retains the sidecar.

    Retry only access/sharing/lock refusals for two seconds, with the same
    staged file; never truncate the destination or drop exclusion. Clean the
    staging file on failure. §AR-agent-orchestrator-workflow.3.3.1.1
    §FS-rhei-run.3.7.3
    """
    staged = None
    try:
        with tempfile.NamedTemporaryFile(
            mode='w', encoding='utf-8', newline='', dir=plan.parent,
            prefix=plan.name + '.', suffix='.staged', delete=False,
        ) as handle:
            staged = pathlib.Path(handle.name)
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        deadline = time.monotonic() + 2
        while True:
            try:
                os.replace(staged, plan)
                return
            except OSError as error:
                refused = isinstance(error, PermissionError) or (
                    os.name == 'nt' and getattr(error, 'winerror', None) in (5, 32, 33)
                )
                if not refused or time.monotonic() >= deadline:
                    raise RuntimeError(
                        f'cannot publish shared fixture {staged} -> {plan}: {error}'
                    ) from error
                time.sleep(0.02)
    finally:
        if staged is not None:
            staged.unlink(missing_ok=True)
