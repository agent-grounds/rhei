# CLI examples

When a visit has exhausted its attempts, grant it another configured budget
without moving it. A supervisor keeps its held subtree and unread checkpoints
([§FS-rhei-retry.2](functional-spec/rhei-retry.spec.md#2-grant-and-refusals)); the grant takes effect on the next run:

```bash
rhei retry plan.1
rhei run plan.rhei.md
```

Strict inputs: seal what a laid plan executes — its machine, the `scripts/`
and `prompt_templates/` beside it, the settings files and the price book — and
refuse to start work in a rhei after one of them changed
([§FS-rhei-run.3.8](functional-spec/rhei-run.spec.md#38-strict-inputs)). Seal right after laying, so an edit made
before the first run is caught too:

```bash
rhei seal drift                      # Sealed rhei `drift`: generation 1, 3 files
rhei run drift --strict-inputs       # refuses a ticket once scripts/step.py changes
```

```text
error: drift.2 was not started: a file sealed for rhei `drift` has changed
  modified  scripts/step.py   (sealed in generation 1)
  The ticket stays in 'step'. Nothing was charged.
  help: restore the file, or accept the change with `rhei seal drift`; then run again
```

Nothing was charged, so accepting the edit and running again starts the ticket
as its first attempt. Earlier generations are kept byte for byte
([§FS-rhei-seal.5](functional-spec/rhei-seal.spec.md#5-rhei-seal)):

```bash
rhei seal drift                      # Sealed rhei `drift`: generation 2 (generation 1 kept)
rhei run drift --strict-inputs
```

Validate a plan with the built-in default states definition:

```bash
cargo run -p rhei-cli -- validate examples/release-automation.rhei.md
```

Validate using a specific states file:

```bash
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml validate examples/release-automation.rhei.md
```

Watch a plan and states file for changes:

```bash
cargo run -p rhei-cli -- validate --watch examples/release-automation.rhei.md
```

Repair missing export-prior edges with `rhei migrate export-priors --dry-run
PATH`, then `rhei migrate export-priors PATH`; see [§FS-rhei-migrate](functional-spec/rhei-migrate.spec.md#fs-rhei-migrate-rhei-migrate).

Render a plan as pretty JSON:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format json --pretty
```

Inspect the effective agents, models, and defaults for a plan:

```bash
cargo run -p rhei-cli -- roster examples/release-automation.rhei.md
```

Emit the complete merged roster and its provenance as JSON:

```bash
cargo run -p rhei-cli -- roster examples/release-automation.rhei.md --json
```

Both views use the execution settings merge; see
[Agents and models](functional-spec/rhei-agents.spec.md#11-global-and-project-settings).

Read one ticket's body, whatever state it is in:

```bash
cargo run -p rhei-cli -- show examples/release-automation.rhei.md --task 1
```

The same body for a script, as one object with `id`, `title`, and `content`:

```bash
cargo run -p rhei-cli -- show examples/release-automation.rhei.md --task 1 --json
```

Both name the plan because this page is run from the repository root. The
everyday shape is shorter: from inside a project the positional may be the bare
ticket id, as `rhei show <ticket-id>`, which prints the same body. See
[§FS-rhei-show](functional-spec/rhei-show.spec.md#fs-rhei-show-rhei-show).

Leave one fact for whoever works next. `note` takes no plan positional: the
store belongs to the project rather than to a plan, so it is found from the
working directory, and these are run from inside the project:

```bash
rhei note "Three concurrent cargo builds under /tmp fill the root disk; build under ~/ag/tmp with --target-dir."
rhei note --restate auth.2
rhei note --strike auth.1
```

A task holds one slot, so the restate and the strike each spend the writer's
own. These run where `RHEI_TASK_ID` names the writing task: a program state, a
`cli:` callback, or `rhei snapshot continue`. An agent `rhei run` spawns has no
such variable, and neither does a manual worker, so either passes the writing
task as `--task <ticket-id>`, the qualified id in its prompt's task heading.
The agent's prompt prints that command with the id already in it. Both refusals
happen before anything is appended, and neither leaves a file behind:

```console
$ rhei note "$(printf 'one\ntwo\nthree\nfour')"
  × an entry is bounded at 3 lines and this one has 4; nothing was written
  help: shorten it, or leave the detail in your result file and note where it
        is.

$ rhei note "Three concurrent cargo builds under /tmp fill the root disk; build under ~/ag/tmp with --target-dir."
  × auth.1 already left that entry; nothing was written
  help: endorse it instead, which costs you the same slot: rhei note --restate
        <task-id>.
```

Every ordinary prompt composed afterwards, anywhere in the project and in any
rhei, carries the live entries under `### Project Notes` inside `## Position`.
See [§FS-rhei-note](functional-spec/rhei-note.spec.md#fs-rhei-note-rhei-note).

Take back a ticket filed by mistake, before anything has acted on it. Run from
inside the project; `--dry-run` checks and previews without changing a byte:

```console
$ rhei remove auth.2 --dry-run
Would remove auth.2; would retire auth.2
  would rewrite index.panta.md
  would rewrite auth.rhei.md
  would delete runtime/exports/auth.2

Dry run — nothing was changed.

$ rhei remove auth.2
removed auth.2; id retired
  deleted runtime/exports/auth.2
```

The id is retired rather than freed, so the next `rhei new --under auth` creates
`auth.3`. A ticket something names, or one that has history — a transition, a
claim, a result, a log — is refused with the whole list and changes nothing;
move it to its machine's cancellation state instead:

```console
$ rhei remove auth.1
  × auth.1 cannot be removed: auth.2 names it in **Prior:**
  help: change or remove what names it first, then remove it.
```

See [§FS-rhei-remove](functional-spec/rhei-remove.spec.md#fs-rhei-remove-rhei-remove).

Render a plan as GitHub-style markdown without metadata or subtask body text:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format github --no-metadata --no-content
```

Render a terminal progress report without ANSI color:

```bash
cargo run -p rhei-cli -- render examples/release-automation.rhei.md --format progress --no-color
```

Reprice one completed run from its recorded measurements with a later book:

```bash
rhei cost WORKSPACE --by run
rhei summary WORKSPACE --run RUN_ID --prices later-prices.json
```

This summary is read-only: ordinary accounting keeps its stored prices. The
positional workspace and any repeatable `--rhei` flags bound which run records
and immutable completion report may be selected.

Provenance for a pull request: a `describe` step freezes the summary it
prints, and a later `ship` step prints exactly that block again, however many
records were written in between:

```bash
rhei summary WORKSPACE --print-checkpoint > checkpoint.json   # in describe
rhei summary WORKSPACE --details                              # the PR body
rhei summary WORKSPACE --details --checkpoint checkpoint.json # in ship: the same bytes
```

Every summary ends with the line that says which build ran, which commit the
checkout started on, which template the plan was laid from, and which set of
records it covers:

```text
Provenance: rhei 0.6.4-dev @ d0446a7 · checkout HEAD at start agent-grounds/rhei@a1b2c3d · template `supervised-ticket-fix` 1.0.0 @ agent-grounds/agent-grounds@d292a9b (clean) · checkpoint 3f9a2c41d0b7: run 55177a, 7 invocations
```

The checkpoint names no path, so it may be stored beside the pull request. A
record or spawn it names that has since been reset away is refused by name. See
[§FS-rhei-summary.6](functional-spec/rhei-summary.spec.md#6-the-checkpoint-document) and [§FS-rhei-summary.2.4](functional-spec/rhei-summary.spec.md#24-the-provenance-line).

Check what a run recorded. Every `rhei run` writes a start and an end record
for each agent, program, and callback it starts under `runtime/evidence/`, and
a manifest when it ends; `run.json` and the `run_finished` record name that
manifest's digest. `rhei verify` re-hashes every file a manifest names, oldest
run first, or only the run whose manifest has the digest given:

```bash
rhei verify WORKSPACE
rhei verify WORKSPACE --digest "$(jq -r .evidence_manifest.sha256 WORKSPACE/runtime/run.json)"
rhei verify WORKSPACE --json
```

It exits `0` when everything named still matches, `1` on any mismatch,
missing or unlisted file, and `2` on a usage error such as a short digest. A
path a narrowed `rhei reset --rhei` removed is reported as pruned, not missing.
See [§FS-rhei-verify](functional-spec/rhei-verify.spec.md#fs-rhei-verify-rhei-verify) and [§FS-rhei-evidence](functional-spec/rhei-evidence.spec.md#fs-rhei-evidence-per-invocation-evidence).

Name one price book for every run on this machine, instead of threading
`--prices` through each entry point that might start one:

```json
{ "defaults": { "prices": "~/.config/rhei/prices.json" } }
```

```bash
rhei roster            # prices: "~/.config/rhei/prices.json" [global]
rhei run WORKSPACE     # prices from that book; no flag needed
```

Write it in `~/.config/rhei/settings.json` for the machine or
`.agent-grounds/rhei/settings.json` for one project, where it replaces the
machine's. `rhei run --prices <PATH>` still wins over both. A selected book is
the run's whole rate table, so an entry the book omits is unpriced rather than
falling back to the built-in one. See
[§FS-rhei-cost-accounting.5.1](functional-spec/rhei-cost-accounting.spec.md#51-price-book-selection).

A book written before the models a machine now targets prices none of them, and
every invocation is charged the $20.00 spend reserve instead. `rhei validate`,
and `rhei run` before its first reserve, name the pairs the book has no exact
entry for, once per book
([§FS-rhei-validate.4.6](functional-spec/rhei-validate.spec.md#46-a-targeted-model-the-price-book-cannot-price)):

```text
warning: price book foundation-site-2026-09-02 (/work/prices.json) has no entry for anthropic/claude-opus-5-5, anthropic/claude-sonnet-5-5: their invocations will be measured but unpriced, and each is charged the spend reserve. Add an entry for each pair to price them.
```

A day spent that way halts with the same names beneath `estimated:`, and
`rhei budget show` repeats them from the journal
([§FS-rhei-budgets.8.1](functional-spec/rhei-budgets.spec.md#81-naming-what-was-unpriced)):

```text
project spend: $400.00 consumed + $0.00 outstanding / $400.00; $0.00 remaining (window (2026-10-26Z))
  estimated:   20 unpriced (charged at $20.00 each)
               anthropic/claude-opus-5-5 (10), anthropic/claude-sonnet-5-5 (10) have no entry in price book foundation-site-2026-09-02 (/work/prices.json)
```

Add an entry for each named pair to that book. It prices the invocations that
settle after it changes; charges already settled stay as they are, and the
window renews the day.

A Codex worker that a managed workspace's requirements moved to a weaker setting
exits `0` and writes nothing. The run names the setting where the missing
outputs are named: a warning on the console when the worker's slot is released,
and a clause after the missing outputs in the ticket's Attention row
([§FS-rhei-configuration-fallbacks](functional-spec/rhei-configuration-fallbacks.spec.md#fs-rhei-configuration-fallbacks-codex-configuration-fallbacks)):

```text
  warning: Codex configuration fallback for task plan.1 in state 'review': Configured value for `permission_profile` is disallowed by requirements; falling back …
| plan.1 | review | worker exited 0 without overview (runtime/overview.md); Codex configuration fallback: Configured value for `permission_profile` is disallowed by requirements; falling back … | write the file(s) above and rerun, or record the outcome with `rhei transition …` |
```

Rerunning as is falls back again. Select a mode the account allows, such as
`codex[workspace]`, which writes under the roots Rhei grants at launch
([§FS-rhei-agents.2.2](functional-spec/rhei-agents.spec.md#22-modes)).

Render a self-contained HTML Flow visualization and open it in the browser:

```bash
cargo run -p rhei-cli -- viz examples/release-automation.rhei.md --open
```

`rhei viz <plan|workspace>` writes a single offline HTML page — the plan, every
task and subtask with its state, the resolved state machine, and the surroundings
inspector — under the workspace's `runtime/` directory (`runtime/<input>.html`,
or `runtime/rhei-viz.html` for a workspace directory), the same place a live run
freezes its final dashboard, or to `--output <FILE>`. Writing under `runtime/`
keeps generated HTML out of the source tree. It is the same Flow surface
`rhei run` serves live, frozen to a file; the live agent terminal and intervene
composer are inert in the static page. See
[`docs/functional-spec/rhei-viz.spec.md`](functional-spec/rhei-viz.spec.md).

Message a running agent during a live run (the headless sibling of the Flow
dashboard's intervene composer):

```bash
cargo run -p rhei-cli -- intervene --plan examples/release-automation.rhei.md \
  --task 3 -m "focus the review on error handling"
```

`rhei intervene` discovers the live run's dashboard from `runtime/dashboard.json`
and writes the message to the target agent's stdin — the same `/intervene`
channel the dashboard composer uses, never a plan transition. It only reaches
agents whose profile keeps stdin open (`intervene_stdin`); see [Enabling live
intervention](functional-spec/rhei-agents.spec.md#112-agents). Every
delivery is recorded to `runtime/interventions.log`.

Claim the next ready task and inspect its instructions:

```bash
cargo run -p rhei-cli -- next examples/release-automation.rhei.md
```

Complete a task and record the result:

```bash
cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 --result "Brief approved"
```

For a multiline or Markdown-heavy result, read a named file or standard input
instead of placing its contents in a shell-expanded argument:

```bash
cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 \
  --result-file result.md

cargo run -p rhei-cli -- complete examples/release-automation.rhei.md --task 1 \
  --result-file - <<'EOF'
Replayed onto `origin/main`.
Kept the result text verbatim.
EOF
```

Print crate versions surfaced by the CLI:

```bash
cargo run -p rhei-cli -- version
```

Reset a plan back to the initial state declared in its state machine:

```bash
cargo run -p rhei-cli -- --state-machine docs/functional-spec/states.yaml reset examples/release-automation.rhei.md
```
