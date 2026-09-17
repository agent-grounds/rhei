# RM-rhei-roadmap: Roadmap

This roadmap is sequenced against the project outcomes. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## Release Checklist

The release process is automated through GitHub Actions. The workflow verifies
the requested version, builds multi-platform PGO binaries, publishes crates.io
packages in dependency order when requested, and creates or updates the GitHub
release from `docs/changelog.md`. [§FS-rhei-distribution](rhei-distribution.spec.md#fs-rhei-distribution-rhei-distribution-and-release-process) [§AR-ci-release](../architecture/ci-release.spec.md#ar-ci-release-ci-and-release-automation-mirror-local-gates)

### Preflight

Run from the repository root before preparing a release:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked --no-fail-fast
```

Confirm the CLI reports the intended release version:

```bash
cargo run -p rhei-cli -- version
```

Run the manual pre-release workflow to check registry names and build a Linux
PGO release binary:

```bash
gh workflow run pre-release-checks.yml
```

### Version Preparation

Use `scripts/set-release-version.py <version>` to keep the workspace version,
internal crate dependency requirements, npm package versions, and PyPI package
versions aligned. `scripts/prepare_changelog_release.py prepare <version>`
promotes `docs/changelog.md` `Unreleased` into the release section.

Patch and minor release helpers perform those steps automatically after a green
`CI` run on `main`, dry-run the release workflow from a candidate branch, then
fast-forward `main` and dispatch publishing.

### Publishing

Manual publishing is done from the `Release` workflow:

```bash
gh workflow run release.yml \
  -f version=0.1.0 \
  -f publish_crates=true \
  -f create_github_release=true
```

The workflow creates or reuses `vX.Y.Z`, builds release artifacts for Linux
GNU x86_64/aarch64, macOS x86_64/aarch64, and Windows x86_64/aarch64, publishes
the Rust crates, and uploads checksummed binaries to the GitHub release.

### Package Wrappers

The npm and PyPI package wrappers remain source-built wrappers around the
matching `rhei-cli` crate version. Their checked-in version metadata is kept in
sync by `scripts/set-release-version.py`; publishing those wrapper packages
should happen only after the matching `rhei-cli` crate version is available.

## Completed: CLI Next No-Claim Diagnostics

Status: completed. `rhei next` now distinguishes completed plans, human-gated
tasks, claimed in-flight tasks, mid-workflow tasks that need an explicit
transition, and prerequisite-blocked tasks. Mid-workflow diagnostics include
copy-pasteable `rhei transition` commands for each outgoing transition, while
blocked-prerequisite diagnostics name the first unfinished prior and its state. [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next) [§FS-rhei-transition-cmd](rhei-transition-cmd.spec.md#fs-rhei-transition-cmd-rhei-transition)

## Completed: CLI Parse Error Accumulation

Status: completed. `rhei validate` now accumulates recoverable parse errors for
single-file plans and Directory Workspace task files so authors can fix a batch
of markdown mistakes without repeated parse/repair cycles. [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate)

## Completed: Panta Default Execution Model

Status: completed. Every load path yields a Panta-rooted graph: a bare rhei is
the single rhei of an implicit Panta with its id derived from the source
location, mutation is project-wide with rewrites routed to each owning rhei, and
`--rhei` narrows project-scoped invocations.

Originally delivered with one deliberate limit — one state machine governed a
whole project — since lifted: the machine is per-rhei, defaulted by the
manifest ([§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest)). [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis) [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture)

## Planned: CLI UX and Release Polish

Status: planned. This section is the canonical home for useful follow-up work
from the April 2026 PM review and the product-management pre-release pass. The
old notes are historical; this roadmap owns the remaining backlog.

- ~~Resolve per-rhei state machines during execution so heterogeneous rheis can
  run under their own machines, with each cross-rhei prior judged against the
  prior's own machine.~~ Done: the machine is a per-rhei property defaulted by
  the manifest, cross-rhei priors are judged under the prior's own machine, and
  templates with distinct machines coexist in one project.
  [§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest) [§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding) [§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)
- ~~Add a `rhei new` command that creates a rhei under Panta without a location
  argument.~~ Done: `rhei new "<title>"` writes the rhei, and
  `rhei new "<title>" --under <rhei|ticket>` writes a ticket with every plan
  field available as a flag. [§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new) [§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)
- Add rhei-level presentation to listing and monitoring: group tickets under
  rhei headings with a per-rhei status rollup, and render the `basin` rhei
  de-emphasized (dimmed or collapsed) while keeping its last-place ordering.
  Today `rhei list` prints a flat qualified-id listing with basin's tickets
  last. [§FS-rhei-panta.3](rhei-panta.spec.md#3-one-unified-view) [§FS-rhei-panta.4](rhei-panta.spec.md#4-invisibility) [§FS-rhei-list.4.1](rhei-list.spec.md#41-text-default)
- Materialize rhei nodes in the merged graph so `node_policy.rhei` can bind a
  profile to the rhei tier and let a profiled rhei carry state and roll up like
  a non-leaf ticket. Today the key has no effect because the graph contains no
  rhei nodes. [§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup) [§FS-rhei-states.9](rhei-states.spec.md#9-node-policy)
- Give `rhei viz` rhei-level *presentation*: visually grouped top-level bands
  per rhei with the `basin` group last and de-emphasized. The merged project
  graph and its cross-rhei edges already render; what is missing is the grouping
  chrome around them. [§FS-rhei-viz.7.3](rhei-viz.spec.md#73-panta-projects-render-as-one-graph) [§FS-rhei-panta.6.4](rhei-panta.spec.md#64-reset-validate-list-viz)
- Validate child-rhei content-section links under Panta: carry a per-section
  link base so a rhei's own content sections resolve against that rhei's
  execution root, not the project root. Today only task-content links are
  checked per rhei; rhei-level content sections are dropped at merge. [§FS-rhei-plan-language.3.6](rhei-plan-language.spec.md#36-link-integrity) [§AR-rhei-panta.5](../architecture/rhei-panta.spec.md#5-execution-root-and-per-rhei-runtime)
- Make failed `rhei complete` attempts from loop states explain the exact
  blocked transition condition and the currently available next transitions. [§FS-rhei-complete](rhei-complete.spec.md#fs-rhei-complete-rhei-complete) [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification)
- Decide and normalize `rhei transition` result-file behavior: either stop
  writing result files for bare transitions or link/audit them consistently
  with `rhei complete`. [§FS-rhei-transition-cmd](rhei-transition-cmd.spec.md#fs-rhei-transition-cmd-rhei-transition) [§FS-rhei-complete](rhei-complete.spec.md#fs-rhei-complete-rhei-complete)
- Improve template discovery and preflight output: list searched paths when no
  templates are found, surface reusable values-file scaffolds in template
  READMEs, and make nested `--list-inputs` defaults copyable. [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
- Resolve `type: path` input semantics: keep the current existence check for
  user-supplied paths, decide whether defaults should be checked, and decide
  whether an explicit `--allow-missing-paths` escape hatch belongs in the CLI. [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
- Extend JSON error output beyond the current `{ "error": { "message": ... } }`
  envelope with a stable `kind` and optional `path` taxonomy before downstream
  integrations depend on it. [§FS-rhei-render](rhei-render.spec.md#fs-rhei-render-rhei-render) [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next)
- Clean up small human-output ambiguities: show agent and model as distinct
  fields, reword built-in validation source labels, clarify live template
  variables versus prose in state instructions, and decide whether rendered
  JSON should keep or flatten `metadata.metadata`. [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next) [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) [§FS-rhei-render](rhei-render.spec.md#fs-rhei-render-rhei-render)

## Planned: A Dry Run That Prints the Composed Prompt

Status: implemented on this branch, one item open. A `rhei run` agent used to
see its own task, its children, its direct priors, and its exports — not its
ancestors, not the rhei's content sections, not what finished elsewhere, and
not what already happened to the task it is retrying. [§FS-rhei-memory](rhei-memory.spec.md#fs-rhei-memory-mid-term-memory) now
specifies four prompt sections that reconstitute that memory by a fixed
algorithm, bounded in tokens, and requires that every terminal task in the
Panta stays reachable from any invocation. [§FS-rhei-memory](rhei-memory.spec.md#fs-rhei-memory-mid-term-memory) [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Landed: `## Position`, `## Plan History`, `## Previous Visits`, and the
`Reading the rhei` / `Leaving a trail` sub-sections render in `rhei run` and
`rhei next` per [§FS-rhei-memory.3](rhei-memory.spec.md#3-the-sections) and [§FS-rhei-memory.4](rhei-memory.spec.md#4-composition-algorithm), and
`## Prior Task Results`, `## Consumed Exports`, and the handoff sections adopt
the fencing rule of [§FS-rhei-memory.4.5](rhei-memory.spec.md#45-fencing-and-rendering), closing the supervision follow-up.

- Add a dry-run form that prints the full composed prompt without claiming
  a ticket. `rhei run --dry-run` keeps its `<prompt...>` placeholder, so
  claim-mode `rhei next` is the only way to see a composed prompt today.
  [§FS-rhei-memory.5](rhei-memory.spec.md#5-surfaces)

## Planned: Relocatable Rhei Root and Cross-Worktree Coordination

Status: planned. Today the coordination signal — a task's claim, `**State:**`,
and the transition that rewrites it — lives inside the working-tree task file,
so parallel agents in separate git worktrees can only observe each other's
progress by committing the rhei. This blocks the common multi-agent setup where
users want live coordination but do not want plan state in git history. The work
below decouples the rhei root from the working tree so worktrees on one machine
share live state through the filesystem rather than through commits. [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis) [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture) [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

- Make the rhei root relocatable with an explicit resolution order: `--rhei-root`
  flag, then `RHEI_ROOT` environment variable, then the shared git common
  directory (`$(git rev-parse --git-common-dir)/rhei/`) when inside a repository,
  then the in-tree default. The common-dir default lets every linked worktree of
  one repository read and write the same runtime state without that state ever
  entering a commit. [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture) [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)
- Split the authored plan from runtime state: keep task bodies, `**Prior:**`, and
  descriptions as the versioned "what," and move claims, current state, and the
  event log into a side store keyed by node id under the relocatable root. This
  lets users commit the plan while keeping live status uncommitted, instead of
  forcing both into the same file. [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture)
- Confirm the concurrency primitive on a shared store: Directory Workspace
  per-task sharding plus `rhei transition` CAS already make parallel claims safe
  on a shared local filesystem; document that contract and add a lock check where
  filesystem atomicity weakens, such as NFS. [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis)
- Specify the single-machine versus cross-machine boundary explicitly:
  same-machine worktrees coordinate through the shared filesystem with no
  commits, while coordination across machines without committing requires an
  out-of-band transport — a shared filesystem or a small sync server — the same
  constraint every on-disk tracker faces. [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis) [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## Planned: Detached Run Follow-Ups

Status: planned. Detached runs ship with `--headless`, `rhei attach`,
`rhei stop`, `rhei runs`, and the `--json` event stream
([§FS-rhei-run-headless](rhei-run-headless.spec.md#fs-rhei-run-headless-detached-runs) [§FS-rhei-run-json](rhei-run-json.spec.md#fs-rhei-run-json-rhei-run---json)). These items extend that base
without changing the execution model.

- Support `--headless` on Windows. The primitives exist (`DETACHED_PROCESS`,
  `CREATE_NEW_PROCESS_GROUP`), but `rhei stop` inherits a signal contract with
  no Windows equivalent, so this means designing that teardown rather than
  translating the Unix one. [§FS-rhei-run-headless.1.3](rhei-run-headless.spec.md#13-platform) [§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)
- Give the attached surface a streaming event transport when the loopback
  server grows per-connection handling. Attachment follows
  `runtime/events.jsonl` by polling today, which is robust and works without a
  control server but adds latency. [§DA-detached-runs](../decisions/architectural/detached-runs.md#da-detached-runs-a-detached-run-is-a-supervisor-process-not-a-daemon-service)
- Extend the JSON error envelope with the stable `kind`/`path` taxonomy the CLI
  UX section already tracks, so `--json` consumers can branch on failure class
  instead of matching message text. [§FS-rhei-run-json.1](rhei-run-json.spec.md#1-selecting-the-format) [§FS-rhei-errors](rhei-errors.spec.md#fs-rhei-errors-cli-errors-and-guidance)
- Add a run-scoped `rhei attach --replay` over a finished run's event log, so a
  run can be inspected in the TUI after the fact rather than only through its
  report and frozen dashboard. [§FS-rhei-run-headless.5.2](rhei-run-headless.spec.md#52-attaching-to-a-run-that-has-ended)

## Planned: Dashboard and Monitoring Follow-Ups

Status: planned. The first dashboard visualization pass is complete; these
items improve operator diagnosis without changing the execution model.

- Add richer readiness reasons in the dashboard for missing input artifacts and
  human gates. The current dashboard explains unfinished `Prior:` blockers but
  intentionally leaves non-prior causes generic. [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal) [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)
- Add task-opening affordances, state/level filtering or dimming, a dependency
  graph view, and diff visualization against another snapshot or git ref. [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)

## Planned: Subtree Supervision Follow-Ups

Status: shipped, with two follow-ups. A non-leaf task can look after its
subtree while it runs instead of only integrating it at the end: a state
declaring `execute_on: <scope>-<event>` wakes the task at every finished child,
every child transition, every finished descendant, or every descendant
transition, and holds the subtree in between. The
`execute_on` field, the hold/release readiness rule, the `supervision` task
metadata, the `openDescendants` condition operand, and the prompt sections all
ship. So does the reason where a surface has somewhere to put it: `rhei next`
names the supervisor holding a ticket and the command that claims it, the run
report says `held by supervisor Task <P> (<state>)` on the ticket it halted on,
and `rhei list --ready` excludes a held descendant by the ready set's own rule.
[§FS-rhei-supervision](rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification) [§DF-subtree-supervision](../decisions/functional/subtree-supervision.md#df-subtree-supervision-a-supervisor-is-a-barrier-over-its-subtree-woken-after-the-fact)

- Carry the reason into the plain `rhei list` listing, the TUI, and the Flow
  dashboard. None of the three has a readiness-reason concept of its own yet —
  `rhei list` shows no reason for any ticket, held or blocked — so this is a
  column that has to be designed once for every cause rather than added for
  supervision alone. [§FS-rhei-list](rhei-list.spec.md#fs-rhei-list-rhei-list) [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)
- Context continuity for `claude-code` supervisors depends on the snapshot
  adapter work below; until it lands, a supervisor with that profile runs each
  visit cold, carried by its checkpoints and briefs. [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)
- Fanout on a supervising state is a v1 validation error; lift it only with a
  rule for what a fanned-out supervisor's continued session means.
  [§FS-rhei-supervision.1.2](rhei-supervision.spec.md#12-validation-rules)
- **Machine-readable supervision.** Three surfaces carry supervision as prose a
  script has to parse: the `events.jsonl` stream has the held reason only inside
  a free-text message, `rhei next --json` renders `checkpoints` as one Markdown
  blob rather than a list of `{task, from, to, visit}` objects, and
  `rhei render --json` spells `checkpoints[].task` with the rhei-local id while
  everything beside it is project-qualified. Design the three together — one
  shape for a checkpoint, one field for a held reason — rather than one at a
  time. [§FS-rhei-supervision.3.3](rhei-supervision.spec.md#33-supervision-metadata) [§FS-rhei-run-json](rhei-run-json.spec.md#fs-rhei-run-json-rhei-run---json) [§FS-rhei-next.4](rhei-next.spec.md#4-peek-mode---peek)
- **Brief provenance.** Briefs are ordinary files the engine never clears, so a
  brief left over from visit 1 is rendered as-is on visit 7 and reads as fresh
  direction. Stamp what wrote it — supervisor id, visit, mtime — where the
  section is rendered, so a stale brief is visible as stale.
  [§FS-rhei-supervision.5.2](rhei-supervision.spec.md#52-the-brief)
- **A resumed run can hand a supervisor a checkpoint for work it killed.** When
  a run is interrupted mid-step, the artifacts the killed worker had already
  written can satisfy the next run's completion condition, so the ticket
  advances without the work being redone and the supervisor is checkpointed on a
  step nobody finished. This is generic `rhei run` resume behaviour rather than
  anything supervision added — it is only *visible* through supervision, because
  a supervisor is woken by it. The fix belongs with resume: decide what evidence
  proves a step ran, not merely that its outputs exist.
  [§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership) [§FS-rhei-supervision.6](rhei-supervision.spec.md#6-interaction-with-other-features)
- **Fence the older pasted sections.** `## Checkpoints` and
  `## Child Task Results` fence what they paste so a pasted `## Result` heading
  cannot outrank the section it sits under ([§FS-rhei-supervision.5.1](rhei-supervision.spec.md#51-the-supervisors-prompt));
  `## Prior Task Results`, `## Consumed Exports`, and the handoff sections
  predate that and still paste raw. Change them together, with one rule for
  every pasted body. [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)

## Planned: Block Library Follow-Ups

Status: planned, deferred past 0.5.0 with no release assigned. The first
increment of composable blocks ships in PR #282 for #47: `rhei instantiate`
mounts blocks under aliases, links them with completion-only seams and direct
data passes, and lowers the result to one ordinary workspace and one flat
state machine ([§FS-rhei-library](rhei-library.spec.md#fs-rhei-library-composable-blocks)). The capabilities below were
deliberately left outside that contract ([§FS-rhei-library.8](rhei-library.spec.md#8-diagnostics-and-deferred-surface)) and are
tracked as tool-report intake tickets, whose lifecycle publishes each upstream
issue; the ids here are the intake ids.

- Catalog and discovery for blocks beyond the template lookup composition
  reuses today: a way to list mountable blocks and read their ports before
  mounting one. Tracked as rhei.83.
- A typed, total textual composition surface, evaluated against agent fluency
  and predictability; composition is wired by `--mount`, `--seam`, `--pass`
  and `--set` or a block's `use` today, and #47 chose no language. Tracked as
  rhei.84.
- Per-node provenance and richer lock metadata; generated output carries root
  and block headers only. Tracked as rhei.85.
- Additional exposure modes past the declared ports and inputs of a mount,
  keeping encapsulation the default. Tracked as rhei.86.
- Conditional or gated seams; a seam consumes exactly one terminal exit
  today, and internal transitions stay untouched. Tracked as rhei.87.
- Expressions and conversions on a pass; a pass wires two declared endpoints
  of the same kind directly today. Tracked as rhei.88.

## Planned: Snapshot Adapter and Retention Work

Status: planned. Snapshot v1 intentionally ships a conservative built-in
support boundary; Pi is supported, while other built-in agents require adapter
spikes before Rhei can safely capture and resume their native sessions.

- Resolve built-in adapter spikes for Claude Code, Codex, and Gemini session
  capture/resume surfaces, then update the built-in profile table and runtime
  support boundary. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification) [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)
- Finalize provider cache TTL defaults in shipped settings and keep the
  snapshot specs pointing at that single source of truth. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification)
- Decide whether `snapshot.emit.on: timeout` should be distinct from
  `failure`, whether terminal-task automatic GC should replace TTL-based GC in
  v2, and whether sensitive states need a per-state auto-emit opt-out. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification) [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)
- Add snapshot summarizer helpers, richer retention automation, and redaction
  audit support in a future manifest schema without turning snapshots into
  cross-agent transcript replay. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification) [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)

## Completed: Post-Alpha Snapshot Continuation

Status: completed. Interactive `rhei snapshot continue` drops an operator into
a preloaded agent session and, unless `--no-capture` is passed, captures the
resulting transcript as an operator generation without advancing the snapshot
`current` pointer or mutating plan state. The built-in Pi profile provides the
v1 built-in interactive continuation surface; built-in agents without a proven
Rhei-readable session capture layout fail clearly with
`unsupported-snapshot-session` and can still be replaced by custom
session-capable profiles. [§FS-rhei-snapshot-operations](rhei-snapshot-operations.spec.md#fs-rhei-snapshot-operations-rhei-snapshot-operations-specification) [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification)

## Completed: Post-Alpha Dashboard Visualization

Status: completed. The browser dashboard that accompanies `rhei run` includes
Gantt, heatmap cube, and Sankey plan views ahead of the operational Tasks,
Slots, Journal, and Links tabs. The dashboard remains the live execution
monitor for slots, task state, journal events, and links while also providing
static plan-shape views without switching tools. [§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) [§FS-rhei-run-tui](rhei-run-tui.spec.md#fs-rhei-run-tui-rhei-run-tui-and-run-event-journal)

The TUI surfaces the dashboard as a power-user view when `rhei run` selects the
TUI frontend, while `--dashboard` and `--no-dashboard` remain explicit
overrides in the CLI and completion surface. [§FS-rhei-completions](rhei-completions.spec.md#fs-rhei-completions-rhei-completion-ux-specification) [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run)
