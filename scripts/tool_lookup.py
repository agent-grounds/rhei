#!/usr/bin/env python3
"""Where a tool the changelog scripts spawn actually lives. §REQ-cross-platform.3

`subprocess` hands a bare name straight to the platform's process launcher, and
on Windows that is `CreateProcess`, which appends `.exe` and consults nothing
else. A `git` or `gh` installed as a `.bat` or a `.cmd` - which is how a shim is
usually written, and how these scripts' own test stubs are written - is then not
found at all, and the script reports a forge it could not ask rather than a
lookup it did not do. `shutil.which` honours `PATHEXT`, so resolving the name
first and spawning what it resolves to is the one lookup that works on all three
platforms the gate runs on.
"""

from __future__ import annotations

import shutil


def tool(name: str) -> str | None:
    """The executable `name` resolves to on `PATH`, or `None` where it is absent."""
    return shutil.which(name)
