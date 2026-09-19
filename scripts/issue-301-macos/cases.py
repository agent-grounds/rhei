"""Prepare disposable source copies for the actual full-suite §FS-rhei-validate.5 E2E."""

import hashlib
from pathlib import Path
import shutil
import tarfile

from execution import record_toolchain, required, write_json

HERE = Path(__file__).resolve().parent
TEST_FILE = "tests/e2e/export_prior_migration_implementation_tests.rs"
SOURCE_FILES = ["Cargo.lock", TEST_FILE, "crates/rhei-cli/src/lib.rs", "crates/rhei-cli/src/cli/states_render.rs"]


def prepare(checkout, scratch, output, name, revision, host=None, instrumented=False):
    """Keep revision/lockfile provenance, applying probes only to copies (§AR-ci-release.1)."""
    output.mkdir()
    archive = scratch / (name + ".tar")
    required(output, "archive", ["git", "archive", revision, "-o", archive], checkout)
    source = scratch / name
    source.mkdir()
    with tarfile.open(archive) as bundle:
        bundle.extractall(source, filter="data")
    archive.unlink()
    before = {path: hashlib.sha256((source / path).read_bytes()).hexdigest() for path in SOURCE_FILES}
    write_json(output / "source.json", {"revision": revision, "source": str(source), "sha256": before})
    actual_host = record_toolchain(output, source)
    if host and actual_host != host:
        raise RuntimeError("source host differs from previous suite")
    required(output, "fetch", ["cargo", "fetch", "--locked", "--target", actual_host], source, 600)
    if instrumented:
        required(output, "init", ["git", "init", "-q"], source)
        for patch in ["watch-trace.patch", "fixture.patch"]:
            required(output, patch, ["git", "apply", HERE / patch], source)
            shutil.copyfile(HERE / patch, output / patch)
        for filename, destination in [
            ("trace.rs", "crates/rhei-cli/src/cli/issue_301_trace.rs"),
            ("fixture.rs", "tests/e2e/issue_301_fixture.rs"),
        ]:
            shutil.copyfile(HERE / filename, source / destination)
            shutil.copyfile(HERE / filename, output / filename)
    if before["Cargo.lock"] != hashlib.sha256((source / "Cargo.lock").read_bytes()).hexdigest():
        raise RuntimeError("dependency preparation changed the pinned lockfile")
    return source, actual_host


def outside_capture(source, output):
    """The instrumented pair differs in exactly this one E2E path (§FS-rhei-validate.5)."""
    path = source / TEST_FILE
    before = path.read_text()
    old = 'let stderr_path = directory.join("watch-stderr.txt");'
    new = 'let stderr_path = _root.join("watch-stderr.txt");'
    if before.count(old) != 1:
        raise RuntimeError("expected one named watch capture site")
    after = before.replace(old, new)
    path.write_text(after)
    write_json(output / "capture-change.json", {"file": TEST_FILE, "old": old, "new": new})
