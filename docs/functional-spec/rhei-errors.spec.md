# FS-rhei-errors: CLI Errors and Guidance

Every failure Rhei reports must tell the user three things: what failed, why, and
what to run next. An error a user cannot act on without reading the source or
the spec is a defect, not a diagnostic.

This follows from [§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals): execution is predictable only when the
tool says how to get unstuck, and simple work stays simple only when a wrong
first command teaches the right second command.

## 1. Anatomy of an Error

A CLI error has a **message** and, whenever the user can fix it, a **help**
line. Miette renders them as:

```
  × template 'analyze-and-dispatch' is missing 2 required inputs
  │   subject — What the coordinator analyzes ...
  │   analysis_brief — How the coordinator should analyze the subject ...
  help: rhei instantiate analyze-and-dispatch subject='<value>' analysis_brief='<value>'
        List every input with: rhei instantiate analyze-and-dispatch --list-inputs
```

### 1.1. Message

The message names the failing subject in quotes and states the failure in one
clause. It reports **all** instances of the same failure at once — one missing
input per invocation turns supplying inputs into a guessing loop where each
attempt buys exactly one more field name.

Values the user must go back and retype are one failure class, so a single run
reports every missing input together, and every supplied-but-rejected input
together: a wrong type, a failing `validate` pattern, and a failing `format`
all cost the same one round trip. A batched report keeps each entry's own
message and its own help rather than collapsing them into a summary.

### 1.2. Help

The help line carries the next action. In order of preference it is:

1. A complete, runnable command that fixes the failure.
2. A precise edit: the file, the key, and the value shape to write.
3. A command that reveals the information the user is missing
   (`--list-inputs`, `rhei states`, `rhei templates`).

A runnable command reproduces the invocation the user actually typed — the
arguments they already supplied plus the correction — so it can be pasted
without re-deriving anything.

One correction changes authored dependency text rather than retrying the
refused read-only command: when an otherwise-valid consumed producer is absent
from the consumer's direct `**Prior:**`, both `validate` and `run` name the
consumer and producer and print `help: rhei migrate export-priors TARGET`.
The target retains a shell-safe supplied spelling, or is the explicitly
discovered project/rhei path when the failed command omitted it. The help never
suggests that validation or execution will migrate implicitly.
[§FS-rhei-migrate.5](rhei-migrate.spec.md#5-recovery-sequence)

A correction is offered in a form the CLI accepts. A value the user can assign
is shown as the assignment; a scalar nested inside an array or object has no
assignment syntax of its own, so the correction names the enclosing input and
shows the corrected value alone. A suggestion that pastes back as a *different*
error is worse than none, because it costs a round trip to discover that.

Errors that cannot be user-caused (broken internal invariants) carry help that
says so and asks for a bug report; they never invent a remedy.

A refusal's help names the action that changes its blocking condition. A retry
is a next action only when it can overcome that condition; inspecting the
message and repeating the same refused invocation is not a remedy. If the next
action requires another actor, help hands off to that actor instead of telling
the caller to bypass the boundary.

For operator-forced recovery, a non-terminal caller is directed to a human
operator at an interactive terminal for fresh typed confirmation; an agent
must not perform the ceremony. A terminal ancestor refusal identifies the
ancestor to reopen first and gives its operator-only transition command,
retaining the plan path, task, source and target state. The command goes only
to a state the forced path would let that ancestor enter: one its profile
allows that qualifies by [§FS-rhei-reset.5.1](rhei-reset.spec.md#51-offering-reset-as-a-remedy)'s rule, measured against the ancestor's own
counted visits. It tries the requested target first, then the ancestor's
profile `initial`, then any other state. Where a state was passed over for a
spent budget, the help says why the command goes elsewhere, naming the first
such state with the forced path's own refusal ([§FS-rhei-errors.1.5](rhei-errors.spec.md#15-the-subject-a-check-measured)). Where every
candidate is passed over and a spent budget is why, the help prints no command:
it names each spent state with that refusal and sends the limit to the workflow
author, since `--force` cannot bypass a visit budget. These are explanations
of the existing safeguards in [§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery), not permission to bypass them.

Retrying a terminal task instead explains the need to reopen it through the
authorized operator path before retry can apply. Retrying inside a worker
hands the request to an operator outside the worker; it never suggests clearing
`RHEI_TASK_ID` or otherwise evading that boundary. A `--reason` without
`--force` refusal tells the caller to remove `--reason`; any offered force
alternative states that it belongs to the human operator's ceremony.

#### 1.2.1. The Target a Help Names

A help that names a command taking a plan or project prints the target of the
command that printed it, so the line runs as printed: `rhei run plan.rhei.md`
ends `See what is left with: rhei list plan.rhei.md`, and nobody retypes a target
they typed a moment earlier. A help that names such a command with no target at
all is the same defect: `List the tickets with: rhei list`, pasted from another
directory, finds no plan, and a bare `rhei states` after a command given its own
machine describes another machine with no error to say so.

- **A typed target is printed as typed**, quoted as one shell word
  ([§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety)). It is not reduced to its file name, which fails from
  another directory, and it is not the project a member target widens to
  ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)): `rhei run panta/auth.rhei.md` prints `rhei list panta/auth.rhei.md`, because
  `rhei list panta` is a wider scope than the one the user asked about.
- **A discovered target is printed as discovered.** A command whose target was
  omitted prints the project or rhei path it discovered, as the
  `rhei migrate export-priors` line above does. A refusal raised before its
  command resolved a target discovers one the way that command would have.
- **A command with no target of its own discovers none.** What `rhei instantiate`
  or `rhei templates` was asked about is not whatever plan the directory holds, so
  it names the path its help is about: `rhei instantiate --execute` names what it
  laid.
- **A placeholder stands only where the command has no target at all**: `rhei
  reset` refused for omitting one, which it never infers (§FS-rhei-panta.6), or a
  command whose discovery found nothing. A value only the reader can choose, such
  as the state a human gate moves to, is not a target and stays a placeholder.
- **A verb that runs with no target stays bare where the command has none.**
  `rhei states` answers with no target at all, so with nothing to name a help
  prints `rhei states`, with any machine the command was given, and never gains a
  placeholder where the bare line runs.
- **A given `--state-machine` is passed on**, spelled the way the target is and
  last on the line, to a command that takes the flag. Without it the pasted command
  resolves another machine and may refuse for a different reason, which
  [§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help) calls worse than no suggestion. A machine the command
  resolved for itself is not passed on: the pasted command resolves it the same way.
- **The command stays on one line.** A diagnostic whose help or message names a
  target this way is not wrapped. The renderer breaks at spaces, so a target
  holding a space would break inside its quotes and paste as a different path; the
  terminal soft-wraps the long line instead. This is the exception
  [§FS-rhei-errors.2](rhei-errors.spec.md#2-copy-paste-safety) allows.
- **What is read elsewhere is absolute.** A help is printed by the invocation
  that raised it and reaches no run report ([§FS-rhei-run-report.1](rhei-run-report.spec.md#1-report-artifact)), so as typed it
  is read from the directory it was typed in. The exception is a detached run,
  whose stderr is its console log, `runtime/run.log` ([§FS-rhei-run-headless.8](rhei-run-headless.spec.md#8-failure-modes)), read
  later and from anywhere. A detached run prints the target and the machine
  absolute, not resolved through a symlink, for the reason [§FS-rhei-reset.5.1](rhei-reset.spec.md#51-offering-reset-as-a-remedy) does.
  One help does reach the run's records: a strict refusal's remedy
  ([§FS-rhei-seal.8.4](rhei-seal.spec.md#84-the-refusal)) is also its `seal_halt` record's remedy and the next action of the
  report's Attention row, and in a detached run all three print it absolute.
- **A verb with a subcommand is read as one.** `rhei budget show project` and
  `rhei snapshot list --plan project` name a target as `rhei list project` does,
  so they stay on one line too. No `--state-machine` is passed on: `rhei budget`
  takes none, and `rhei snapshot list` reads the same store with or without one.
  Where the command has no target they stay bare, because each discovers its own.

#### 1.2.2. A State Machine That Will Not Load

A state machine file fails to load in one of four ways, each with its own help.
A scan of the text as YAML, document by document, tells the first three apart,
and reading the message never does. All four open with the same headline,
`failed to parse state machine '<path>': <cause>`, and all four end in the
command that re-checks the file: `rhei states --state-machine <path>`.

Text the scan rejects in any document is a syntax failure. The help says the
file is not valid YAML and to fix the syntax. It refers to the position above
only when the headline prints one, which a failure at the very first character
of the file does not.

Text the scan accepts in every document, when there is more than one, is a
several-documents failure. The help says the file is valid YAML but must hold
one document, and says nothing about syntax.

Text the scan accepts as one document, which still cannot be read into a YAML
value, is a read failure: a plain integer too large to hold, a nesting or alias
limit, a duplicate key. The help says the file cannot be read as a state machine
and to correct what the headline names. It makes no claim either way about valid
YAML, says nothing about syntax, and refers to no position.

Text that is read into a YAML value but has the wrong shape is a shape failure. Its help says the
file is valid YAML, never says to fix the syntax, and never refers to a position
the headline does not print. What it asks for depends on what the shape lacks:

- A required top-level field is absent: the precise edit, naming the key, that
  it goes at the top level of the file, and its value shape, as
  [§FS-rhei-states.1.1](rhei-states.spec.md#11-top-level-fields) lists them.
- The document holds no top-level mapping, because it is empty, only comments,
  `null`, a scalar or a sequence: that the document must be a mapping, naming
  every required top-level key. There is no one field to add. Independently of
  that help, the headline's cause is `the document must be a mapping`, inside
  the same wrapper naming the failing file. It names no Rust or serde type,
  and empty or comment-only input must not instead report a missing `name`.
  This also applies to a tagged sequence. Both `rhei states --state-machine`
  and `rhei validate` report this cause and retain their refusal exit code 1;
  visualization and template-union refusals that display the load error use
  the same cause, retaining their surrounding diagnostic context and help.
  Which inputs are accepted does not change.
- Anything else, such as a wrong type at any depth or a field missing from a
  nested entry: the correction of what the headline names. The headline already
  carries the key's path and position, so the help makes no claim that the key
  belongs at the top level.

The help states the correction itself rather than pointing at a document: the
reader of a refusal may hold only the installed binary, with no repository
around it to follow a path into.

### 1.3. Near Misses

When a name is not found and a declared set of valid names exists, the error
suggests the closest match and, when the set is small, lists it. Unknown agent
ids, states, template names, template inputs, task ids, and program names all
follow this rule.

A listing is a substitute for a near miss, not an inventory: past eight
candidates the error names the first few, says how many remain, and defers to
the command that lists them all. Each candidate appears once — registries are
built by merging built-in entries with user settings, and a name present in
both is still one name.

### 1.4. Where the Refused Name Is Declared

Listing the names a registry knows says what exists; it does not say where to
write the one the author wanted. So when a name is refused against a registry
that settings declare, the error also names the key the entry is written under
and the files it may be written in — the project settings file and the global
one ([§FS-rhei-agents.1.1](rhei-agents.spec.md#11-global-and-project-settings)). Unknown agent ids and unknown agent modes follow this
rule.

The clause is an instruction, not a report. A registry is seeded with built-in
entries, so a name can be refused against one that no settings file contributed
to, and there is then no file to report. What the author needs is the same
either way — where to declare the name they wanted — so the wording still reads
correctly when the file it names does not exist yet.

Which project file is named is not a free choice. It is the file the merge
resolved and never the one Rhei writes ([§FS-rhei-agents.1.1](rhei-agents.spec.md#11-global-and-project-settings)), so a project still
on the deprecated home is sent to the file Rhei is actually reading rather than
to one that would shadow it.

Where a single command reports many failures under one help line ([§FS-rhei-errors.1.1](rhei-errors.spec.md#11-message)), this
remedy rides in each entry rather than in the help: one help line cannot carry
four different keys, and the entry is also what a machine consumer reads as that
failure's own message ([§FS-rhei-errors.5](rhei-errors.spec.md#5-machine-readable-errors)).

### 1.5. The Subject a Check Measured

A check that weighs several candidate subjects names, in its message, the one
it actually measured — not the one the caller happened to be standing on. A
message naming the wrong subject costs more than a vague one would: it is a
confident pointer at a place with nothing to change, and the round trip is
spent before the reader can discover that.

The measurement comes with it. A budget reported as spent says how far it is
spent, as `used/limit` with the unit it is counted in, so the reader learns what
to raise it past without re-deriving the count from the plan's metadata.

**Refusing a counted-loop re-entry.** A loop-back into a counted state is
refused once that state's budget is spent, and the budget consulted is the
**destination** state's ([§FS-rhei-transitions.4.3](rhei-transitions.spec.md#43-counted-loops)) — which, on a loop back out
of a gate, is not the state the task is sitting in. The refusal therefore names
the destination, and names which kind of budget it was, because the two kinds
are mutually exclusive and are raised by different keys:

```
visit budget for state 'supervising' is exhausted (2/2 visits)
poll budget for state 'ci-wait' is exhausted (3/3 attempts)
```

The first names a state declaring `visits:`; the second a state declaring
`poll.max_attempts:` ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)). A refused self-loop names its own
state, which is both the state being left and the state whose budget was spent,
so the rule reads the same either way: the subject is whatever the check
measured.

### 1.6. Composed block context

A composition error names the mount alias or public endpoint the user wrote and
the manifest path that declared it. When two declarations disagree, it names
both paths. A nested failure includes the complete root-to-failure alias chain;
a recursion failure includes the repeated block and whole cycle. Unknown ports,
data endpoints, inputs, and aliases list the small valid set or suggest the
closest name under [§FS-rhei-errors.1.3](rhei-errors.spec.md#13-near-misses).

The help line gives the nearest accepted edit or a command that reveals the
surface, such as `rhei instantiate ... --list-inputs`. An ownership or identity
collision must name both owners and tell the author which alias, declaration,
or compatibility mapping to change. It must not surface as a later duplicate
state, task, path, or parser failure stripped of its mount context.

## 2. Copy-Paste Safety

Any command Rhei prints — in an error, a help line, or a success summary — must
survive being pasted into an interactive shell: every value parses as the single
argument Rhei meant, and no token is ever split across lines.

A command wider than the terminal still wraps, at a space, unless the point that
prints it keeps it whole, as [§FS-rhei-reset.5.1](rhei-reset.spec.md#51-offering-reset-as-a-remedy) and [§FS-rhei-errors.1.2.1](rhei-errors.spec.md#121-the-target-a-help-names) do. Rejoining two lines
is a visible, recoverable inconvenience; a path or selector broken mid-token is
neither, which is why the renderer breaks only at spaces. Suggestions are kept
short for the same reason — paths are printed relative to the working directory
when they sit beneath it, apart from the target of [§FS-rhei-errors.1.2.1](rhei-errors.spec.md#121-the-target-a-help-names), which
keeps the spelling the user typed.

Values are POSIX-quoted whenever they contain characters outside
`[A-Za-z0-9_.,:/@%+=-]`, and whenever they begin with `=`, which zsh expands to
a command path. This is not cosmetic: execution target selectors contain `[`
and `]`, and an unquoted `agent=codex[yolo]:openai:gpt-5.5` fails in zsh with
`no matches found` before Rhei is ever executed. Printed selectors are
therefore always quoted as `agent='codex[yolo]:openai:gpt-5.5'`.

The quoting is the platform's own, because the shell the command is pasted into
is: a value that needs quoting is wrapped in POSIX single quotes on Unix and in
`cmd`'s double quotes, with any embedded `"` doubled, on Windows.

In a `KEY=VALUE` argument only the value is quoted, so the key — which is what
the suggestion is teaching — stays readable.

The same rule applies to documentation and to `--list-inputs` output, which is
read as a source of copyable values. Where a default is shown as a multi-line
block for readability, the block is followed by a `copy:` line carrying the
same value as a single quoted assignment, because the block's own scalars are
bare YAML.

A repair example is keyed to the input it corrects. A suggestion built around a
guessed name is not a next action: pasted back, it fails on the name rather
than on the value.

## 3. Failing at the Input Boundary

A value is validated where the user supplied it, not where it is eventually
consumed. Template inputs are checked during `rhei instantiate` argument
resolution — before any file is rendered — so the error names the input the user
typed rather than the rendered artifact that failed to load.

### 3.1. Execution Target Inputs

A template input declared with `format: execution-target` is parsed as an
execution target selector ([§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification)) at instantiation time. A malformed
value reports the input name, the offending value, the accepted shapes, and a
corrected, shell-quoted example built from the value the user supplied.

### 3.2. Unrecognized Plan Directories

When a command's plan argument is an existing directory containing neither
`index.panta.md` nor `index.rhei.md`, Rhei rejects it at the input boundary with
a nonzero exit. The message names the directory and identifies it as neither a
recognized Panta Project nor a Directory Workspace. It presents the two valid
shapes as alternatives: `index.panta.md` makes the directory a Panta Project
([§FS-rhei-plan-language.1.5](rhei-plan-language.spec.md#15-panta-project)), while `index.rhei.md` makes it a Directory Workspace
([§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)). It must not imply that both manifests belong in one
directory.

Help points to the correction that matches the directory's intended level. It
either tells the caller which one manifest to add or tells them to pass the
actual plan or workspace path. In particular, when `rhei run DIRECTORY --rhei
ID` was given a container whose `DIRECTORY/ID` is a recognized Directory
Workspace, the help explains that `DIRECTORY` is one level above the workspace
and gives a runnable command targeting `DIRECTORY/ID` directly.

A missing manifest is a plan-shape failure, not evidence of a filesystem
failure. Its diagnostic therefore gives no permissions, writability, or free
space advice. Genuine filesystem failures retain their cause-derived guidance
from [§FS-rhei-errors.6](rhei-errors.spec.md#6-coverage). A headless run reports the same startup diagnosis and nonzero outcome
through [§FS-rhei-run-headless.1.1](rhei-run-headless.spec.md#11-startup-is-synchronous) rather than hiding it in the detached run's log.

This refusal does not change what Rhei accepts: recognized Panta Projects,
directly addressed Directory Workspaces, and Single-File Plans retain their
existing discovery and loading behavior.

## 4. Paths in Errors

An error never points at a path the user cannot inspect. Temporary directories
used for `--dry-run` rendering, and output directories that instantiation
removes on failure, are not named as if they were user artifacts; the error
names the input or template that produced the bad content instead.

A filesystem failure while rendering into scratch space names the file by its
position inside the template — a path the user can open — and points the remedy
at `$TMPDIR` rather than at a directory that no longer exists.

## 5. Machine-Readable Errors

Commands with a JSON output mode emit errors as a single-line JSON object on
stderr. The object carries the help text alongside the message so machine
consumers see the same next action as humans:

```json
{"error":{"message":"...","help":"..."}}
```

`help` is omitted when the error carries none.

A command may carry named detail members beside `message` and `help`, where the
refusal is about a subject the caller has to read rather than only act on — the
state of an account, the two names a conflict is between. `message` and `help`
keep their meaning exactly: the detail is what the prose already said, in the
shape a reader does not have to parse it out of. Which members a command emits
is that command's own contract and is specified where the command is.

## 6. Coverage

The contract applies to every diagnostic `rhei` prints on a failing exit path.
Filesystem failures derive their help from the underlying error kind (missing
path, permission, already-exists) and always name the path.

Coverage is a property of the whole binary rather than of any one call site, so
it is enforced by a test that fails when a diagnostic is raised without help,
not left to review. So is [§FS-rhei-errors.1.2.1](rhei-errors.spec.md#121-the-target-a-help-names): a test fails when a printed
command holds a target placeholder at a site that test does not list, each listed
site with the reason it has no target to print, and when help text names a command
taking a target with no target at a site it does not list, each with the reason
the line is right as it stands.

Help is derived from the failure, never assigned by the area of code it sits
in. A remedy that does not act on the reported failure — telling the user to
check a destination for a command that writes to stdout, or to edit the state
machine for a `waitpid` failure — is the same defect as no help at all, and the
honest answer for a cause the user cannot have created is [§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help)'s bug report.
Recurring categories share one wording so that improving a remedy improves
every site that reaches it.

This includes diagnostics constructed through shared macros and errors
forwarded from lower layers. A wrapper must require failure-specific help at
its caller, preserve useful help already carried by the underlying diagnostic,
or use a helper that derives guidance from the actual cause. A universal retry
or bug-report fallback does not cover user-caused failures. Filesystem
conversion keeps the failing path and the underlying cause together with its
cause-specific action.

The coverage test checks wrapper callers as well as direct diagnostic sites;
a help clause in a macro definition cannot cover a help-less caller. A negative
fixture proves such a caller is rejected. Runtime tests also check what the
help tells the reader to do, since presence of a help line or absence of a
particular misleading phrase alone cannot establish this contract.

`rhei remove` refusals are covered like every other: each opens
`<qualified-id> cannot be removed: <reason>`, names every blocker with the
path, task or run it was found in, and ends with the remedy that applies — for
a ticket with history, its machine's cancellation through `rhei transition`
([§FS-rhei-remove.3.4](rhei-remove.spec.md#34-a-ticket-with-history)); for a dependent or a child, the ticket to change first.
A pending removal that blocks another entry point prints the one `rhei remove`
invocation that resumes it ([§FS-rhei-recover.4](rhei-recover.spec.md#4-pending-root-interlock)).

## 7. A Spawn Failure the Command Line's Size Explains

An agent that carries its prompt in `argv`
([§FS-rhei-agents.2.2](rhei-agents.spec.md#22-modes)) is bounded by the
platform's limit on a command line, and a plan that has run long enough composes
a prompt that reaches it. The operating system reports this as a failure to
start a process, which reads exactly like a missing binary — so a spawn failure
asks one further question before it prints the `PATH` remedy: **could this be
the size of what rhei composed?**

When the answer is yes, the failure names the size as the cause and points at
prompt delivery:

```
  × failed to spawn agent 'gemini': Argument list too long (os error 7)
  │ the composed prompt is 214016 bytes and this agent passes it as one
  │ command-line argument, past this platform's 131072-byte limit
  help: use an agent that delivers the prompt on stdin ('claude-code',
        'codex'), or register a custom agent with "stdin_prompt": true
```

The answer is yes only when the prompt is what the platform refused. A line can
be too long for a reason the prompt had nothing to do with, and [§FS-rhei-errors.7.2](rhei-errors.spec.md#72-when-the-size-is-not-the-prompts) says what
the failure states then.

Three things follow from [§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help), and none of them is cosmetic.

The remedy is a change the user can make. A settings entry for a built-in id
replaces that profile wholesale
([§FS-rhei-agents.1.3](rhei-agents.spec.md#13-merge-semantics)), so "set
`stdin_prompt` on `gemini`" would paste back as a different failure — a partial
profile with no session layout — which [§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help) rules out. The help therefore names
the agents that already deliver on stdin and the shape of a custom entry that
does.

The size is stated, not implied. A user cannot see the composed prompt, so the
byte count and the platform's limit are what turn "too long" into a decision
about which agent to run.

Every other spawn failure keeps the message and the help it has. A binary that
is genuinely missing still says so, and still says `rhei diag`.

### 7.1. What Answers the Question, per Platform

The rule is one rule on every platform; the evidence behind it is not, and
[§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity) asks the
difference to be stated where it occurs.

On Linux and macOS the operating system answers. Both report `E2BIG` — errno 7 —
and rhei reads it from the failure it was handed. The two caps differ: Linux
rejects any single argument of 131072 bytes or more whatever `ARG_MAX` says —
the cap is on the argument as the kernel stores it, which is the text plus its
terminating NUL, so the last size that fits is one byte short of it — while
macOS has no per-argument cap and fails on a total of roughly one megabyte. The
same error therefore fires at a different size on each, which is a declared
difference in the limit rather than a difference in behaviour.

On Windows rhei answers, by measuring the command line it composed against the
32767-character `CreateProcessW` limit. Windows reports no error distinct enough
to read this from, and the Rust error kind that would name it portably is newer
than this workspace's minimum supported Rust version, so there is nothing to
match on.

**The measurement is the fallback for an indistinct failure, not an override of
a distinct one.** Windows still names the failures it can name — a binary that
is not there, and one this user may not run — and a failure that names its own
cause answers the question before the ruler comes out. A long command line
standing beside it is a coincidence, and reading it as the cause is how a
missing binary comes to be blamed on the prompt. Only what the platform leaves
unexplained is left for the measurement to explain.

**The measurement explains a failure; it never causes one.** Rhei does not
refuse to spawn on a size it has only estimated. A composed length is an
approximation of what the operating system will actually count, and turning an
invocation that would have run into a refusal is worse than the failure this
point is about.

### 7.2. When the Size Is Not the Prompt's

Two things can be true at once: the platform refused the line, and the prompt is
not what made it too long. A failure that prints the prompt's byte count beside a
larger limit has then said something false in the same sentence as something
true, and offered a remedy that would leave the line exactly as long as it was.
That is the confidently wrong cause [§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help) rules out, arriving by a different door
than a missing `PATH`.

So the question is asked in two parts. **Did the prompt reach the command line at
all?** That is read from the argument vector rhei composed, because more than one
transport keeps the prompt off it: the Claude Code stream-json arm
([§FS-rhei-agents.1.1.2](rhei-agents.spec.md#112-agents)) sends the
prompt down the same pipe without declaring `stdin_prompt`, and would otherwise
be told its prompt was too long for a line it is not on. **And is the prompt what
is over the cap?** On a per-argument cap that is the prompt's own bytes; on a
total cap it is whether taking the prompt off the line would bring the rest back
under. Only when both hold is the prompt named.

On a total cap the sentence carries both numbers — what the whole line measured
and how much of it was the prompt — since the prompt's own size is below the
limit there and stating it alone would read as the contradiction this point
exists to prevent.

When the prompt is not the cause, the failure states what was measured and keeps
the help it had, because prompt delivery is not the remedy:

```
  × failed to spawn agent 'argv-agent': Argument list too long (os error 7)
  │ the composed command line is 200104 bytes, past this platform's
  │ 131072-byte limit
  help: the agent command could not start. Check it exists on PATH and is
        executable: rhei diag
```

And rhei states no size its own measurement does not support. A composed length
is an approximation of what the operating system counts ([§FS-rhei-errors.7.1](rhei-errors.spec.md#71-what-answers-the-question-per-platform)), so a refusal of a
line rhei measures as fitting is left to speak for itself: nothing is added, and
the `PATH` remedy stands alone.

## 8. Related

- [Templates Specification](rhei-templates.spec.md) [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) —
  instantiation inputs and `--list-inputs`
- [Agents Specification](rhei-agents.spec.md) [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification) — execution target
  selector grammar
- [Validate Specification](rhei-validate.spec.md) [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) — plan and
  state machine validation diagnostics
