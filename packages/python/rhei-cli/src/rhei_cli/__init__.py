from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path
from typing import Iterable, Sequence

__version__ = "0.5.0"
_CRATE_VERSION = "0.5.0"


def _install_root() -> Path:
    override = os.environ.get("RHEI_CLI_INSTALL_DIR")
    if override:
        return Path(override)
    return Path.home() / ".cache" / "rhei-cli" / _CRATE_VERSION


def binary_path() -> Path:
    exe = "rhei.exe" if os.name == "nt" else "rhei"
    return _install_root() / "bin" / exe


def ensure_binary() -> Path:
    binary = binary_path()
    if binary.exists():
        return binary

    cmd = [
        "cargo",
        "install",
        "rhei-cli",
        "--version",
        _CRATE_VERSION,
        "--root",
        str(_install_root()),
        "--locked",
        "--force",
    ]
    try:
        subprocess.run(cmd, check=True)
    except FileNotFoundError as exc:
        raise RuntimeError(
            "failed to run cargo. Install Rust/Cargo from https://rustup.rs, "
            "then run rhei again."
        ) from exc

    if not binary.exists():
        raise RuntimeError(f"cargo install completed, but {binary} was not created")
    return binary


def run(
    args: Sequence[str] | None = None,
    *,
    capture_output: bool = False,
    check: bool = False,
    text: bool = True,
    cwd: str | os.PathLike[str] | None = None,
) -> subprocess.CompletedProcess[str]:
    binary = ensure_binary()
    return subprocess.run(
        [str(binary), *(args or [])],
        capture_output=capture_output,
        check=check,
        text=text,
        cwd=cwd,
    )


def main(argv: Iterable[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    binary = ensure_binary()
    completed = subprocess.run([str(binary), *args])
    return int(completed.returncode)
