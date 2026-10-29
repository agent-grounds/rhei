# FS-rhei-seal: `rhei seal` and the run-input seal

A **seal** records, per rhei, the bytes a run reads to execute that rhei: its
machine, the scripts and prompt templates beside the machine, the settings
files, and the price book. A strict run ([§FS-rhei-run.3.8](rhei-run.spec.md#38-strict-inputs)) compares those
files with the seal before it starts anything, and refuses when one changed.
`rhei seal` accepts a change by appending a **generation**; every earlier
generation is kept byte for byte, so old evidence stays verifiable.
[§DA-run-input-seal](../decisions/architectural/run-input-seal.md#da-run-input-seal-a-strict-run-checks-what-it-executes-against-a-seal-the-run-takes)

The seal answers "did what this rhei executes change after it was sealed". It
does not answer where a plan came from — that is the `laid` fence
([§FS-rhei-library.7.1](rhei-library.spec.md#71-the-fence-comment-is-the-whole-of-template-provenance)) — and it records none of the fence's fields: no template
name, version, `src:sha256`, `git:`, state word or `inputs:sha256`. Nothing that
seals or checks reads the fence, so a laid plan and a typed plan are sealed the
same way, and `rhei instantiate` writes no seal.

## 1. Usage

```bash
rhei seal <RHEI_PLAN_OR_WORKSPACE> [--rhei <RHEI_ID>]
```

| Flag               | Default | Description                                                        |
|--------------------|---------|--------------------------------------------------------------------|
| `--rhei <RHEI_ID>` | all     | Seal only the named rheis (repeatable), as [§FS-rhei-run.2.5](rhei-run.spec.md#25-project-scope---rhei) narrows a run. |
| `--state-machine`  | resolved | The global flag; the file it names is sealed in place of the machine it replaces. |

Who takes a seal:

- the first `rhei run --strict-inputs` on a rhei that has no seal, which
  records generation 1 before its first spawn ([§FS-rhei-run.3.8](rhei-run.spec.md#38-strict-inputs));
- a strict run that admits a new member mid-run, which records that member's
  generation 1 on arrival;
- `rhei seal`, which records generation 1 for a rhei with no seal and appends
  generation n+1 for one that has. Run it right after laying a plan, and an
  edit made before the first run is caught as well.

Nothing else writes one. A non-strict run neither writes nor checks a seal, and
leaves one written earlier untouched.

## 2. What Is Sealed

The sealed set is per rhei and follows what a run reads to execute it, not
"every file that is not a task".

### 2.1. The Machine and Its Directories

- The machine that governs the rhei: its own `states.yaml`, the project default
  it inherits ([§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest)), or the file `--state-machine` names.
- The `scripts/` and `prompt_templates/` directories beside that machine,
  recursively: every regular file and symlink under them. The set is declared
  by layout — `scripts/` beside the machine is where every template ships its
  scripts and where composition copies them — so no machine key lists it.

A member that inherits the project default seals the project root's machine
and directories as its own inputs.

### 2.2. Settings and the Price Book

- The user settings file, `~/.config/rhei/settings.json`.
- **Both** project settings candidates, `.agent-grounds/rhei/settings.json` and
  the deprecated `.agents/rhei/settings.json`, at the project root
  ([§FS-rhei-agents.1.1](rhei-agents.spec.md#11-global-and-project-settings)). Each is recorded as present or absent, so a current
  file that starts shadowing the deprecated one is an addition.
- The price book the run resolved, from `defaults.prices` or `--prices`
  ([§FS-rhei-cost-accounting.5.1](rhei-cost-accounting.spec.md#51-price-book-selection)); `--prices` seals the file it names in place
  of the one it replaces.

Only digests are recorded, never content: copying a user settings file into a
workspace could copy credentials with it.

### 2.3. The Built-In Machine

A rhei governed by the built-in machine has no machine file and nothing beside
one. Its seal records no machine and no directories — only the settings files
and the price book. A `scripts/` directory next to such a plan is not sealed.

### 2.4. What Stays Out

Plan and task files, `runtime/`, `.rhei/`, the environment, the agent CLI, and
anything a command reaches outside `scripts/`. Task files are covered per
invocation by the evidence of #520, not by the seal. Flags that name no file —
`--agent`, `--model`, `--agent-mode`, the timeouts — are not sealed.

## 3. The Record

### 3.1. Generations

The seal of rhei `<rhei>` is the directory `runtime/seal/<rhei>/` under that
rhei's execution root ([§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)), with one file per generation,
`<n>.json`, numbered from 1. Single-file rheis that share an execution root
keep separate seals, keyed by rhei id.

A generation file is written once and never rewritten; an accepted change is a
new file. The **latest** generation — the highest `n` — is the one a strict run
compares against. Generation numbers never restart while `runtime/` exists
([§FS-rhei-seal.6](rhei-seal.spec.md#6-lifecycle)).

### 3.2. The Header

Each generation file is one JSON object, `rhei.seal.v1`, whose schema is
`crates/rhei-cli/schemas/rhei.seal.v1.schema.json`:

```json
{"directories":[{"path":"prompt_templates","present":false},{"path":"scripts","present":true}],"entries":[{"executable":false,"kind":"file","path":"scripts/step.py","sha256":"9f2c…"},{"executable":false,"kind":"file","path":"states.yaml","sha256":"41ab…"},{"kind":"absent","path":"/home/u/.config/rhei/settings.json"}],"generation":1,"machine":"states.yaml","rhei":"drift","rhei_build":{"commit":"d0446a7aa9…","dirty":false,"version":"0.6.4-dev"},"run_id":"55177a…","schema":"rhei.seal.v1","taken_at":"2026-10-29T01:00:00Z","taken_by":"run"}
```

- `schema` — always `rhei.seal.v1`.
- `generation` — `n`, matching the file name.
- `rhei` — the rhei id.
- `taken_by` — `run` when a strict run took it, `rhei seal` when the verb did;
  `run_id` is the run's id for `run` and `null` for `rhei seal`.
- `taken_at` — when, as an RFC 3339 UTC instant.
- `rhei_build` — the build that took it, as the run descriptor spells it
  ([§FS-rhei-run-headless.2](rhei-run-headless.spec.md#2-the-run-descriptor)).
- `machine` — the sealed machine's path, or `null` under the built-in machine.
- `directories` — each sealed directory with whether it existed, so a
  directory created later is an addition.
- `entries` — every sealed path ([§FS-rhei-seal.3.3](rhei-seal.spec.md#33-entries)).

### 3.3. Entries

- **Paths.** A path under the seal's **base** — the directory holding the
  machine, or the project root under the built-in machine — is relative to it
  and written with `/` on every platform. A file outside it, such as the user
  settings file, is named by its absolute path.
- **Kinds.** `file`, with `sha256` (lowercase hex of the bytes) and
  `executable`; `symlink`, with `target` (the link text) and `sha256` of the
  file it names, `null` when it names a directory or nothing; and `absent`, for
  a settings candidate or price book recorded as not present.
- **File modes.** `executable` is the owner-execute bit where the platform has
  one. On Windows it is `null` and is not compared ([§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity)).
- **Symlinks.** A link is recorded with its text and, when it names a file,
  that file's digest. A link to a directory is not followed. A link checked out
  as a plain file — Git on Windows without symlink support — is a plain file.

### 3.4. Canonical Encoding

The file's bytes are canonical JSON: keys sorted, no insignificant whitespace,
`entries` sorted by `path` and `directories` by `path`. It is the encoder the
fence's `inputs:sha256` already uses, so the same record always has the same
bytes and the same digest.

## 4. Changes

A strict run, and `rhei seal` when it reports what it accepted, compare the
files on disk with the latest generation. "Changed" means the bytes on disk.
Each difference has one kind:

| Kind       | Meaning                                                                 |
|------------|-------------------------------------------------------------------------|
| `modified` | A sealed file's bytes differ                                            |
| `added`    | A file appeared in a sealed directory, a sealed directory appeared, a settings candidate recorded absent now exists, or a rhei gained a machine of its own |
| `deleted`  | A sealed file, directory or settings file is gone, or a rhei lost its own machine |
| `mode`     | Only the executable bit differs                                         |
| `link`     | A symlink's text changed, or a path changed between symlink and file     |

## 5. `rhei seal`

`rhei seal` appends a generation for each rhei in scope and prints, per rhei,
the generation it wrote, that the earlier ones are kept, and each change it
accepted against the previous generation, one line per path with its kind:

```text
Sealed rhei `drift`: generation 2 (generation 1 kept)
  modified  scripts/step.py
```

A rhei with no seal gets generation 1, printed as
`` Sealed rhei `drift`: generation 1, <n> files ``. A rhei with nothing changed
still gets a new generation: sealing is an act of acceptance, and its record is
what later evidence names.

Like an operator-forced transition, `rhei seal` takes the run's roots without
waiting. While a run of the plan is live it refuses, names that run's id, and
writes nothing; it exits `1`.

## 6. Lifecycle

- A **full** `rhei reset` removes the seal with the rest of `runtime/`
  ([§FS-rhei-reset.2](rhei-reset.spec.md#2-behavior)).
- A **narrowed** `rhei reset --rhei` keeps every seal and says so on stdout
  ([§FS-rhei-reset.2.1](rhei-reset.spec.md#21-narrowed-reset---rhei)): a seal is keyed by rhei, not by ticket. Records that
  survive a narrowed reset may name a generation, and a second `1.json` at the
  same path would break the write-once rule. For a fresh baseline on one rhei,
  run `rhei seal --rhei <id>`.
- `rhei retry`, `rhei recover` and an interrupted run leave the seal alone.

## 7. What It Does Not Claim

The seal checks the bytes of files. It does not freeze an interpreter, a helper
sourced from outside `scripts/`, a subprocess, `PATH`, the environment, the
agent CLI, or a skills directory: checking a script's bytes freezes none of
what that script runs. It catches accidental drift under a trusted orchestrator
— an uncommitted template edit, a script edited mid-run. It is not attestation:
anything that can write the workspace can also run `rhei seal`.

Strict mode never reads the `laid` fence, so it never refuses a template that is
dirty or unversioned ([§FS-rhei-templates.6.1.5](rhei-templates.spec.md#615-source-identity)). `rhei transition`,
`rhei complete` and `rhei next`, run by hand, are not checked: strict mode is a
property of `rhei run`.

## 8. Strict Runs

What `rhei run --strict-inputs` does with the seal ([§FS-rhei-run.3.8](rhei-run.spec.md#38-strict-inputs)). Without
the flag none of it applies.

### 8.1. Who Seals

The run. At start, before its first spawn, a strict run records generation 1
for every in-scope rhei that has no seal, from the bytes it loaded and the
files beside them, and prints `` Sealed rhei `<rhei>`: generation 1, <n> files ``
for each. A rhei is never sealed lazily before its own first spawn: an edit
made by another rhei's worker in between would otherwise be sealed as laid. A rhei that has a seal is
compared with its latest generation, whichever run or `rhei seal` took it, so
an edit made between two runs is caught by the second. Only `--rhei` scope is
sealed and checked. `rhei instantiate` takes no seal.

### 8.2. Check Points

Before every agent spawn, every program, and every transition that runs a
callback, a strict run re-hashes the rhei's sealed files and compares them with
the latest generation. The check runs before bound admission
([§FS-rhei-run.3.4](rhei-run.spec.md#34-bound-admission)), so a refused start reserves nothing.

"Changed" means the bytes on disk. The copy the run holds needs no check of its
own: every load a strict run makes, at start and at each reload, hashes the
bytes it parses, so what it holds is only ever sealed bytes.

A callback that does not run — under `--no-callbacks` — is not checked.
`--continue-on-error` does not waive the check. `--dry-run` reports what the
check would find, or that it would seal, and writes nothing.

### 8.3. Settings Are Read Once

A strict run reads the settings files once, at start, and passes that kept
copy to everything that would otherwise re-read them during the run: the
transition, claim revalidation, the ready scan, the missing-output check,
auto-advance, and the halt report's input check. Nothing in a strict run
re-reads settings, so no decision rests on settings the seal does not cover.
A non-strict run reads as it does today.

### 8.4. The Refusal

A changed sealed input refuses the start on the halt path of a refused bound
admission:

- **State.** The ticket stays exactly where it is, with its artifacts. No
  transition fires and no result is written.
- **Charge.** None: no travel, no invocation, no attempt, no visit, no poll
  attempt ([§REQ-bounded-neural-work.5](../requirements/bounded-neural-work.spec.md#5-exhaustion-is-a-halt-where-the-ticket-stands)). Drift that only a person can fix does not
  spend the ticket's bound.
- **The rest of the run.** It starts nothing more in that rhei and carries on
  with the others.
- **Exit code.** `1` ([§FS-rhei-run-json.5](rhei-run-json.spec.md#5-exit-codes)), also under `--until-idle`: a
  refusal is work for a person.

The message names the ticket, each changed file with its kind
([§FS-rhei-seal.4](rhei-seal.spec.md#4-changes)), the generation it was sealed in, and the remedy:

```text
error: drift.2 was not started: a file sealed for rhei `drift` has changed
  modified  scripts/step.py   (sealed in generation 1)
  The ticket stays in 'step'. Nothing was charged.
  help: restore the file, or accept the change with `rhei seal drift`; then run again
```

Under `--json` the refusal is a `seal_halt` record
([§FS-rhei-run-json.2.1](rhei-run-json.spec.md#21-records)) carrying `reason_code: "sealed_input_changed"`. The
run report's Attention row reads `sealed input changed: <file>` with the next
action `rhei seal <plan>` ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)).

To resume, a person restores the file or runs `rhei seal`, then runs the same
`rhei run … --strict-inputs` again.

### 8.5. An Exit Found After Drift

A worker that started on sealed inputs may exit after a sealed file changed —
its own program edited a script, or a worker beside it did. What happens to
that exit depends on the transition it selects:

- A transition that runs **no callback** routes normally. The worker ran on
  sealed inputs and the run holds a sealed machine, so there is nothing to
  refuse. The next start in that rhei is refused.
- A transition whose **callback** is refused at its check point leaves the exit
  recorded the way [§FS-rhei-run.3.7.6](rhei-run.spec.md#376-stopping-attributed) records an exit that found a broken plan:
  interrupted and uncharged, with the exit unrouted, so the ticket stays in its
  state and a re-sealed plan resumes it as an interruption.

This holds with and without `--parallel`.

### 8.6. Admission

At a live admission ([§FS-rhei-panta.6.2](rhei-panta.spec.md#62-rhei-run)) each existing member's seal is checked
before its machine is re-adopted from disk.

- **An existing member whose sealed files changed is refused.** It keeps the
  machine it held, its in-flight work finishes, and nothing new starts in it.
- **A newly admitted member is sealed on arrival**, as its own generation 1,
  and it runs, so a producer that lays a follow-on member still has it
  considered by the enclosing run.

A member laid mid-run whose template ships `settings.json` merges it into the
project settings file every member reads: that is drift for the existing
members, which are refused, while the new member runs.
