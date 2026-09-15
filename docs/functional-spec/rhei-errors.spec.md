# FS-rhei-errors: CLI Errors and Guidance

Every failure Rhei reports must tell the user three things: what failed, why, and
what to run next. An error a user cannot act on without reading the source or
the spec is a defect, not a diagnostic.

This follows from [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals): execution is predictable only when the
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

A correction is offered in a form the CLI accepts. A value the user can assign
is shown as the assignment; a scalar nested inside an array or object has no
assignment syntax of its own, so the correction names the enclosing input and
shows the corrected value alone. A suggestion that pastes back as a *different*
error is worse than none, because it costs a round trip to discover that.

Errors that cannot be user-caused (broken internal invariants) carry help that
says so and asks for a bug report; they never invent a remedy.

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
one (§FS-rhei-agents.1.1). Unknown agent ids and unknown agent modes follow this
rule.

The clause is an instruction, not a report. A registry is seeded with built-in
entries, so a name can be refused against one that no settings file contributed
to, and there is then no file to report. What the author needs is the same
either way — where to declare the name they wanted — so the wording still reads
correctly when the file it names does not exist yet.

Which project file is named is not a free choice. It is the file the merge
resolved and never the one Rhei writes (§FS-rhei-agents.1.1), so a project still
on the deprecated home is sent to the file Rhei is actually reading rather than
to one that would shadow it.

Where a single command reports many failures under one help line (§1.1), this
remedy rides in each entry rather than in the help: one help line cannot carry
four different keys, and the entry is also what a machine consumer reads as that
failure's own message (§5).

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

## 2. Copy-Paste Safety

Any command Rhei prints — in an error, a help line, or a success summary — must
survive being pasted into an interactive shell: every value parses as the single
argument Rhei meant, and no token is ever split across lines.

A command wider than the terminal still wraps, at a space. Rejoining two lines
is a visible, recoverable inconvenience; a path or selector broken mid-token is
neither, which is why the renderer breaks only at spaces. Suggestions are kept
short for the same reason — paths are printed relative to the working directory
when they sit beneath it.

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
from §6. A headless run reports the same startup diagnosis and nonzero outcome
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

## 6. Coverage

The contract applies to every diagnostic `rhei` prints on a failing exit path.
Filesystem failures derive their help from the underlying error kind (missing
path, permission, already-exists) and always name the path.

Coverage is a property of the whole binary rather than of any one call site, so
it is enforced by a test that fails when a diagnostic is raised without help,
not left to review.

Help is derived from the failure, never assigned by the area of code it sits
in. A remedy that does not act on the reported failure — telling the user to
check a destination for a command that writes to stdout, or to edit the state
machine for a `waitpid` failure — is the same defect as no help at all, and the
honest answer for a cause the user cannot have created is §1.2's bug report.
Recurring categories share one wording so that improving a remedy improves
every site that reaches it.

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
be too long for a reason the prompt had nothing to do with, and §7.2 says what
the failure states then.

Three things follow from §1.2, and none of them is cosmetic.

The remedy is a change the user can make. A settings entry for a built-in id
replaces that profile wholesale
([§FS-rhei-agents.1.3](rhei-agents.spec.md#13-merge-semantics)), so "set
`stdin_prompt` on `gemini`" would paste back as a different failure — a partial
profile with no session layout — which §1.2 rules out. The help therefore names
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
That is the confidently wrong cause §1.2 rules out, arriving by a different door
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
is an approximation of what the operating system counts (§7.1), so a refusal of a
line rhei measures as fitting is left to speak for itself: nothing is added, and
the `PATH` remedy stands alone.

## Related

- [Templates Specification](rhei-templates.spec.md) [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) —
  instantiation inputs and `--list-inputs`
- [Agents Specification](rhei-agents.spec.md) [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification) — execution target
  selector grammar
- [Validate Specification](rhei-validate.spec.md) [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate) — plan and
  state machine validation diagnostics
