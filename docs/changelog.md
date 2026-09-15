# Changelog

## Unreleased

### Added

- **Tasks can declare explicit read exclusions for blind work.** `**Excludes:**`
  accepts checkout paths, runtime files or directories, and declared exports;
  validation rejects malformed, escaping, duplicate, unresolved, or
  required-input conflicts. Rhei filters matching payloads from serial,
  parallel, retry, fan-out, and `rhei next` context while retaining navigation.
  Built-in profiles are composition-only; custom profiles may declare a
  `deny_read.path_flag` adapter for process-tree enforcement. (PR #274)

- **Per-session reports and declared metrics are now specified.**
  §FS-rhei-session-reports defines one readable Markdown report rendered from
  each agent session log under `runtime/reports/`, and §FS-rhei-metrics defines
  the optional `metrics:` declaration in `states.yaml` with engine-recorded
  iteration bindings under `runtime/metrics/`. Documentation only; no runtime
  behavior changes in this entry. (PR #272)

- **Plans that declare `Consumes` now receive a non-fatal validation and run
  advisory that the field selects export prompt injection, not filesystem
  visibility.** Run frontends retain the warning in their native output, and
  authoring guidance now explains that workers can still read undeclared
  sibling exports. Scripts that consume successful validation or run output
  for such plans must allow the additive warning. (PR #269)

- **`rhei complete` now accepts result messages from a file or standard
  input.** `--result-file <PATH>` safely carries multiline or Markdown-heavy
  text, with `-` selecting stdin, while the existing inline `--result` form
  and its literal dash behavior remain unchanged. (PR #270)

- **The new `rhei roster [RHEI_PLAN] [--json]` command prints the effective
  agent, model, and defaults registry.** Its machine-readable form includes
  source metadata and per-merge-unit provenance, so dispatchers can use Rhei's
  built-ins and precedence rules without copying or reimplementing them. (PR #271)

- **`rhei run` now parks supported Codex/OpenAI session limits and resumes
  automatically.** A reset-bearing refusal keeps the task in its authored
  state, releases its worker slot, persists the safe retry deadline across
  interruption or restart, and leaves foreground/headless runs waiting while
  unrelated work continues. Journals and JSON streams expose the additive
  `provider_limited` outcome instead of reporting an ordinary agent failure.
  (PR #277)

- **States can select reasoning effort independently of their agent target,
  model, and permission mode.** Built-in and custom supporting profiles map the
  canonical effort to native arguments, while unsupported profiles ignore it;
  an existing authored `effort` key that was previously ignored now takes
  effect or fails validation when its value is invalid. (PR #276)

- **Unrestricted project runs now admit members instantiated while the run is
  live.** A validated member is published atomically, initialized with its own
  machine, callbacks, execution root, lock, and accounting context, then enters
  the existing sequential or parallel scheduler without starting another run.
  Publication refuses a destination created during staging. Runs narrowed with
  `--rhei` keep their startup candidate set fixed. (PR #275)

- **Transition callbacks now receive a stable identity for each firing and an
  explicit pending ledger status.** Canonical JSON exposes `firingId` and
  `ledgerStatus`, and CLI callbacks receive equal environment values, so an
  `on_enter` integration can include the in-progress transition without
  inferring it from file-write timing. Both ledger formats remain unchanged;
  strict callback JSON decoders must permit additive fields. (PR #262)

### Fixed

- **Resumed program polls now invoke their next subprocess attempt after the
  persisted retry deadline.** A future deadline no longer sends the run into
  callback-only mode, where an exit-zero route could fire without a matching
  program exit; ordinary readiness still prevents an early spawn. (PR #266)

- **Structured accounting extraction failures now explain why capture parsing
  failed.** Capture and invocation records preserve concise diagnostics, and
  `rhei cost --task` exposes them in both text and JSON without quoting raw
  agent output. (PR #267)

- **Plan commands now classify existing directories without a Rhei manifest.**
  The diagnostic identifies the path as neither a Panta Project nor a Directory
  Workspace, presents `index.panta.md` and `index.rhei.md` as alternatives, and
  points `rhei run DIRECTORY --rhei ID` at a recognized `DIRECTORY/ID` workspace
  instead of suggesting permissions or free-space checks. (PR #268)

## 2. [0.5.0] - 2026-09-14

### Fixed

- **Price books now accept and preserve the extension metadata their published
  v1 schema permits.** Additional document and entry properties retain their
  JSON values in every participating run-owned copy without affecting price
  selection, validation, coverage, or calculated amounts. (PR #257)

- **The diagnostic path no-wrap regression now follows the CLI's shorter-path
  selection contract.** The test previously required an absolute fixture path
  even when the CLI correctly rendered a shorter relative spelling intact. It
  now covers both relative-selected and absolute-selected paths beyond the
  renderer's wrap width, without depending on checkout or temporary-directory
  layout. (PR #259)

- **Template discovery now continues through every filesystem ancestor per
  template name.** A nearer empty templates directory or one containing only
  other names no longer hides an ancestor project template and exposes a user
  or built-in fallback; listings and completion aggregate the nearest copy of
  each name. This can make ancestor names newly visible and causes matching
  ancestor copies to shadow fallback copies. Pass an explicit template path to
  select one exact directory. (PR #258)

- **Retried agent attempts now keep distinct accounting identities and totals.**
  The shared spawn plan carries one run/move/attempt identity through streamed,
  final, and durable usage reporting, so sequential and parallel retries no
  longer collapse into one live row or block later runs. Existing visit-level
  records are retained as separate attempts when run ids or disjoint valid
  intervals distinguish them; exact copies still count once, while genuine
  conflicts now stop startup with an accounting-identity diagnostic. (PR #260)

- **The bundled authoring skills now resolve a built-in `rhei` project default
  the way the CLI does.** Their no-CLI guidance no longer lets a member-only
  `states.yaml` replace built-in `pending`/`completed` for an inheriting or
  restating member. It retains project-root and standalone lookup for `rhei`,
  and the unique member-root fallback for custom project defaults. Existing
  copied skill installations can pick up the correction by rerunning
  `rhei install-skills`. (PR #255)

- **Validation failures name every state-machine source they actually used.**
  A heterogeneous Panta project no longer attributes every invalid state to
  only the project default: the diagnostic lists each resolved source and the
  rheis it governs, while single-source failures keep their existing wording.
  (PR #256)

- **A member rhei can opt back into the built-in state machine when its project
  uses a custom default.** An explicit `**States:** rhei` now falls back to the
  built-in machine when the complete candidate-file lookup finds no matching
  definition, while a matching definition still wins and a missing custom
  machine remains an error. Undeclared siblings continue to inherit the project
  default. (PR #248)

- **A failed `rhei next` claim now leaves its task unchanged and retryable.**
  Auto-advancing a task previously wrote the target state and counted-visit
  metadata before it wrote the assignee or transition ledger, so a later I/O
  failure could strand the task advanced and unowned. Claiming now revalidates
  and resolves the effective state and owner under the task lock, preflights
  and serializes the ledger, and restores the original task, metadata, and
  ledger bytes if any pre-commit persistence step fails. State, ownership, and
  one ledger entry commit together; prompt-rendering failures after that point
  leave the durable claim intact. (PR #245)

- **`rhei new --dir` can adopt an authored state-machine workspace.** A
  same-id directory containing only `states.yaml` and optional
  `prompt_templates/` was called an existing rhei and refused, making the
  documented machine-first authoring order impossible without reversing it and
  using `--keep-on-error`. The explicit Directory Workspace form now adopts
  that bundle (or an empty directory), resolves the machine through ordinary
  post-write loading, and preserves every authored byte through success,
  rollback, and dry run. The default form remains a single-file create and now
  reports the same-id directory as a layout conflict; actual rheis remain
  collisions, while unrelated content is named as an occupied destination.
  (PR #249)

- **An explicit Panta member state-machine declaration now gives its matching
  local `states.yaml` precedence even when it repeats the project default.**
  Such a member now uses the local definition's states, transitions, profiles,
  callbacks, and artifact contracts; a newly consulted malformed local
  candidate now reports its load error. Omit the member declaration to inherit
  the resolved project machine wholesale. (PR #254)

- **The worker skill's capture instruction now says when it can be followed.**
  `rhei-plan-worker` is handed one plan and forbidden from looking for a project
  around it, so it routinely works a lone `.rhei.md`. *Capturing Work You Did
  Not Come For* prescribed `rhei new --under basin` there with no precondition,
  and the basin exists only inside a Panta project: the command refused and
  offered `rhei init`, which the same section reserves for a human, leaving an
  agent with no path for work it had just been told to capture. The section now
  states the precondition and says to report the capture in the result instead
  of creating a project, and `rhei new`'s specification states the precondition
  where the capture is prescribed, alongside the one its neighbour already had.
  No command behaviour changes. (PR #238)

- **`rhei transition --supervisor` now refuses a real task that is not the
  moving task's nearest in-scope supervising ancestor.** The invalid value used
  to be accepted and silently ignored, so the move succeeded and could wake the
  actual owner with a phantom checkpoint. It now fails before callbacks,
  artifacts, state, result, ledger, or checkpoint effects, and tells the caller
  which supervisor is valid or to omit the flag when there is none. Correct
  local and qualified values suppress the owner's checkpoint as documented.
  **Breaking:** scripts that supplied a different real task and relied on the
  transition succeeding must pass the actual owner or omit the flag. (PR #246)

- **Validation rejects an undeclared effective settings-default agent mode
  before execution.** `rhei validate` previously checked modes written on a
  state or target selector but accepted the same invalid selection from merged
  `defaults.agent_mode` or the legacy top-level `agent_mode`, leaving runtime
  resolution to fail later. Validation now applies the runtime precedence to
  static selections and names the effective agent, mode, declaration key, and
  settings locations. **Upgrading:** configurations that relied on this false
  successful validation now fail until the selected mode is corrected or
  declared; shadowed defaults and agents without declared modes are unchanged.
  (PR #247)

- **`rhei cost` and `rhei summary` read the accounting of the rhei they were
  pointed at.** Both resolved their accounting root one level above where a run
  laid into a Panta work root writes its records, so every spelling of a member
  — the directory, its `index.rhei.md`, `.` or `..` from inside it, or nothing
  at all — reported the project directory's records rather than the member's,
  and the project spelling reported only whatever the project directory
  happened to hold. A member now reads its own accounting root and the project
  reads the union of the run root and every rhei execution root, `basin`
  included, deduplicated by canonicalized path so a root two single-file rheis
  share is read once and each record counted once. Both commands take
  `--rhei <ID>` (repeatable), spelled as `rhei list`'s is, so one ticket's cost
  is reachable from the project spelling. An empty `rhei cost` keeps its first
  line and names the roots it searched beneath it, so an answer of zero is
  distinguishable from a miss, and `--json` carries a `roots` array on every
  reading. `rhei summary`'s task tally narrows with its records and never
  prints the roots line, because its output is publishable verbatim. A
  `rhei cost --task <ID>` naming a ticket the narrowed reading does not cover
  is now refused, naming the scope it read and the rhei the ticket belongs to,
  rather than answered with the empty totals of a ticket that genuinely cost
  nothing; an id no rhei holds anywhere is still reported as unknown. Where
  records are written is unchanged. (PR #219)

- **`runtime/run.log` no longer counts one agent invocation several times.**
  Every usage report printed its own `accounting:` line, so an invocation whose
  agent reported several turns wrote one running total per turn and then the
  final figure. Summing the lines multiplied the run's cost: a 13-invocation run
  read as $16.42 against the report's correct $8.51. An invocation that has a
  cost now contributes exactly one line, carrying what its durable accounting
  record says it cost, so the lines total to the same figure as
  `runtime/run-report.md` and `rhei cost`. The line's text is unchanged. Given
  up deliberately, and only on this surface: a reader tailing a detached run no
  longer watches the figure rise while an agent is still working. The TUI cost
  view, the browser dashboard and `rhei run --json` receive every report as
  before. (PR #227)

- **A bundled script's `${#ARR[@]}` no longer aborts instantiation.** The
  instantiation renderer read `{#` as a MiniJinja comment opener, so a template
  bundling an ordinary Bash script failed to parse, and the message blamed an
  invalid `{{ }}` expression the file did not contain. `{# ... #}` is not
  instantiation syntax, so it is now hidden from the parser and emitted as
  written. A template that really is malformed is told which `{{` or `{%` was
  never closed and the line that opener sits on, rather than the later line at
  which the parser ran out of input, and the parser's own words are quoted
  instead of contradicted. A render failure names the failing expression and its
  line, and an undeclared input is named in the remedy along with the
  `{% raw %}` route and `--list-inputs`. Hiding that text costs a handful of
  reserved code points; a template file whose own text carries one is refused by
  an error naming the file and the code point, while an input value that carries
  one is resolved into the output as the value it is. (PR #226)

- **A supervisor that finishes through a gate is no longer told it cannot
  finish.** The warning asked whether one `openDescendants` transition pointed
  straight at a final state, so a machine whose edge lands on a human gate that
  itself reaches `completed` — the shape of the shared `agora` machine — was
  told "the supervisor has no way to finish" by `rhei validate`, by
  `rhei instantiate`, and at the start of every `rhei run` on the workspace,
  while its recorded runs finished through the gate exactly as designed. The
  rule is now reachability: the walk starts at the target of the
  `openDescendants` edge and follows the edges that count as a way out of a
  state, so a gated supervisor is silent and one whose edge lands in a pocket
  that reaches nothing final is still warned about. One machine that validated
  clean starts warning — one whose only `openDescendants` exit is `cancelled`,
  because abandonment is not the supervised work being declared done. The
  run-time halt moved with the warning: it says no `openDescendants` transition
  reaches a final state, and offers pointing an existing edge at a state that
  reaches one beside the literal line it already named, which is unchanged.
  Neither surface now suggests the one repair that would delete a deliberate
  human gate. No exit code changes; warnings never fail `rhei validate`.
  (PR #236)

### Changed

- **A program's declared exit route is no longer recorded as a subprocess
  failure.** When a program state's exit code matched an exact `exit_code:`
  transition, `rhei run` both took the edge and appended
  `` `rhei run`: the subprocess exited N in state 'S'. `` to the ticket's result
  file, so a routed exit read as a failure and the result file was never empty —
  a program guarding on an empty `RHEI_RESULT_PATH` could never write the
  terminal result it owned. An exit that selects an exact integer or
  integer-array `exit_code` edge is now a declared route and leaves no entry. A
  catch-all `exit_code: nonzero` edge is unchanged and still records why,
  because there the program did not choose the code. **Breaking:** a program
  whose exact-match edge lands directly on a `final: true` state must now write
  `RHEI_RESULT_PATH` itself, which
  [the environment variable's own contract already required](functional-spec/rhei-programs.spec.md#2-environment-variables);
  one that does not leaves the ticket in its state and is reported with `result`
  among its missing outputs, rather than advancing on a sentence the engine
  wrote for it. The run report's row for such a halt now names the code that
  worker really exited with — `worker exited 3 without result (…)` — where it
  always said `worker exited 0`, which contradicted the same report's own
  Transition Ledger two sections below it. (PR #229)

- **A state machine with a state nothing can leave is now refused when it
  loads.** Every command that reads the machine — `validate`, `run`, `next`,
  `complete`, `transition`, `instantiate` — rejects it before a task is
  scheduled, instead of validating clean and stranding the first task that
  reaches the state after the work in it is spent. A state is left by an edge
  whoever moves the task can take: every `from: <state>` edge counts, a
  `from: "*"` edge counts only where its target is not `final: true`, and out
  of a `gating: true` state every edge counts, because a human takes it with
  `rhei transition`. The error names every stranded state at once, says which
  wildcard target it will not count as progress, and prints the transition line
  to add. **Upgrading:** a machine of this shape that passes `rhei validate`
  today will fail, which is the intended effect — it was going to strand a task.
  The repair is the one transition line the error prints. (PR #225)

- **A supervisor that steers by appending no longer burns the visit that did
  it.** A supervising visit which appends an open child and exits 0 without a
  result was judged against the task graph as the pass found it *before* the
  spawn, where the subtree still looked closed: `openDescendants < 1` held, the
  terminal edge was selected, the terminal result was demanded of a visit that
  never claimed to finish anything, and the run halted and re-spawned instead of
  releasing. The operands that selection reads are now taken from the plan as
  re-read after that exit, while the edge itself still leaves the state the
  invocation ran in, so the appended child counts as the descendant it is, the
  release self-loop fires, and the child runs in the same run rather than
  waiting for a second `rhei run`. (PR #224)

- **A continued snapshot session lands under the rhei that owns the ticket.**
  `rhei snapshot continue` created its agent session directory under the
  project root, so in a Panta project one ticket's live transcript sat beside
  the shared snapshot cache instead of with the rhei that ran the ticket. A
  session is that ticket's own runtime artifact rather than a stored
  generation, and the project root is where the cache lives and not the
  session. The session directory now resolves against the owning rhei's
  execution root, where `rhei run` already puts it, while the fixed-location
  `dir_template` and the locator that reads it back keep resolving against the
  directory the continuation's own agent runs in — two roots where one value
  used to stand for both. A single-file plan is unaffected, because the two
  roots are the same directory there. (PR #223)

- **A reading reports how long an invocation took, even when the record
  predates the field.** `rhei cost --json --task` now derives an invocation's
  elapsed time from `started_at` and `ended_at` when the stored `duration_ms`
  is absent, so records written before that field existed no longer publish
  wall-clock time as nothing — a 31-minute invocation read as zero. A record
  carrying its own duration is published with that number untouched, because it
  was measured in milliseconds while the agent ran while the endpoints are only
  accurate to the second. Nothing on disk is rewritten, `duration_ms` stays
  optional in `rhei.accounting.invocation.v1`, and `rhei summary` now shares
  the one derivation instead of keeping a second copy of it. (PR #215)

- **An end-to-end assertion no longer depends on where miette wrapped.**
  Captured stderr is not a tty, so every diagnostic is rendered wrapped at
  eighty columns, and a `contains` on a phrase that straddles the break fails
  on whichever machine pushes it over — a developer's, never CI's. The e2e
  harness now undoes the soft wrap at one seam, `stderr(&output)`, and every
  assertion reads stderr through it; the private normalizer in `next_tests.rs`
  and the whitespace-collapsing match in `assert_stderr_contains` are gone with
  it. Rejoining inserts the single space the wrap removed and never merges two
  rendered blocks, so a token broken mid-word and a phrase stitched out of a
  message and its help both stay visible as the faults they are. The inverse is
  close but not exact: a newline a message carries itself, falling where the
  line was already full, renders identically to a wrap and is joined, because
  miette splits a message at its own newlines before wrapping each piece. A
  test runs the two shipped diagnostics that meet this and records it as a
  known limit. The harness also clears `FORCE_COLOR` and `CLICOLOR_FORCE`
  before every spawn, so an ANSI gutter forced by the operator's shell cannot
  make the same assertions machine-dependent again. No product code changes.
  (PR #218)

- **A mangled agent note no longer takes user content with it.** `rhei init`
  paired the first of two begin markers with the *second* block's end marker,
  so anything a merge had stranded between them was read as note material and
  deleted without a word; and a marker-less `## Rhei` section ran to the end of
  the file, swallowing trailing user content that carried no heading of its
  own. Both boundaries are now written into the agent-note section of the init
  specification and pinned by tests, including one that combines a nested host
  with a malformed instruction file — the combination #187 reported and no
  case covered. (PR #220)

- **A large supervisor brief no longer aborts a `claude-code` spawn.** The
  built-in profile now delivers the prompt on Claude Code's stdin under an
  explicit bare `-p`, instead of passing it as one command-line argument that
  Linux rejects at 131072 bytes or more. Emitting the prompt flag with no value
  is a general rule rather than a `claude-code` detail: any agent entry that
  declares both `prompt_flag` and `stdin_prompt` now receives that flag, where
  before it received neither the flag nor the prompt. The two fields answer
  different questions — which flag makes the agent non-interactive, and where
  the prompt text travels — so an agent whose prompt flag *requires* a value
  must not set `stdin_prompt`, or the flag will swallow whatever follows it.
  The `--` separator every `stdin_prompt`
  profile emits moved to the end of the command line, so a state's
  `mcp_servers:` and `skills:` reach the agent rather than landing past the
  separator where they are read as prompt text and ignored. An agent that still
  carries its prompt in `argv` and hits the platform's limit now says the
  composed prompt is too big and how big, instead of telling you to check
  `PATH` for a binary that is plainly there. It says that only when the prompt
  is what the platform refused: a line put over the cap by something else
  reports what was measured and keeps the `PATH` remedy, rather than naming a
  byte count smaller than the limit in the same sentence. (PR #222)

- **`{# ... #}` is no longer cut out of an instantiated file.** Text between a
  `{#` and the next `#}` used to disappear from the output without a word — a
  script holding both `${#A[@]}` and a later `#}` lost every line in between.
  That text is now emitted verbatim, and rendering a file that contains `{#`
  prints one warning naming the file and the line of the first occurrence, so a
  template authored against the old reading is told its output has changed
  rather than changing silently. `{% raw %}` regions are unaffected, and the
  warning does not change the exit code. (PR #226)

- **Escaping inside an `{% autoescape %}` block is no longer applied.** Hiding
  `{#` from the parser means writing interpolated values through a formatter of
  rhei's own, and that formatter does not read the format an `{% autoescape %}`
  block selects, so a value interpolated inside one now arrives unescaped.
  `{% autoescape %}` is not one of the constructs §FS-rhei-templates.5 lists and
  that list is closed, so a template using it was never inside the language rhei
  renders; this line is here because the change of meaning is silent otherwise.
  (PR #226)

- **A refused agent or mode now says where the name is declared.** `rhei
  validate` listed the agents or the modes a registry already knows and stopped
  there, and a mode refused as a run started listed them and stopped there too,
  leaving the author to find the settings tree by grepping for the name they
  wanted. Each refusal now also names the key the entry is written under —
  `agents.<id>` for an agent, `agents.<id>.modes` for a mode — and the two files
  it may be written in: the project settings file rhei actually read, which is
  the deprecated `.agents/rhei/settings.json` when that is what the project has,
  and `~/.config/rhei/settings.json`. The wording is one wording per category,
  so `validate` and a run say the same thing about the same mistake, and an
  agent that declares no modes is still offered both ways out, dropping the
  brackets or declaring the mode. (PR #231)

- **Both documents that describe `rhei instantiate --output` now name its
  default.** The template-writer skill's `--output` row said only that the path
  must not already exist, and `rhei instantiate --help` said only `Output
  directory`, so neither reader could tell where a workspace lands when the flag
  is omitted, nor which path the refusal was about. Both now carry
  `<project>/<template-name>/` inside a Panta project, `./<template-name>/`
  outside one, as the specification has always said. The behaviour is unchanged.
  (PR #237)

- The repository moved to the `agent-grounds` GitHub organization, along with
  `ephor`, `fissile` and `grund`, and every live reference now names it: the
  crate's `repository`, the four npm and Python package manifests, CI's
  `GRUND_REPOSITORY` clone URL, one program path and eleven test fixtures
  carrying a full `owner/repo` key. The crate, npm and PyPI names are unchanged,
  so nothing an installer names moves. `scripts/check-registry-names.sh` now
  accepts either owner, because a package already published carries the
  repository URL it was released with until its next release, and a pattern
  naming only the new owner reads those as names taken by a stranger. The
  generated `AGENTS.md` block is left alone: `grund init` writes it from grund's
  own template, so it corrects itself when that template ships. (PR #213)

- **Supervisor handoffs link broad project context instead of pasting it.** A
  task in a state that declares `execute_on` still receives its position,
  operative handoff memory, and direct navigation to its rhei and project, but
  no longer rereads their potentially repository-scale standing context merely
  to brief one selected step. Ordinary worker prompts retain both context
  blocks unchanged. (PR #211)

- A poll state's handled wait is now recorded as `waiting` rather than `failed`
  wherever a run reports an outcome, whether an agent or a program ran the
  attempt, and the specification now says plainly that the state ledger records
  moves and a poll self-loop is not one. `rhei attach --json` also stops
  rewriting an outcome word it does not recognize to `completed`, so an older
  reader attached to a newer run passes the run's own word on instead of
  reporting every handled wait as a completion. (PR #228)

- **A retained headless lock no longer makes a dead supervisor look live.** On
  Linux, `rhei runs` now verifies that the recorded process owns a contended
  run lock, so an inherited lock held after that process exits is classified as
  ended while inconclusive ownership checks remain unknown. (PR #212)

- **`rhei next` can claim a task in a workspace that declares its own node
  kinds.** Claim mode re-reads the selected task's file under the lock before
  writing `**Assignee:**`, and that re-read now parses under the kinds the
  workspace index declares in `structure.nodeKinds` instead of the
  omitted-`structure` default of `Task` alone. A directory workspace whose
  kinds omit `task` selected its root and then failed to claim it with
  `task '<id>' not found in <task-file>`, which left the manual workflow
  usable only through `rhei run`. A basin ticket, which has no index of its
  own, was broken the same way and is fixed the same way: its kinds now come
  from the project manifest. (PR #214)

- **A test run given its own target directory rebuilds `rhei` there.** The E2E
  and integration harnesses derive the profile directory they check from the
  running test binary, but spawned their `cargo build -p rhei-cli` without it,
  so a run invoked with `--target-dir` built into the checkout's default
  `target/` and then panicked over the binary missing from the directory it was
  given. The nested build now carries `--target-dir` for that same directory,
  keeping the release profile it already followed, and a build that succeeds
  while leaving nothing behind now quotes the command it ran beside the path it
  checked. (PR #221)

- **`rhei transition --supervisor` is a documented option.** The flag rhei's
  own supervisor prompt instructs an agent to type was hidden from
  `rhei transition --help`, and offered by shell completion only from a prefix
  that matched nothing else. It is now in both, described as what it does: it
  suppresses the checkpoint the move would otherwise deliver to the named
  supervisor, rather than granting any authority — a supervisor's move on a
  held descendant lands with or without it. Behaviour is unchanged; a value
  that does not name the transitioning task's nearest in-scope supervising
  ancestor is still accepted and still has no effect. (PR #230)

- **A refused loop re-entry names the state whose budget is actually spent.**
  `rhei transition` decides a loop-back against the *destination* state's
  budget, so refusing `human-review -> supervising` reported `human-review` —
  the one state of the pair with no `visits:` to raise. The refusal now names
  the destination and carries its usage, as `visit budget for state
  'supervising' is exhausted (2/2 visits)`, and a poll state's refusal reads
  `poll budget for state 'ci-wait' is exhausted (3/3 attempts)` so the reader is
  sent to `poll.max_attempts:` rather than to a key that state cannot declare.
  Which transitions are permitted is unchanged. (PR #235)

- **The documented state-machine placements are ones Rhei finds.** The
  state-machine-writer spec and the bundled `rhei-state-machine-writer` and
  `rhei-plan-writer` skills recommended locations under `docs/` and
  `.agent-grounds/rhei/` that discovery has never searched. They now name the
  plan's directory, the workspace or project root, and a rhei's own root, and
  they say when `--state-machine` is needed. Discovery is unchanged. Re-run
  `rhei install-skills` to refresh copied skills. (PR #239)

## 3. Older releases

- [0.4.1](changelog/0.4.1.md) - 2026-09-07: - **Parallel refills preserve each task's requested execution identity.** When a freed `--parallel` slot schedules newly ready work, the reloaded task's full `**Target:**` override now continues to select its agent, mode, provider, and model instead of silently falling back to the state's target.
- [0.4.0](changelog/0.4.0.md) - 2026-09-05: - **The fissile config lives at `.agent-grounds/fissile.toml`, where an agent working in the repository can still reach it.** `.agents/` is where agent *instructions* live, and a managed permission profile mounts it read-only inside a checkout, so an agent that hit a size finding could not adjust a budget or record an exception without leaving its sandbox — tool config had ended up in the one directory it was least able to repair.
- [0.3.3](changelog/0.3.3.md) - 2026-08-31: - **`dir_template` can now name a per-working-directory session store.** A `FlatById` layout's `dir_template` may contain the placeholder `{cwd_dashed}`, which expands to this spawn's own canonicalized working directory with every character outside `[A-Za-z0-9-]` replaced by `-` — the convention Claude Code uses for its own per-project session directories — so a template like `~/.claude/projects/{cwd_dashed}` names the directory a supervised checkout actually writes into, instead of one literal path shared across every checkout.
- [0.3.2](changelog/0.3.2.md) - 2026-08-30: - **The re-spawn note on a poll state names its own `poll.max_attempts` instead of an internal sentinel.** A poll state is exempt from the visit attempt budget — `poll.max_attempts` already bounds it — and that exemption was encoded internally as `u64::MAX`, which `rhei run` then printed verbatim: `attempt 4 of 18446744073709551615`.
- [0.3.1](changelog/0.3.1.md) - 2026-08-30: - **The release commit stages `xtask/Cargo.toml`.** `Auto bump` had failed on its last three runs, always at `release.yml`'s version check and always before the publish step, with `xtask/Cargo.toml internal dependency requirement is stale: ...
- [0.3.0](changelog/0.3.0.md) - 2026-08-23: - Give a cold invocation the project's **mid-term memory**.
- [0.2.0](changelog/0.2.0.md) - 2026-08-22: - Separate a run from the surface that watches it.
- [0.1.0](changelog/0.1.0.md) - 2026-05-21: - Initial alpha release line for the Rhei CLI, Rust crates, npm wrappers, and PyPI wrappers.
