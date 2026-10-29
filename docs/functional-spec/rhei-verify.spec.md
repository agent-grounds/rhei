# FS-rhei-verify: `rhei verify`

Check that what a workspace records about its runs is still what those runs
wrote: every file a run manifest ([§FS-rhei-evidence.9](rhei-evidence.spec.md#9-the-manifest)) names still exists and
hashes as recorded. `rhei validate` checks a plan before it runs; `rhei verify`
checks what a run recorded after it ran. It reads only — it never finalizes a
run and never writes anything — and it calls no model.

It verifies a workspace **in place**, or an archive `rhei archive` packed
([§FS-rhei-archive](rhei-archive.spec.md#fs-rhei-archive-rhei-archive)) away from the workspace it came from. The checks, the
findings, the output and the exit status are the same for both.

## 1. Usage

```bash
rhei verify [RHEI_PLAN_OR_WORKSPACE | ARCHIVE] [--digest <SHA256>] [--json]
```

The positional is an **archive** when it names a regular file whose first 512
bytes are a tar header. Its first member must then be the index
`rhei-archive.json` ([§FS-rhei-archive.4.2](rhei-archive.spec.md#42-the-index)); a tar whose first member is
anything else is `archive_unreadable`, never resolved as a plan. Any other
positional resolves as below.

The positional resolves the run's root exactly as `rhei summary` does
([§FS-rhei-summary.1](rhei-summary.spec.md#1-usage)): a plan file, a workspace directory, or — omitted — the
nearest enclosing project, workspace or lone plan. Every manifest under
`runtime/evidence/runs/` of that root is verified, oldest run first.

`--digest` takes a full 64-hex digest ([§FS-rhei-evidence.9](rhei-evidence.spec.md#9-the-manifest)) and verifies only
the manifest whose file has it. A shortened digest is refused with exit `2`, as
any malformed argument is.

## 2. What It Checks

- every file a manifest's `files`, `accounting` and `seals` name exists and
  hashes as recorded;
- a finalized run directory holds no record its manifest does not name;
- each snapshot generation a manifest names, when it is in the cache the
  settings name, has a transcript whose bytes hash to the recorded
  `transcript_sha256`. The transcript itself is hashed; the generation's own
  `manifest.json` is only where its path is read.

### 2.1. From an Archive

An archive is read into memory, never unpacked to disk. Each check above reads a member instead of a file
at the same relative path, and a generation from the members the index lists
for its `ref` instead of the cache. The reader classifies members from the
index, never from a path prefix. The index only classifies: `--digest` does not
cover it, so it never names a member on its own. A member is named only when a
check reads it — the manifest, a file the manifest names, the `manifest.json`
and transcript of a generation whose `ref` the manifest's `snapshots` names,
and a reset event that lists the run. Anything else the index lists is
`unlisted`.

A member path is safe when it is relative, `/`-separated, not empty, and has no
`.` or `..` component, no drive and no `\`. Every member must be safe, a
regular file — or a GNU `././@LongLink` naming the next one — and appear once;
otherwise it is `archive_unsafe_member`. A member at a path a packed reset event
prunes for the run is `unlisted`: the packer never writes one, so it was added
afterwards. A member the index lists that is not in the archive is `missing`.


## 3. Findings

Each disagreement or omission is one **finding**, with a stable `reason_code`:

| `reason_code` | Meaning | Fails |
|---|---|---|
| `mismatch` | a named file hashes differently from its record | yes |
| `missing` | a named file is absent and no reset event explains it | yes |
| `unlisted` | a finalized run directory holds a record its manifest does not name; or an archive member nothing names, or one at a path a packed reset event prunes | yes |
| `unknown_digest` | `--digest` matches no manifest | yes |
| `archive_unreadable` | an archive is truncated, its first member is not `rhei-archive.json`, its index has an unknown `schema`, or anything but zero padding follows the end-of-archive blocks; `run_id` is null and nothing else is checked | yes |
| `archive_unsafe_member` | an archive member breaks the rule of [§FS-rhei-verify.2.1](rhei-verify.spec.md#21-from-an-archive), with its path | yes |
| `nothing_to_verify` | the root holds no manifest: a workspace from before evidence, or one with no finished run | yes |
| `pruned` | a path a reset event names for this run ([§FS-rhei-evidence.10.1](rhei-evidence.spec.md#101-the-reset-event)), whatever is there now | no |
| `snapshot_not_cached` | a named snapshot generation is not in the cache the settings name, wherever that is — or, in an archive, was not packed | no |
| `not_finalized` | a run directory with no manifest: a live run, or a killed one the next run has not finalized | no |

A `pruned` path is never also checked: the newer run's manifest covers what is
there now.

## 4. Output

Text output prints one block per run, then a closing line. Every digest is
printed in full, all 64 hex digits, because `--digest` needs it:

```console
$ rhei verify .
run 55177a  finished (exit 0)  manifest sha256:5be1c0d2…(64 hex)
  9 invocations (7 agent, 1 program, 1 callback), all complete
  31 files verified, 1 snapshot generation verified
verified: 1 run
```

A finding prints its reason in capitals, the path, and the recorded and found
hashes where it has them, and the closing line counts what failed:

```console
run 55177a  finished (exit 0)  manifest sha256:5be1c0d2…(64 hex)
  MISMATCH runtime/logs/task-ticket.review-review.log
    recorded sha256:77d0…(64 hex)
    found    sha256:03ab…(64 hex)
verification failed: 1 mismatch in run 55177a
```

### 4.1. `--json`

`--json` prints exactly one JSON object to stdout and nothing else:

```json
{
  "runs": [
    { "run_id": "55177a", "status": "finished", "manifest": "runtime/evidence/runs/55177a/manifest.json",
      "digest": "<64 hex>", "files_verified": 31, "invocations": 9 }
  ],
  "findings": [
    { "reason_code": "mismatch", "run_id": "55177a", "path": "runtime/logs/task-ticket.review-review.log",
      "recorded_sha256": "<64 hex>", "found_sha256": "<64 hex>" }
  ],
  "verified": false
}
```

A run that is `not_finalized` appears in `runs` with `digest: null`. A finding
with no run (`unknown_digest`, `nothing_to_verify`) has `run_id: null`; one with
no path or hash has `null` there. `verified` is what the exit status says. The
fields are additive within this shape: a consumer tolerates fields it does not
know and carries an unknown `reason_code` through.

## 5. Exit Status

`0` only when at least one manifest was verified and no finding fails. `1` on
any failing finding of [§FS-rhei-verify.3](rhei-verify.spec.md#3-findings), which `--json` splits by
`reason_code`. `2` on a usage error. Nothing else is encoded in the exit code.

## 6. Related Specifications

- [§FS-rhei-evidence](rhei-evidence.spec.md#fs-rhei-evidence-per-invocation-evidence) — the records and manifest it reads.
- [§FS-rhei-archive](rhei-archive.spec.md#fs-rhei-archive-rhei-archive) — the archive it also reads.
- [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) — the check before a run, which this is the counterpart of.
