# FS-rhei-recover: `rhei recover`

Resolve a forced state transition that was interrupted after its durable
recovery marker was published. Recovery is explicit and attended: ordinary
commands never finish or undo the operation as a side effect of loading the
root. This command is the only consumer allowed to remove
`.rhei/forced-recovery.json`. §FS-rhei-transition-cmd.6.1

## 1. Usage and authority

```bash
rhei recover <EXECUTION_ROOT>
```

`<EXECUTION_ROOT>` is the exact root printed by the pending-recovery
diagnostic, not a plan, project, or ticket target. The command reads the marker
directly before attempting to load any plan file. It accepts no `--yes`,
authorization file, environment/configuration grant, or machine-callable
alternative.

When no marker exists, the command exits successfully, prints `no forced
recovery pending`, and changes nothing. Otherwise stdin must be a terminal and
the invoking operator must type back the marker's recovery id, task id, exact
`from -> to` hop, and the previewed decision (`rollback` or `forward`), spelled
`recover <recovery-id> <task-id> <from> -> <to> <decision>`. A blank,
mismatching, EOF, or non-terminal response refuses without effects. Every new
invocation or retry requires a fresh typed response.

This ceremony makes an exceptional operation deliberate and attributes it to
the invoking operating-system account. It cannot authenticate a human against
an agent running as the same account and able to drive a pseudo-terminal;
agents are forbidden to invoke it. A site with genuine principal separation
MAY interpose its own verifier before the prompt, but rhei ships no verifier
and absence of one never disables operator recovery.

## 2. Marker format

The marker is UTF-8 canonical JSON with a trailing newline. Unknown versions,
unknown fields, duplicate keys, invalid base64url, paths outside their declared
owner, and a missing or malformed required field make it corrupt. Version 1 has
this shape:

```json
{
  "version": 1,
  "recovery_id": "018f...",
  "hop": {"task_id": "auth.1", "from": "human-gate", "to": "implement"},
  "files": [
    {
      "path": "tasks/01-work.md",
      "roles": ["task", "metadata"],
      "before": {"kind": "present", "bytes": "..."},
      "after": {"kind": "present", "bytes": "..."}
    },
    {
      "path": "runtime/results/auth.1.md",
      "roles": ["result"],
      "before": {"kind": "absent"},
      "after": {"kind": "present", "bytes": "..."}
    }
  ],
  "ledger": {
    "path": "runtime/state-transitions.log",
    "offset": 42,
    "prefix_sha256": "...",
    "metadata_line": "auth.1 !force-v1 ...\n",
    "movement_line": "auth.1 human-gate@implement\n"
  }
}
```

All keys shown are required. `recovery_id` is a fresh UUID v7 rendered in
lower-case hyphenated form. Marker paths use `/`, are relative to the execution
root, contain neither `.` nor `..` components, and identify regular files.
`files` is sorted by path and contains each path once. `roles` is a sorted,
non-empty subset of `task`, `metadata`, `checkpoint`, and `result`; it names
which transition effects the whole-file image covers. The task rewrite,
counted-visit metadata, supervision checkpoint, result append, assignee change,
and result-link addition/removal are therefore all explicit before/after bytes,
including when several roles share one physical plan file.

Version 2 retains this shape and adds a required `owner` on every file image:
`"execution-root"` for a contained image, or `"basin-project-metadata"` for the
basin's shared manifest. Files are sorted uniquely by `(owner, path)`; duplicate
resolved paths are forbidden. Version 1 has no owner field and retains exactly its
root-contained meaning. Writers use version 2 only for a basin transaction
with a project-metadata image; neither version changes the audit-pair format.

The typed `basin-project-metadata` owner is allowed only when the canonical
execution root is the regular, nonsymlink `basin/` directory immediately under
a project. Its sole allowed path is `index.panta.md`, resolving to the canonical
immediate parent's regular, nonsymlink manifest. Its roles are exactly
`["checkpoint", "metadata"]`, and both images are present complete manifest
bytes. The basin must have no authored `index.rhei.md`. No arbitrary parent or
absolute paths, alternative filenames, unknown owners, symlink escapes, or
ambiguous owner arrangements are allowed. All `execution-root` image paths
retain version 1's containment checks. Counted visits and checkpoints remain
in that manifest under qualified basin ids, with unrelated metadata and bytes
included in both images. §FS-rhei-panta.6

There is still exactly one marker, under `PROJECT/basin/.rhei/`, one explicit
recovery invocation targeting that basin, and one commit witness in its
`runtime/state-transitions.log`. The project has no second marker or witness.

An image is either `{"kind":"absent"}` or
`{"kind":"present","bytes":"<base64url>"}`. `bytes` is unpadded base64url
of the complete file, including its original newline convention; `absent`
means the path did not exist. Empty present files remain distinct from absent
files. The ledger is not duplicated in `files`: `offset` is its byte length
before the operation, `prefix_sha256` is lower-case hexadecimal SHA-256 of
those exact prefix bytes (the empty digest when the ledger was absent), and the
two lines are the exact UTF-8 bytes to append.

The coordinator writes the complete marker to a sibling temporary file, syncs
that file, atomically replaces the marker pathname, and syncs the `.rhei`
directory before changing any image or ledger byte. Every file replacement is
likewise temp-write, file-sync, rename, and parent-directory sync. The marker
remains durable until every selected image and the ledger outcome are durable.

## 3. Recovery decision

Recovery acquires the same locks and in the same order as the force operation:
all affected run locks in sorted canonical-root order, all exclusive root
guards in that order, then sorted metadata/task files and the transition
ledger (§FS-rhei-transition-cmd.6.1). Derive the affected owners from canonical
filesystem identities and the validated image owners, without parsing the
in-doubt plan: the execution root, plus the immediate parent project for a
version-2 basin manifest image, and filesystem-discovered project members
needed by dependent project reads. Preparation, file-image resolution and replay
use this same owner set. Recheck owner identities, image paths and marker
validity under the complete lock set before replay. Run-lock acquisition is non-blocking and
names the recorded owner on contention. After locking, recovery re-reads the
marker and ledger and verifies that the previewed marker bytes, id, hop, and
decision are unchanged. A changed or vanished marker refuses; it is never
treated as the confirmed operation.

The ledger bytes determine the only valid decision at the saved offset:

- When the current ledger is exactly the saved prefix, the operation did not
  commit. Recovery installs every `before` image and leaves the ledger at the
  saved offset: **rollback**.
- When the bytes after the saved prefix are a non-empty proper prefix of the
  exact adjacent metadata-plus-movement pair and there are no later bytes,
  recovery truncates precisely that torn suffix, syncs the ledger and parent,
  and installs every `before` image: **rollback**.
- When the exact complete pair starts at the saved offset and ends at EOF, the
  durable pair is the commit witness. Recovery syncs the surviving ledger and
  installs every `after` image: **forward**.
- A prefix digest mismatch, bytes at the offset that are not an exact prefix of
  the pair, a partial UTF-8 or newline variation, a reversed/non-adjacent pair,
  duplicate pair, or bytes after the complete pair are ambiguous evidence.
  Recovery refuses and names the marker and ledger evidence that must be
  restored; it neither truncates nor chooses a direction.

Before installing anything, recovery verifies every current affected file is
byte-identical to either that file's `before` or `after` image. Any third value
is evidence of an out-of-band edit and refuses the whole recovery. Installing
images is idempotent: an already-correct image is not appended or transformed.
The recovery id in the exceptional audit row is also checked before any append;
recovery never writes a second result entry, checkpoint, movement row, or
exception row.

Once the required images and ledger state are synced, recovery durably removes
the marker by unlinking it and syncing `.rhei`. An interruption before that
directory sync leaves a recoverable marker; the next explicit invocation makes
the same decision and repeats idempotently. Success reports the id, hop, and
`rolled back` or `rolled forward` outcome.

## 4. Pending-root interlock

Every rhei command and direct runtime consumer that can read or mutate an
execution root checks for the marker before and after acquiring a shared root
access guard and holds that guard through the operation. The guard is a stable
per-account lock keyed by SHA-256 of the canonical root in rhei's platform
state directory, so reading a read-only project does not require creating a
file inside it. The force and recovery coordinators take the same guard
exclusively before publishing, inspecting, or resolving a marker. Multi-root
operations discover the complete owner set before acquiring guards in sorted
canonical-root order, checking pending markers before and after acquisition.
Project/manifest access discovers the reserved `basin/.rhei/forced-recovery.json`
without parsing metadata; basin and project-member access includes the parent
project owner and discovers the same marker. Direct manifest writers retain
the complete guard set through their writes. An already active shared-manifest
writer finishes before force captures its before-image; later access refuses
while the marker survives. Nested loads reuse guards without reversing lock
order. Headless startup holds the shared owner set before launcher-lock
creation, descriptor reads, log truncation or spawning, through the parent
handshake; its child acquires the discovered run locks nonblocking before its
first shared acquisition, so it cannot queue behind a force waiting for the
parent's shared guards. It revalidates the discovered roots after loading.

Init includes both its host and destination in the guarded operation, including
host companion-file writes and creation of a previously absent destination
(§FS-rhei-init.2). It discovers the affected existing owners and retains their
sorted shared guards before any creation or other effect and through all writes.

External run-registry reporting may retain an uncheckable root as unknown using
only its registry entry (§FS-rhei-run-headless.3). Failure to inspect a root is
neither proof of pending recovery nor permission to read that root: no dependent
root reads, mutations, ended classification or pruning may follow. Unrelated
accessible entries remain resolvable under their own guards. An established
marker, including corrupt or unreadable contents, still causes strict refusal.

When a marker is present, every entry point other than `rhei recover` refuses
before returning plan, project, member, metadata, result, ledger, dashboard, or
watch data and before any mutation. This includes lenient `rhei list`, project
and member loading, watches, reset, run startup, and the viz, prompt-memory,
handoff, accounting, summary, and dashboard readers. The diagnostic prints the
recorded task and hop and exactly one command:

```text
rhei recover <execution-root>
```

No loader automatically rolls forward or back. With no marker, acquisition
and release of the shared guard are the only added behavior and existing
command bytes and effects remain unchanged. Older binaries cannot enforce this
interlock and must not access an in-doubt root.

## 5. Portability and interruption

Confirmation, canonical paths, user attribution, file and directory syncing,
atomic replacement, truncation, shared/exclusive guards, and run-lock owner
diagnostics have the same contract on Linux, macOS, and Windows.
§REQ-cross-platform

An injected or real interruption is recoverable at every boundary: before and
after marker publication; before and after each image replacement; between the
exception and movement rows; before and after ledger sync; and before and after
marker removal. At each boundary a later explicit recovery produces either the
complete original root with no pair or the complete after-root with exactly one
pair, result append, checkpoint, and visit update—never a mixed root. Basin
coverage includes each distinct task, whole project manifest and result image,
both owners' run locks, an active manifest writer and queued readers, and a
second interruption during every replay boundary. The basin ledger alone
chooses the outcome; unrelated manifest data and sibling files remain intact,
including updates completed before capture. Unknown owner/path arrangements,
third image values, and ambiguous or torn ledger evidence are checked across
both owners. Version-1 and ordinary workspace coverage remains required.
