# AR-source-file-size: Source File Size Architecture

Rhei source files are part of the working memory shared by humans and agents.
They must stay small enough for focused review, reliable agent context loading,
and predictable refactoring. This architecture rule supports readable,
reviewable plans and predictable execution work. [§GOAL-rhei-outcomes](../functional-spec/goals.md#goal-rhei-outcomes-goals)

## 1. File Size Policy

Hand-authored source, template, example, and test files must be kept in the
500-line range.

- A file at or below 500 lines needs no special justification.
- A file above 500 lines and at or below 2000 lines is a large-file exception.
  It must be listed in a large-file register with its path, reason the size is
  necessary, owner or owning area, and the condition that should trigger
  splitting it. The register must not record exact line counts because they
  become stale quickly.
- A file above 2000 lines is not allowed. It must be split before the work that
  creates or expands it is considered architecturally complete.

Generated files, vendored third-party files, lockfiles, and external fixtures
may be excluded only when they are clearly marked as generated or third-party
and are not edited by hand. If a generated file becomes a regular hand-edited
maintenance surface, this policy applies to it.

Documents are budgeted too, by how they are read rather than by the numbers
source takes.

A citable specification — a grund-declared document, `.spec.md` or otherwise,
that a citation reaches by section — is fetched one section at a time through
`grund <ID>.<section>` and is never loaded as one undifferentiated file, so its
length is not charged to every read the way a source file's is. That earns it a
larger budget, not an exemption from measurement: 750 lines soft, 2000 hard.
750 is the value grund itself uses and about three times the p95 of the
foundation spec trees, so the warning arrives when a document has taken on a
second subject rather than when it is merely thorough. 2000 is the same ceiling
this section sets for any hand-authored file; for a document it is the backstop
for one that has stopped being a document. A split lifts whole sections into a
child declaration and leaves every citation resolvable — a citation that stops
resolving costs later readers more than the lines it saved.

An entrypoint is the opposite case. `README.md`, `AGENTS.md`, `CLAUDE.md`, and
the files under a `skills/` directory are loaded whole into every session that
starts from them, so every line is charged to every read. They are budgeted at
250 soft and 500 hard, and the way under that limit is to move a section into a
document a citation can reach, leaving the orientation and the link behind.

A changelog is an append-only record of releases that have already happened. It
is not line-measured at all: there is no split to ask for, and a finding against
it would name no edit anyone could make.

## 2. Large-File Register

The register is `fissile`'s exception registries, not a table maintained by
hand: `docs/file-size-agent-exceptions.toml` for entries that leave a soft
finding standing and `docs/file-size-human-exceptions.toml` for entries that
clear the hard gate. `.agent-grounds/fissile.toml` encodes §1 — 500 soft and
2000 hard for source, 750 and 2000 for a citable spec, 250 and 500 for an
entrypoint — so the rule is now checked at commit time rather than stated and
hoped for.
The gate itself is one `fissile check --staged` hook in
`.pre-commit-config.yaml`, next to `grund check`: it reads the staged set, so it
answers for the files a commit actually touches and stays silent about the
backlog it did not create.

Each entry still carries what §1 asks of it: the path, the reason the size is
necessary, and the condition that should trigger splitting it. `fissile` spells
those as `path`, `reason`, and `until`, and adds `kind`, which fixes what the
reason has to establish — `structural` names the constraint that makes splitting
illegal and never expires, `deferred` names the boundary that is missing and
what has to exist before the split can happen. Restating the file's contents is
not a reason in either case.

A registry entry does record a number, `max_accepted`, which this section
previously ruled out on the grounds that exact line counts go stale. The
objection holds against an exact count and is answered by not writing one:
`fissile` quantizes every ceiling it writes up to the `[exceptions.bump]` step,
so the recorded value is a decision — *this file may run to 800 lines* — rather
than a reading taken on the day the entry was written, and an edit inside the
step does not touch the registry. `fissile audit --stale-exceptions` reports the
two ways an entry can stop being true: it accepts a file that no longer exists,
or it stands more than one step above the file it accepts. `fissile exception
retune` moves the number when only the number is wrong, leaving the reason
alone.

Entries are added with `fissile exception add`, never by hand. A hard entry is a
human's to add: it is the only thing that clears a gate §1 calls not allowed,
and an agent that could write one could waive the rule it is being held to.

## 3. Split Shape

When a file crosses the hard limit, split it along existing behavioral
boundaries first. The split should preserve public behavior and make the next
split obvious. Do not create arbitrary numeric chunks unless the file is being
split mechanically as a temporary containment step; those chunks must still be
named after the behavior they contain.

`crates/rhei-cli/src/main.rs` is only the CLI shell. It includes focused parts
under `crates/rhei-cli/src/cli/`:

- `path_guards` contains the one platform-aware path question — does a
  declared path start at a root — that the validator, the prompt renderer, and
  the TUI inspector all ask.
- `cli_declarations` and `cli_dispatch` contain clap command declarations and
  top-level dispatch.
- `completion_candidates` and `completion_context` contain shell completion
  and completion-context helpers.
- `templates_list`, `templates_instantiate`, `templates_discovery`, and
  `templates_inputs` contain template listing, instantiation, discovery,
  validation, input parsing, rendering, and materialization.
- `states_render`, `metadata_conditions`, `metadata_rewrite`,
  `transition_context`, `artifacts`, `system_transition_triggers`, and
  `system_transition_execution` contain state-machine inspection, plan
  metadata, artifact contracts, and transition application.
- `run_options`, `settings_types`, `settings_load_validate`,
  `tooling_resolution`, `agent_resolution`, `agent_model_resolution`,
  `agent_command`, `agent_spawn`, and `programs` contain run configuration,
  settings merge/validation, tooling resolution, agent command construction,
  agent spawning, and program-state execution.
- `snapshot_pointer`, `snapshot_records`, `snapshot_list_show`,
  `snapshot_refs_gc`, `snapshot_continue_lock`, `snapshot_runtime_emit`, and
  `snapshot_runtime_preload` contain the `current`-pointer reader and writers
  every generation shares, snapshot CLI/cache handling, and run-loop snapshot
  emit/preload hooks.
- `run_command`, `run_git_consistency`, `run_agent_mode`, `run_work_items`,
  `run_program_sequential`, `run_agent_sequential`,
  `run_agent_sequential_completion`, `run_parallel_spawn`,
  `run_parallel_schedule`, `run_agent_pool`, `run_parallel_agent_exit`,
  `run_parallel_program_completion`, `run_callback_mode`,
  `run_failure_transitions`, and `ready_transition` contain orchestration,
  durable-state consistency checks, ready-set collection, sequential agent and
  program execution, parallel spawning and scheduling, per-invocation result
  handling, failure routing, and automatic transition selection.
- `next_command`, `complete_reset_commands`, `complete_reset_rewrites`,
  `file_locks`, `render_install_commands`, `install_skill_agents`, and
  `diagnostics` contain the remaining command families, the cross-platform
  file-lock helpers every locking command shares, and shared diagnostics.
- `tests_cli_render`, `tests_complete_reset_tooling`, `tests_file_locks`,
  `tests_agent_resolution`, `tests_agent_execution_validation`,
  `tests_settings_tooling`, `tests_path_guards`, `tests_snapshots_gc`,
  `tests_snapshot_pointer`, and `tests_snapshot_runtime` contain CLI unit tests
  split by nearby behavior. Add new unit tests next to the part that owns the
  behavior.

`crates/rhei-validator/src/lib.rs` is only the validator shell. It includes
focused parts under `crates/rhei-validator/src/validator/`:

- `preamble` contains public imports, report types, errors, agent/profile
  schema primitives, and target parsing.
- `state_defs` contains state, snapshot, profile, node-policy, and state
  machine declarations.
- `state_machine_impl` contains `StateMachine` loading, core accessors, and
  model/target validation.
- `state_machine_prompt_templates` contains reusable prompt template loading,
  placeholder substitution, effective-prompt composition, and prompt-template
  validation.
- `state_machine_snapshots` contains snapshot emit/inherit validation.
- `state_machine_runtime_validation` contains program, poll, and tooling
  validation.
- `state_machine_profiles` contains profile/node-policy validation, schema
  version interpretation, and template-condition validation.
- `validation_helpers` contains shared semantic validators and parsing helpers.
- `validator_entry` contains public validation entrypoints, plan traversal,
  state/profile checks, dependency integrity, and terminal-tree coherence.
- `validator_links` contains Markdown link extraction and file-reference
  validation.
- `tests_state_machine`, `tests_plan_validation`, `tests_links_tooling`,
  `tests_profiles`, `tests_poll`, and `tests_snapshots` contain validator unit
  tests split by validation topic.

`crates/rhei-core/src/parser.rs` is only the parser API shell and shared
frontmatter helpers. Parser implementation parts live under
`crates/rhei-core/src/parser/`:

- `builder` contains node-stack assembly and node finalization.
- `plan` contains the main Markdown plan parser.
- `recovery` contains best-effort multi-error parsing.
- `workspace` contains directory workspace index and task-file parsing.
- `plan_tests` and `workspace_tests` keep parser tests beside the behavior
  they exercise.

`crates/rhei-tui/src/dashboard.rs` is only the dashboard sink and HTTP
request shell. Dashboard parts live under `crates/rhei-tui/src/dashboard/`:

- `state` contains event reduction and serializable dashboard payload types.
- `html` contains the embedded browser UI.
- `tests` contains dashboard state and URL-encoding tests.

`crates/rhei-tui/src/tui.rs` is only the terminal lifecycle, channel, and
input loop shell. Terminal UI parts live under `crates/rhei-tui/src/tui/`:

- `state` contains event reduction, lookup/readiness rules, and keyboard-visible
  UI state.
- `derive` contains pure rollups and navigation chips shared by input and views.
- `input` contains keyboard handling and live action composers.
- `render` contains the shared ratatui frame, chrome, and overlays.
- `views` contains the Flow, Machine, Cost, Journal, Tasks, and minimal body
  renderers.
- `theme` contains state category glyph/color mapping.
- `text` contains stream labels, truncation, and terminal-text sanitization.
- `tests` contains terminal input, state, rendering-line, and text tests.

`crates/rhei-output/src/lib.rs` is only the output crate API shell. Renderer
parts live beside it:

- `json` contains JSON conversion.
- `github` contains GitHub-oriented Markdown rendering.
- `progress` contains terminal progress report rendering.
- `common` contains shared task-label formatting helpers.
- `tests` contains renderer tests split out of the public API shell.

`tests/integration/integration_markdown_plans.rs` is only the integration
test shell. Shared fixture helpers live in
`tests/integration/integration_markdown_plans/common.rs`; behavior groups
live in sibling files named for their command or behavior area:
`validation_cli_basics`, `validation_parse_errors`, `transitions_success`,
`transitions_failures_completion`, `callbacks_execution`,
`callbacks_redirect_context`, `run_basic`, `run_programs_callbacks`, `reset`,
`workspace_validation`, and `workspace_execution`.

Future work must keep new code inside the owning part file or create a new
part with a behavior name. If adding code would push a part past the 500-line
range, split that part before adding more behavior.

## 4. Current Violations

No hand-authored repository file is currently known to be above the hard
2000-line limit. New work must not introduce one.

| Path | Required Direction |
|---|---|
| _None._ | |
