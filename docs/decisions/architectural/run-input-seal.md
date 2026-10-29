# DA-run-input-seal: A strict run checks what it executes against a seal the run takes

## 1. Status

accepted

## 2. Context

Nothing detects that a plan's `states.yaml`, `scripts/`, `prompt_templates/` or
settings changed after a run began, or between two runs of one plan. A program
that rewrites a script beside its machine has the next ticket execute the
rewritten bytes, and an edit to `states.yaml` is executed by the next
`rhei run` without a word. Rhei execution is held to being predictable
([§GOAL-rhei-outcomes](../../goals.md#goal-rhei-outcomes-goals)) and its records to being auditable
([§GND-rhei-purpose.2](../../grund.md#2-what-this-project-does-about-it)), and neither holds when what ran is not what was there.

The `laid` fence ([§FS-rhei-library.7.1](../../functional-spec/rhei-library.spec.md#71-the-fence-comment-is-the-whole-of-template-provenance)) records where a plan came from. It
does not record what a run consumed, and it is not meant to: decision 5 of
[§DA-composition-by-union.3](composition-by-union.md#3-decision) refused a lock file, and its amendment for #514 defines
that refused file as "a sidecar file holding the same facts" as the fence.

## 3. Decision

**A strict run checks what it executes against a seal, and the seal is taken
by the run or by `rhei seal` — never by composition.** The seal is a record per
rhei of the bytes a run reads to execute it: the machine, the `scripts/` and
`prompt_templates/` directories beside it, the settings files, and the price
book. Each accepted change appends a generation and keeps the earlier bytes.
[§FS-rhei-seal](../../functional-spec/rhei-seal.spec.md#fs-rhei-seal-rhei-seal-and-the-run-input-seal) [§FS-rhei-run.3.8](../../functional-spec/rhei-run.spec.md#38-strict-inputs)

Four things follow, and they are the decision rather than its implementation.

1. **It is not decision 5's lock file.** The seal holds none of the fence's
   facts: no template name, version, `src:sha256`, `git:`, state word or
   `inputs:sha256`. Nothing that seals or checks reads the fence, so a laid
   plan and a typed plan are sealed the same way, and decision 4 of
   [§DA-composition-by-union.3](composition-by-union.md#3-decision) stands: a composed plan is still
   indistinguishable at runtime from one a person typed.
2. **`rhei instantiate` writes no seal.** The run that executes a rhei, or an
   explicit `rhei seal` any plan can use, takes it. A caller that wants edits
   made after laying to be caught runs `rhei seal` right after instantiating.
   A seal taken by composition would make a laid plan different at runtime from
   a typed one, which is the line this decision may not cross.
3. **It is called a seal, with generations.** Not a manifest, which
   [§DA-composition-by-union.3](composition-by-union.md#3-decision) decision 2 forbids beside the template's own,
   and not a lock, which is the vocabulary that decision removed.
4. **It is opt-in, and it is not attestation.** Only `rhei run --strict-inputs`
   checks a seal; without the flag nothing is written and nothing is refused
   ([§REQ-bounded-neural-work.2](../../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today)). It catches accidental drift under a trusted
   orchestrator: anything that can write the workspace can also run
   `rhei seal`. It checks the bytes of files and freezes no interpreter,
   sourced helper, subprocess, environment or agent CLI.

### 3.1 Alternatives rejected

**Seal at `rhei instantiate`.** It would cover only laid plans, and would make
composition write a per-plan record the runtime reads — the record decision 5
refused and the runtime distinction decision 4 forbids.

**A fresh baseline at every run start.** It never sees an edit made between two
runs, which is the case the triage of #521 showed executing silently.

**A machine key listing the sealed scripts.** Composition would have to learn to
merge it across templates. `scripts/` beside the machine is where every template
already ships its scripts and where the union already copies them, so the set is
declared by layout.

**"Every file that is not a task".** It sweeps in runtime and control files and
misses the settings that live outside the plan.

**`rhei run --reseal`.** Accepting a change is its own act, done after reading
what changed, not in the same breath as running it.

## 4. Consequences

A strict run refuses a spawn, program or callback whose sealed inputs changed,
starts nothing for it and charges nothing, and names each changed file. A
person restores the file or accepts it with `rhei seal`, and runs again.

For strict runs only, a sealed file is no longer freely editable mid-plan
([§GOAL-rhei-outcomes](../../goals.md#goal-rhei-outcomes-goals), "directly editable by humans and agents"): an edit needs
`rhei seal` before the run goes on. Task files stay outside the seal and freely
editable; what an invocation read of them is the per-invocation evidence of
#520, which also stamps the generation in force once both slices exist.

A template that writes into its own `scripts/` or `prompt_templates/` at run
time cannot run under `--strict-inputs`.
