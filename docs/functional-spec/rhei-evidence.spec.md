# FS-rhei-evidence: Per-Invocation Evidence

What each process a run starts was given and what it produced, written once at
the moment the orchestrator knows it first-hand and never rewritten: the
rendered prompt, the configuration it was built from, the command and working
directory it ran with, the agent CLI's version, how it ended, and the log it
left. A run ends by writing one **manifest** over every file its evidence
names, and `rhei verify` ([§FS-rhei-verify](rhei-verify.spec.md#fs-rhei-verify-rhei-verify)) checks a workspace against those
manifests. [§GND-rhei-purpose.2](../grund.md#2-what-this-project-does-about-it) [§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)

Evidence is **provenance under a trusted orchestrator, not attestation**. An
agent running as the same user can rewrite a record and its manifest together;
what the evidence catches is accidental change and a workspace that no longer
holds the run someone saved the digest of. It is immutable objects plus a
manifest, not a hash chain: with parallel slots a chain adds ordering and
crash-recovery rules without adding authenticity.

Evidence records **what a run did**, as spawn records ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records))
and accounting records ([§FS-rhei-cost-accounting.3](rhei-cost-accounting.spec.md#3-invocation-record)) already do. It is not the
per-node record of where a plan's nodes came from that decision 5 of
[§DA-composition-by-union](../decisions/architectural/composition-by-union.md#da-composition-by-union-composition-is-graph-union-over-names-as-their-authors-wrote-them) refuses, so it does not reopen that decision.

Evidence decides nothing, with one exception. No admission, attempt, retry,
completion or budget check reads it, except where a planned log name is taken
and no spawn record accounts for it: there the conditions of
[§FS-rhei-evidence.11](rhei-evidence.spec.md#11-a-killed-attempts-log) read the killed run's start record, group record and
manifest, and only a spawn site acts on what it reads, by writing the spawn
record. From then on the spawn record is the authority for that attempt as for
every other. The spawn record stays the authority for attempt numbering and
proof of work, and the accounting record stays the authority for spend.

## 1. The Store

```text
runtime/evidence/                      in the run's root, beside run.json
  runs/<run-id>/<key>.start.json       one per process, written as it starts
  runs/<run-id>/<key>.end.json         one per process, written after it exits
  runs/<run-id>/<key>.group.json       one per process, written right after it starts
  runs/<run-id>/manifest.json          written once, when the run ends
  objects/sha256/<hex>                 retained prompts, messages and configurations
  events/<uuid>.json                   one per narrowed reset
```

The **run's root** is the directory whose `runtime/run.json` describes the run
([§FS-rhei-run-headless.2](rhei-run-headless.spec.md#2-the-run-descriptor)): the workspace, or in a Panta project the project
root. One run's evidence is one directory, `runs/<run-id>/`, so a run is one
thing to verify and one thing to pack ([§FS-rhei-archive](rhei-archive.spec.md#fs-rhei-archive-rhei-archive)). A member rhei's root is its
project's, whichever command asks — the run, a transition's callbacks, a reset
or `rhei verify` — even though the member keeps its own execution root.

### 1.1. Nothing Is Rewritten

- A record is created exclusively. A record that already exists is never
  replaced; the attempt that would have replaced it is refused ([§FS-rhei-evidence.3.3](rhei-evidence.spec.md#33-publication-and-refusal)).
- An object is written under a temporary name and renamed into place. An
  object already present is left alone, because the same name means the same
  bytes.
- No command but a full `rhei reset` ([§FS-rhei-evidence.10](rhei-evidence.spec.md#10-lifecycle)) deletes anything
  under `runtime/evidence/`.

### 1.2. Encoding and Paths

Every record, event and manifest is written in the canonical JSON encoding of
[§FS-rhei-summary.6.2](rhei-summary.spec.md#62-canonical-bytes-and-the-checkpoint-id) — keys sorted, no insignificant whitespace, one trailing
newline — by the same encoder the summary checkpoint uses. There is one such
encoder in rhei; evidence adds none.

Every hash is the lowercase hex sha256 of the raw bytes, with no newline
translation ([§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity)). Every path a record or manifest names is
relative to the run's root and separated by `/` on every platform, and a reader
joins it from its components ([§REQ-cross-platform.5](../requirements/cross-platform.md#5-paths-are-data)).

### 1.3. What Writes Nothing

- `rhei run --dry-run` spawns nothing and records nothing.
- `--no-callbacks` runs no callbacks, so it records none; `--no-agent` records
  the callbacks it runs.
- `rhei snapshot continue` is an attended session outside any run.
- An operator's `rhei transition`, `rhei complete` or `rhei recover` at a
  shell, with no live run in that workspace ([§FS-rhei-evidence.2.2](rhei-evidence.spec.md#22-callbacks-and-their-owner)).

## 2. Keys

Each process has one **key**, the stem of its two record files.

### 2.1. Agents and Programs

An agent keeps the `invocation_id` of [§FS-rhei-cost-accounting.3.7](rhei-cost-accounting.spec.md#37-attempt-identity). A program
gets one too, minted by the same shared spawn plan in the same spelling, with
`program` as its target segment:
`<task>::<state>::program::visit-<visit>::run-<run-id>::move-<moves>::attempt-<attempt>`.
A program still gets no accounting record.

`<key>` is the first 32 hex digits of the sha256 of the `invocation_id`. The raw
id is never a file name.

### 2.2. Callbacks and Their Owner

The firing id is never persisted ([§FS-rhei-transitions.1.2](rhei-transitions.spec.md#12-firing-identity-and-callback-time-visibility)), so a callback
cannot be keyed by it, and a ledger line exists only for a firing that
committed. Each callback process instead gets a fresh UUID, `callback_id`,
which is also its `<key>`. It names one process and says nothing about the
firing or its order. The record places it by facts that already exist: the
task, `from`, `to`, the callback's phase, and the ticket's move count.

Callback evidence is written only into a **live run**: the run's root of the
workspace the transition acts on ([§FS-rhei-evidence.1](rhei-evidence.spec.md#1-the-store)) has its run lock held and its id in `runtime/run.json`
([§FS-rhei-run-headless.3](rhei-run-headless.spec.md#3-run-identity-and-liveness)). With no live run nothing is written, because no
run's manifest could cover it. Nothing about the owner reaches a worker's
environment; [§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables) is unchanged.

`owner_invocation_ids` names the process the callback ran inside:

- a transition the orchestrator fires itself to route an exit names the one
  invocation whose exit it routes, on every callback of that transition —
  `on_leave` and `on_enter` alike — and on none after it;
- a `rhei transition <task>` or `rhei complete <task>` in a workspace with a
  live run names every start record of that task, in that run's directory,
  that has no end record — one for a single slot, all of them for a fan-out,
  sorted;
- in a live run with no open record for the task, the list is empty.

The callback's stdin carries the firing id, so it is neither stored nor hashed.

## 3. The Start Record

`rhei.evidence.invocation.v1` with `phase: "start"`. It is prepared and
published before the subprocess starts, so that a write that fails starts
nothing ([§FS-rhei-evidence.3.3](rhei-evidence.spec.md#33-publication-and-refusal)), and the group record that only the spawn can
fill in is a record of its own ([§FS-rhei-evidence.3.4](rhei-evidence.spec.md#34-the-group-record)).

### 3.1. Fields

| Field | agent | program | callback |
|---|---|---|---|
| `schema`, `phase`, `kind`, `run_id`, `task_id`, `state`, `started_at` | yes | yes | yes |
| id | `invocation_id` | `invocation_id` | `callback_id`, `owner_invocation_ids`, `from`, `to`, `callback_phase`, `moves` |
| `argv`, `cwd` | yes | yes | yes |
| `prompt` | a retained value ([§FS-rhei-evidence.5](rhei-evidence.spec.md#5-retained-values)) | — | — |
| `config` | a retained value ([§FS-rhei-evidence.6](rhei-evidence.spec.md#6-the-configuration-actually-consumed)) | yes | yes |
| `agent_cli_version`, `agent_cli_version_note` | yes ([§FS-rhei-evidence.7](rhei-evidence.spec.md#7-the-agent-cli-version)) | — | — |
| `snapshot` | [§FS-rhei-evidence.8](rhei-evidence.spec.md#8-the-snapshot-generation) | — | — |
| `grants` | copied from the spawn record | copied from the spawn record | — |
| `seal` | the checked generation, or `null` | the same | the same |
| `log_path` | the log's path, relative to the run's root | the same | — |

`rhei_build` and `checkout_head_at_start` are the manifest's, not each
record's ([§FS-rhei-evidence.9.1](rhei-evidence.spec.md#91-what-it-names)).

`seal` is `{rhei, generation, path}`: the seal generation ([§FS-rhei-seal.3.1](rhei-seal.spec.md#31-generations)) a
strict run ([§FS-rhei-seal.8.2](rhei-seal.spec.md#82-check-points)) compared the task's rhei against before this
start and found unchanged, and that generation file's path relative to the
run's root. The path is on the record so the manifest can name the file from
the records alone, a killed run's included. It is `null` in a run that is not
strict, which neither writes nor checks a seal, and for a callback a `rhei
transition` fires outside the run's process, which no strict run checked. A
record never names a generation the run did not check.

### 3.2. `argv` and `cwd`

`argv` is the argument list rhei handed the operating system, before Windows
quotes it into one command line. A prompt passed as an argument is shown as the
literal element `<prompt>`. A program declared with `shell: true`, or as a
string, records the shell it ran under: `["sh", "-c", "<line>"]`, or
`["cmd", "/S", "/C", "<line>"]` on Windows. `cwd` is the absolute working
directory the process was started in.

`argv` is recorded unredacted: a program's argv is authored in the plan the
workspace already holds, and an agent's argv carries flags and paths, not MCP
values, which reach it through a config file whose `env` is reduced to names
([§FS-rhei-evidence.6.1](rhei-evidence.spec.md#61-environment-names-only)).

### 3.3. Publication and Refusal

- A spawn that never starts removes its start record, as it removes its log
  ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)), so the next attempt can take the same key.
- A start record that cannot be written refuses the spawn, the way a log that
  cannot be created does. No worker runs unrecorded.

### 3.4. The Group Record

`rhei.evidence.group.v1`, at `runs/<run-id>/<key>.group.json`, written
exclusively right after the spawn, for an agent and for a program. It holds the
invocation's id and what the worker can be found by: on Unix `process_group`,
the leader's pid, which is its group's id ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)); on Windows `pid`
and `created_at`, that process's creation time. It cannot be part of the start
record, which is published before the spawn and never rewritten
([§FS-rhei-evidence.1.1](rhei-evidence.spec.md#11-nothing-is-rewritten)).

A group record that cannot be written is a warning, never a refusal: the worker
is already running, and the only cost is that a later kill of that attempt is
refused as an unaccounted log ([§FS-rhei-evidence.11.3](rhei-evidence.spec.md#113-when-the-evidence-does-not-account-for-the-log)). Its one reader is that
recovery, which is how a run tells whether a worker its killed predecessor
started is gone.

## 4. The End Record

`rhei.evidence.invocation.v1` with `phase: "end"`, the same key and id. It is
written after the log's footer, which is the log's last byte: late descendant
output has already been detached ([§FS-rhei-agent-output-drain.2](rhei-agent-output-drain.spec.md#2-output-written-after-the-exit-is-best-effort)).

| Field | agent | program | callback |
|---|---|---|---|
| `ended_at`, `exit_code`, `ending` | yes | yes | yes |
| `log` | `{path, sha256, bytes}` | `{path, sha256, bytes}` | — (a callback has no log) |
| `interventions` | [§FS-rhei-evidence.5.4](rhei-evidence.spec.md#54-interventions) | — | — |

`ending` uses the spawn record's vocabulary ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)). A process the
run interrupts gets `interrupted`. A run killed outright writes no end record;
that is what *incomplete* means ([§FS-rhei-evidence.9.3](rhei-evidence.spec.md#93-a-run-killed-outright)).

`ending` is how the process ended when its log closed, and the record is never
rewritten ([§FS-rhei-evidence.1.1](rhei-evidence.spec.md#11-nothing-is-rewritten)). An ending the engine decides later, from
what the attempt left on disk, is the spawn record's alone: the end record of a
withheld or a provider-unavailable attempt says `exited`
([§FS-rhei-agents.3.2.1](rhei-agents.spec.md#321-runtime-semantics)).

## 5. Retained Values

A prompt, an intervention message and a configuration object are each recorded
as a **retained value**: `{sha256, bytes, stored}`, plus `stored_sha256` when
`stored` is `redacted`. `sha256` and `bytes` are always the *original* bytes'.

| `stored` | What `objects/sha256/` holds |
|---|---|
| `original` | the original bytes, under `sha256` |
| `redacted` | the redactor's output, under `stored_sha256` |
| `none` | nothing: `evidence.retention` is `hash-only` |
| `withheld` | nothing: the redactor failed; the spawn still runs |

A redacted copy cannot verify the original hash, which is why the record says
which copy it holds — unlike a snapshot manifest, which deliberately does not
([§FS-rhei-snapshot-operations.4.2](rhei-snapshot-operations.spec.md#42-privacy-redaction-hook)).

### 5.1. The Prompt

`prompt.sha256` is the sha256 of the rendered prompt exactly as composed. The
bytes are the same whichever way it travels, and `prompt.delivery` says which:

- `argument` — passed with the profile's `prompt_flag`;
- `stdin` — written to the agent's stdin;
- `stdin-json` — claude-code with `intervene_stdin`, where rhei wraps the prompt
  in a stream-json message;
- `none` — a profile with neither a `prompt_flag` nor `stdin_prompt`.

### 5.2. Retention

`evidence.retention` ([§FS-rhei-agents.1.1.9](rhei-agents.spec.md#119-evidence)) governs all three kinds of
retained value together: `retain`, the default, keeps the bytes; `hash-only`
keeps none. Retaining is the default because a hash without the bytes cannot
answer what an agent was told. The store lives under `runtime/`, which
`rhei init` gitignores and a full reset deletes; the new exposure is
persistence, and the redactor and `hash-only` exist for it.

### 5.3. Redaction

When `snapshots.redactor` is set, every retained value is run through it under
the redactor process contract of [§FS-rhei-snapshot-operations.4.2](rhei-snapshot-operations.spec.md#42-privacy-redaction-hook), and the
stored copy is its output. There is one privacy hook, so a project that set it
for transcripts cannot leak prompts by forgetting a second one. A redactor that
exits non-zero or times out stores nothing for that value (`withheld`); unlike a
snapshot write, the spawn is not aborted.

### 5.4. Interventions

Each live message delivered to the agent ([§FS-rhei-viz.5](rhei-viz.spec.md#5-running-execution-view)) adds a retained value
with `at` and `outcome` to the end record's `interventions`, in delivery order.
Its bytes are the message as the operator sent it, without the newline rhei
adds when it writes the message to the agent's stdin.
The message is also mirrored into the log, so the log hash covers it too.
If the agent exits immediately after receiving a live message, that successful
delivery still belongs in its invocation's end record. Before finalizing that
record and its log hash, rhei accounts for deliveries already in progress and
finishes mirroring each successfully delivered message. Once that invocation's
delivery record is finalized, no further delivery to it can succeed. A failed
stdin write or an unreachable target adds no delivered intervention.
At invocation exit, a pending write whose success has not been acknowledged is
cancelled rather than waiting for a descendant that keeps stdin open without
reading. Acknowledgement and exit cancellation have one ordered boundary: an
acknowledged success still finishes its mirror and retention, while cancellation
reports failure and excludes the attempt from delivered interventions. A later
write completion cannot turn a cancelled attempt into a successful delivery.
`runtime/interventions.log` stays the operator's journal.

## 6. The Configuration Actually Consumed

`config` is the retained value of the JSON object rhei built the spawn from,
after every merge and resolution. Live admission reloads settings mid-run, so
what was consumed is the in-memory value, not a file on disk; sealing files is
another matter's.

- **An agent:** the resolved profile in the shape `rhei roster --json` gives it
  ([§FS-rhei-agents.1.1.7](rhei-agents.spec.md#117-inspecting-the-effective-roster)), the selected mode, the resolved model profile with
  its binding for this agent, effort, timeout and attempt budget, the MCP
  servers and skills left after availability checks, and the state's snapshot
  rules. The state's part sits under `effort` (the canonical value, before
  translation), `attempt_budget` (`{per, max}`, `per` being `visit` or `poll`),
  `snapshot` and `session`, each `null` when the state declares none.
- **A program:** the state's `program` object after template expansion, and
  its timeout.
- **A callback:** its declaration after resolution, and its time bound.

### 6.1. Environment Names Only

Every `env` map in the object — an MCP launch spec's, `program.env`, any other —
is recorded as the sorted list of its names. No value and no hash of a value is
written anywhere under `runtime/evidence/`: an unsalted hash of a short secret
can be confirmed by guessing.

### 6.2. The Bytes and the Hash

The object is encoded canonically ([§FS-rhei-evidence.1.2](rhei-evidence.spec.md#12-encoding-and-paths)), and `config.sha256`
is the sha256 of exactly those bytes, so `sha256sum` on a stored `original`
object recomputes it. A URL or argument can still carry a credential, which is
why the object follows the retention and redaction rules of
[§FS-rhei-evidence.5](rhei-evidence.spec.md#5-retained-values).

## 7. The Agent CLI Version

The profile field `version_args` ([§FS-rhei-agents.1.1.2](rhei-agents.spec.md#112-agents)) is appended to the
profile's **whole** `command` — never to its first element — and run with stdin
closed, bounded at 10 seconds, in the directory the agent is spawned in (the
record's `cwd`), so a wrapper named relative to the checkout resolves as it does
for the agent. A wrapper `["bash", "scripts/x.sh"]` is asked
`bash scripts/x.sh --version`, not bash. The recorded version is the first
non-empty line of stdout, trimmed.

- Every built-in declares `["--version"]`, which its own command accepts:
  `claude --version`, and `codex exec --version`. A profile with a `family`
  inherits its built-in's value; `version_args: null` turns the probe off.
- The probe runs once per run per distinct resolved command and directory, and
  again when a stamped file's size or modification time changes, because a CLI
  can update itself mid-run. The stamped files are the resolved executable and
  every later element of `command` that names a file from that directory: for
  `["bash", "scripts/x.sh"]` the script, which is what changes when the wrapper
  does, as well as bash.
- With no version, `agent_cli_version` is `null` and `agent_cli_version_note`
  says why: `no version_args`, `probe exited <code>`, `probe timed out`, or
  `probe printed nothing`.

The probe calls no model, so it spends nothing ([§REQ-bounded-neural-work](../requirements/bounded-neural-work.spec.md#req-bounded-neural-work-every-unit-of-neural-work-is-bounded-before-it-starts)).

### 7.1. Effort Fixture Version Queries

Regression coverage of this probe obeys
[§REQ-test-isolation.1](../requirements/test-isolation.spec.md#1-the-checkout-is-not-a-tests-directory):
the shared effort fixture, installed as `claude`, `codex`, `kilo` or `pi`,
answers its built-in version query (`--version`, or `exec --version` for
Codex) successfully with a deterministic non-empty version line. It recognizes
these query arguments before reading stdin, resolving a task root or requiring
run identity, and writes no task artifacts. This holds both without `RHEI_*`
identity and with an inherited outer run identity: a separate sentinel named by
the outer `RHEI_CHECKOUT_ROOT` stays empty while the inner run completes its task
and records the native effort arguments
([§FS-rhei-agents.2.2](rhei-agents.spec.md#22-modes)).

Only actual version-query arguments take this path; a task prompt containing
`--version` still executes and records its task normally. This is a fixture
requirement and regression scenario under the existing isolation rule; it adds
no production guarantee about the probe's inherited environment.

## 8. The Snapshot Generation

`snapshot` is `{ref, transcript_sha256}`: `ref` is exactly the `parent_ref` the
preload computed ([§FS-rhei-snapshots.6.1](rhei-snapshots.spec.md#61-parent_ref-semantics)), and `transcript_sha256` is copied
from that generation's own manifest ([§FS-rhei-snapshots.8](rhei-snapshots.spec.md#8-manifest-schema)). It is `null` when
the invocation ran cold. Lineage resolution ([§FS-rhei-snapshots.4.3](rhei-snapshots.spec.md#43-lineage-resolution)) is
untouched.

## 9. The Manifest

When a run ends, rhei writes `runs/<run-id>/manifest.json`
(`rhei.evidence.manifest.v1`) as the run loop ends, before the `run_finished`
record that carries its digest and before `run.json` takes its terminal status,
so the two agree. Its **digest** is the sha256 of the file,
which is canonical.

### 9.1. What It Names

- `run_id`, `status` (`finished`, `failed`, `interrupted` or `incomplete`),
  `exit_code`, `started_at`, `ended_at`, and `finalized_by` (`null` unless
  [§FS-rhei-evidence.9.3](rhei-evidence.spec.md#93-a-run-killed-outright) wrote it). `status` is `interrupted` for a run a signal
  ended, `incomplete` for one finalized after it was killed, and otherwise the
  terminal status `run.json` takes from the same exit code
  ([§FS-rhei-run-headless.2](rhei-run-headless.spec.md#2-the-run-descriptor)): `finished` for `0` and for the idle `3`, `failed`
  for any other;
- `rhei_build` and `checkout_head_at_start`, as on `run.json`;
- `invocations`: each process's `kind`, its id (`invocation_id` or
  `callback_id`), its `key`, and whether it `completed`;
- `files`: `{path, sha256}` for every file the run's records name — the records
  themselves, group records ([§FS-rhei-evidence.3.4](rhei-evidence.spec.md#34-the-group-record)) included, the logs and the
  stored objects;
- `accounting`: `{path, sha256}` for each invocation's accounting record, where
  it has one. In a Panta project those live in the owning rhei's accounting
  root ([§FS-rhei-cost-accounting.2](rhei-cost-accounting.spec.md#2-runtime-files)), and their paths are relative to the run's
  root like every other;
- `seals`: `{path, sha256}` for each seal generation file a record's `seal`
  names. A generation file is written once ([§FS-rhei-seal.3.1](rhei-seal.spec.md#31-generations)), so any
  difference later is a real `mismatch`;
- `snapshots`: `{ref, transcript_sha256}` for every generation used or emitted.

### 9.2. Where It Is Announced

`runtime/run.json` ([§FS-rhei-run-headless.2](rhei-run-headless.spec.md#2-the-run-descriptor)) and the `run_finished` record
([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)) both gain `evidence_manifest: {path, sha256}`, the
manifest's path and its full digest. Both are additive under
[§FS-rhei-run-json.2.2](rhei-run-json.spec.md#22-schema-version), so `schema` stays `1`. It is `null` on a dry run, and on
`run.json` until the run ends.

### 9.3. A Run Killed Outright

A killed run leaves its directory with no manifest. The next `rhei run` in that
workspace, once it holds the run lock, finalizes it: `status: incomplete`,
`finalized_by` the new run's id, each process with a start record and no end
record listed with `completed: false`, and each log recorded as found at that
moment, located by the start record's `log_path`. No end record is invented for a process that has none. That
finalization is the recovery event. For an agent it is what lets the next spawn
account for the killed attempt's log and resume its visit, once its group is
gone ([§FS-rhei-agents.8.1.1](rhei-agents.spec.md#811-an-agent-killed-with-its-run)); for a program it is what lets the refusal say the
program may have finished ([§FS-rhei-evidence.11.3](rhei-evidence.spec.md#113-when-the-evidence-does-not-account-for-the-log)). Neither reads a log's
bytes for meaning: the hash recorded here is what ties the log to the attempt.

### 9.4. The Manifest and the Checkpoint

The summary checkpoint ([§FS-rhei-summary.6](rhei-summary.spec.md#6-the-checkpoint-document)) says *which* invocations a summary
covered, and may span runs; a manifest says what each process of one run was
given and produced. They share the encoding and the invocation ids, so they join
on id, and neither embeds the other.

## 10. Lifecycle

| Command | Existing evidence | Writes |
|---|---|---|
| `rhei reset` | deleted with the rest of `runtime/` | nothing |
| `rhei reset --rhei` | kept, and its output says so | one reset event ([§FS-rhei-evidence.10.1](rhei-evidence.spec.md#101-the-reset-event)) |
| `rhei retry` | untouched; it rewrites only spawn records | nothing; the granted attempt's start record carries the grant |
| `rhei recover` | untouched | nothing; its audit pair in the ledger is the record |
| a run interrupted by Ctrl+C or `rhei stop` | — | end records with `ending: interrupted`, and the manifest with the run's status |
| a run killed outright | start records with no end record | the next `rhei run` finalizes it ([§FS-rhei-evidence.9.3](rhei-evidence.spec.md#93-a-run-killed-outright)), and an agent visit it cut off resumes from it ([§FS-rhei-evidence.11](rhei-evidence.spec.md#11-a-killed-attempts-log)) |
| `rhei snapshot gc` | untouched | nothing; `rhei verify` reports a pruned generation as not cached |
| `rhei remove` | untouched | nothing |
| `rhei archive` | untouched | nothing in the workspace; only `--output` ([§FS-rhei-archive.3.4](rhei-archive.spec.md#34-rhei-writes-only---output)) |

### 10.1. The Reset Event

A narrowed reset ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)) removes the ticket's ledger lines, so the
next entry is entry 1 again and takes the plain log name: the next run writes
new bytes at a path an earlier manifest names. So it writes
`events/<uuid>.json` (`rhei.evidence.event.v1`): its `event_id`, `kind:
"reset"`, `at`, the rhei ids and tickets it reset, `runs` — every run id with a
directory under `runs/` when the event was written — and `removed`, every path
under the run's root it removed. A removed path is `pruned` for each manifest of
a run the event lists, whatever is at that path now; the newer run's own
manifest covers the new bytes ([§FS-rhei-verify.3](rhei-verify.spec.md#3-findings)). Ordering comes from that
list, never from timestamps.

For a `--task` scope ([§FS-rhei-reset.2.5](rhei-reset.spec.md#25-ticket-scope---task)), `tickets` is its closure, and `rheis` is
the sorted, distinct rheis those tickets belong to, though the reset left their
other tickets as they were: `rheis` says where the event's paths are, not what
was reset whole.

A log moved away by hand, the other remedy for a refused log, is later reported
as missing: the reset is the remedy that records why the log went.

## 11. A Killed Attempt's Log

The one place evidence is read to decide something
([§FS-rhei-agents.8.1.1](rhei-agents.spec.md#811-an-agent-killed-with-its-run)): a planned log name is taken, no spawn record accounts
for it, and the records of a run killed outright may.

### 11.1. The Conditions

A start record accounts for the taken log as a killed attempt when all six hold:

1. its run has a manifest with `status: incomplete`, whichever run finalized it
   ([§FS-rhei-evidence.9.3](rhei-evidence.spec.md#93-a-run-killed-outright));
2. it has no end record;
3. it says `kind: agent`;
4. its `task_id` and `state` are the spawn's; its `log_path`, resolved against
   the run's root, names the planned log, the two compared in canonical form;
   and the `move-<m>` and `attempt-<a>` of its `invocation_id`
   ([§FS-rhei-evidence.2.1](rhei-evidence.spec.md#21-agents-and-programs)) are the spawn's move count and attempt;
5. the planned log's sha256 equals the hash that manifest recorded for it. An
   agent's output reaches its log only through the run's pipes, so once the run
   is dead nothing writes it, and the hash taken at finalization is its final
   content;
6. its group record ([§FS-rhei-evidence.3.4](rhei-evidence.spec.md#34-the-group-record)) exists and the group is gone
   ([§FS-rhei-evidence.11.2](rhei-evidence.spec.md#112-gone)).

The readings that write nothing — a run's budget check and halt verdict, `rhei
next`, `rhei list`, claim revalidation and `--dry-run` — apply the same
conditions. Only a run holding the ticket's run lock finalizes a killed run, and
`rhei run` finalizes before it plans its first spawn, so the run that left the
records cannot still be alive; until some run has finalized it, those readings
predict the refusal. An interruption never reaches these conditions: it writes
its own spawn record ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)). Nor does a worker whose spawn record failed to
write ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)): it has an end record and its run a manifest.

### 11.2. Gone

Read from the group record, never guessed. On Unix the group is gone only when
signalling it with signal `0` fails with `ESRCH`; `EPERM`, an unreaped zombie
still in the group, and any other error mean it is not. On Windows the agent is
gone when opening its pid finds no process, or one whose creation time differs
from `created_at`, because a pid is reused only after its process has exited;
"access denied", and any other failure to open or query it, mean it is not. A
reused group id or pid can therefore cost a refusal, never cause a resume. On
Windows only the agent itself is checked, not what it started: that is the reach
termination has there today ([§REQ-cross-platform](../requirements/cross-platform.md#req-cross-platform-one-tool-on-linux-macos-and-windows)).

### 11.3. When the Evidence Does Not Account for the Log

Where those records name the taken log but do not account for it, the refusal
says which case, in order:

- **Still running**: the group is not gone ([§FS-rhei-evidence.11.2](rhei-evidence.spec.md#112-gone)). This wins
  over every other case, because any remedy but stopping it starts a second
  worker beside the first ([§DA-supervised-process-groups](../decisions/architectural/supervised-process-groups.md#da-supervised-process-groups-subprocesses-are-supervised-process-groups-with-one-termination-path)): `refusing to spawn:
  <path> belongs to a worker whose run was killed, and its process group still
  has processes`, one line per process left in the group with its pid and
  command line, and the help to stop them if they are the killed worker's and
  run again. No `kill` command — the id may name another group by the time it
  is read — and neither moving the file nor a reset is offered.
- **A program**: only the `kind` condition fails. `refusing to spawn: <path>
  belongs to a program whose run was killed while it ran; it may have finished`,
  and the help: if it did, one of the listed `rhei transition` commands, one per
  edge out of the state and runnable as printed; if not, move the file away or
  run the reset of [§FS-rhei-reset.5.1](rhei-reset.spec.md#51-offering-reset-as-a-remedy). rhei never restarts on its own a program
  its killed run started: it may already have merged, posted or claimed.
- **No group record**: everything else holds — a run killed before that write,
  a failed write, a workspace killed before group records existed. The
  [§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming) refusal plus `its run was killed, but rhei cannot tell whether that agent has
  stopped`.
- **Otherwise** the [§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming) refusal, word for word. Moving the log away still
  wins: with nothing at the name the state starts as attempt 1, plainly named.
