from __future__ import annotations

import subprocess
from typing import Sequence

import rhei_cli

__version__ = "0.5.1-dev"


def run(
    args: Sequence[str] | None = None,
    *,
    capture_output: bool = False,
    check: bool = False,
    text: bool = True,
    cwd: str | None = None,
) -> subprocess.CompletedProcess[str]:
    return rhei_cli.run(
        args,
        capture_output=capture_output,
        check=check,
        text=text,
        cwd=cwd,
    )


def version() -> str:
    completed = run(["version"], capture_output=True, check=True)
    return completed.stdout.strip()


def help_text() -> str:
    completed = run(["--help"], capture_output=True, check=True)
    return completed.stdout
