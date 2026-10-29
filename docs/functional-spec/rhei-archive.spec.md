# FS-rhei-archive: `rhei archive`

Pack one finished run's evidence into one file that `rhei verify` checks away
from the workspace it ran in ([§FS-rhei-verify](rhei-verify.spec.md#fs-rhei-verify-rhei-verify)). In place, a manifest
([§FS-rhei-evidence.9](rhei-evidence.spec.md#9-the-manifest)) proves nothing once the workspace is reset or removed, and
a generation in a cache that has since gone cannot be checked at all. The
archive packs exactly what the manifest already names. It is not a second
evidence model: no evidence record gains a field. Under a trusted orchestrator
it is provenance, not attestation — whoever can rewrite the workspace can
rewrite the archive.

## 1. Usage

```bash
rhei archive [RHEI_PLAN_OR_WORKSPACE] --run <ID> --output <FILE> [--json]
```

The positional resolves the run's root exactly as `rhei verify` does
([§FS-rhei-verify.1](rhei-verify.spec.md#1-usage)). `--run` names one run directory under
`runtime/evidence/runs/`, and `--output` the file to write. Both flags are
required. An `--output` under the root's `runtime/evidence/` is a usage error.

## 2. What It Packs

The run's manifest, every path the manifest names in `files`, `accounting` and
`seals`, every reset event under `runtime/evidence/events/` that lists the run,
and every snapshot generation the manifest names that is in the cache. Nothing
else. A path a packed reset event prunes for this run ([§FS-rhei-evidence.10.1](rhei-evidence.spec.md#101-the-reset-event))
is not packed: the bytes now at it belong to a newer run.

### 2.1. Snapshot Generations

Every generation the manifest's `snapshots` names is packed when it is in the
cache the settings name ([§FS-rhei-snapshots.7](rhei-snapshots.spec.md#7-storage-layout)), wherever that cache is: the
default in-workspace `.rhei/cache/` and an absolute `snapshots.cache_dir` alike.
The manifest's `files` never lists a transcript, so without the generations the
archive would not be self-contained. A generation packs its `manifest.json` and
its transcript; its `current` pointer is not packed. A generation already gone
from the cache (`rhei snapshot gc`) is not packed, and verifying the archive
reports it `snapshot_not_cached`, as in place.

### 2.2. Seal Generations

Each seal generation file the manifest names ([§FS-rhei-evidence.9.1](rhei-evidence.spec.md#91-what-it-names)) is
packed like any other named file, at its path under `runtime/seal/`. The packer
reads every `{path, sha256}` list of the manifest, not only `files` and
`accounting` by name.

### 2.3. Values As Stored

A retained prompt, message or configuration ([§FS-rhei-evidence.5](rhei-evidence.spec.md#5-retained-values)) is packed
as the object store holds it: the original, the redacted copy under
`stored_sha256`, or nothing for `hash-only` and `withheld`. The archive never
re-runs the redactor and never recovers an original.

## 3. Writing

### 3.1. Checked In Place First

The run is verified in place ([§FS-rhei-verify.2](rhei-verify.spec.md#2-what-it-checks)) before anything is written.
Any failing finding — `mismatch`, `missing`, `unlisted` — refuses the archive,
and so does a run that is `not_finalized`, which `rhei verify` reports without
failing. An archive is only ever a copy of a run that was intact.

### 3.2. Checked While Written

Each member is hashed as it is written, generation transcripts included, and
compared against what the manifest records for it. A difference deletes the
temporary file and exits `1` with the finding. Members stream into a temporary
file beside `--output`, which is moved into place without replacing anything:
an `--output` that appeared in the meantime is refused, not overwritten.

### 3.3. Path Safety

Every path a manifest names goes through the member-path rule of
[§FS-rhei-verify.2.1](rhei-verify.spec.md#21-from-an-archive) before it is opened: relative, `/`-separated, with no `.`,
`..`, drive or `\`. It is opened without following symlinks, and a path that is
a symlink, or that resolves outside the run's root, is refused as
`archive_unsafe_member`. The configured snapshot cache is the one deliberate
read outside the root, and the no-symlink rule holds for a generation's files
there too.

### 3.4. Rhei Writes Only `--output`

Archiving changes nothing in the workspace and writes nothing anywhere else.
There is no default location and no retention: keeping, copying and deleting the
file is the caller's job. A later `rhei reset` does not touch it.

## 4. Format

An uncompressed tar: **ustar plus GNU long names**. A member whose path ustar
can hold has a ustar header; one it cannot hold is preceded by a GNU
`././@LongLink` entry. The reader accepts that one extension and refuses every
other entry that is not a regular file. `tar tf` lists an archive and
`tar xOf a.tar <member> | sha256sum` checks a member against the manifest with
no rhei at all.

### 4.1. Layout

Every member path is relative and `/`-separated. Members after the index are
sorted, with mtime `0`, uid and gid `0` and mode `0644`, so packing the same
run twice gives the same bytes.

```text
rhei-archive.json                            the index, always the first member
runtime/evidence/runs/<run-id>/manifest.json
<every path the manifest names>              at its path relative to the run's root
runtime/evidence/events/<uuid>.json          each reset event that lists the run
snapshots/<the generation's cache path>/manifest.json
snapshots/<the generation's cache path>/transcript.<ext>
```

A packed generation's directory is its `ref` spelled as the cache path
([§FS-rhei-snapshots.7](rhei-snapshots.spec.md#7-storage-layout)) under the fixed `snapshots/` prefix, whether the cache was
in the workspace or at an absolute `cache_dir`.

### 4.2. The Index

`rhei-archive.json` (`rhei.archive.v1`, published by `rhei schema`) belongs to
the archive, not to the evidence. It is canonical JSON:

```json
{ "schema": "rhei.archive.v1", "run_id": "55177a",
  "manifest": { "path": "runtime/evidence/runs/55177a/manifest.json", "sha256": "<64 hex>" },
  "events": ["runtime/evidence/events/<uuid>.json"],
  "snapshots": [ { "ref": { … }, "members": ["snapshots/…/g1/manifest.json", "snapshots/…/g1/transcript.jsonl"] } ] }
```

`events` lists the reset-event members and `snapshots` each packed generation's
`ref` with its member paths. A reader classifies members from these lists,
never from a path prefix: a Panta rhei may itself be called `snapshots`.

## 5. Output and Exit Status

```console
$ rhei archive . --run 55177a --output ~/evidence/55177a.tar
archived run 55177a  manifest sha256:5be1c0d2…(64 hex)  34 members  1 snapshot generation
  wrote /home/me/evidence/55177a.tar  sha256:91c4…(64 hex)
```

A refusal prints the run's block and findings as `rhei verify` does
([§FS-rhei-verify.4](rhei-verify.spec.md#4-output)) and a closing line saying nothing was written.

`0` when the archive was written. `1` when the run is unknown or not finalized,
a finding fails in place or while writing, the output exists, or a write fails;
nothing is written then. `2` on a usage error.

### 5.1. `--json`

`--json` prints exactly one object: the `rhei verify --json` object
([§FS-rhei-verify.4.1](rhei-verify.spec.md#41---json)) for the one run, plus `archive: {path, sha256}` — the file
written and its full digest — or `archive: null` when nothing was written.
`verified` is what the exit status says. Each refusal `rhei verify` has no code
for carries its own failing `reason_code`:

| `reason_code` | Meaning |
|---|---|
| `unknown_run` | `--run` names no run directory; `run_id` is the id given |
| `output_exists` | `--output` exists, before writing or when moving into place; `path` is it |
| `write_failed` | the output could not be written; `path` is it |

## 6. What It Carries In Clear

An archive is meant to leave the machine, so what it carries unredacted is said
here, in `rhei archive --help`, and beside the example in `docs/cli-examples.md`:

- `hash-only` and the redactor reach only retained values. Logs are never
  redacted, and they often repeat the prompt.
- Start records carry `argv` unredacted and the absolute `cwd`
  ([§FS-rhei-evidence.3.2](rhei-evidence.spec.md#32-argv-and-cwd)).
- Seal generations name the user settings path.
- A transcript is redacted only if the redactor ran when the generation was
  emitted.
- "No absolute path enters the archive" is true of member names only.

## 7. Related Specifications

- [§FS-rhei-verify](rhei-verify.spec.md#fs-rhei-verify-rhei-verify) — the checks, which an archive runs unchanged.
- [§FS-rhei-evidence](rhei-evidence.spec.md#fs-rhei-evidence-per-invocation-evidence) — the records and the manifest it packs.
- [§FS-rhei-snapshots.7](rhei-snapshots.spec.md#7-storage-layout) — the cache path a packed generation is addressed by.
