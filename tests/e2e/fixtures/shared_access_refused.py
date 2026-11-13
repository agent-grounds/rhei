"""The one classifier of a read or replacement refused by an open handle.

Every fixture that reads or publishes a file the engine replaces by rename
includes this file, rather than a copy of the rule.
"""
import os


def _shared_access_refused(error):
    """Classify access/sharing/lock refusals for polling and publication.

    Other I/O errors propagate. §FS-rhei-run.3.7.3.1
    §FS-rhei-agent-visit-pairing.5.1 §AR-agent-orchestrator-workflow.3.3.1.1
    """
    return isinstance(error, PermissionError) or (
        os.name == 'nt' and getattr(error, 'winerror', None) in (5, 32, 33)
    )
