# FS-rhei-complete: `rhei complete`

Atomically complete a task: transition to a terminal state, write the result to a file, link it from the task body, and remove the `**Assignee:**` line. This is the single command an agent calls when it is done with a task.

`rhei complete` is sugar. It owns exactly one thing no other verb does —
inferring the one-hop non-cancelled terminal target (§4.1) — and reaches it
through the same shared transition path as every other verb, carrying the
message selected by `--result` or `--result-file` (§2.2, §4). The result file,
its link, the dropped assignee, the ledger
line, and the refusal of a terminal entry with no result are all properties of
entering a `final: true` state ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)), not of this command.

## 1. Usage

```bash
rhei complete <TICKET_ID> --result <MESSAGE>
rhei complete <TICKET_ID> --result-file <PATH>
rhei complete [RHEI_PLAN] --task <TASK_ID> --result <MESSAGE>
rhei complete [RHEI_PLAN] --task <TASK_ID> --result-file <PATH>
```

The first form is the everyday one: every other ticket surface (`rhei list`,
`rhei next`, error messages) prints bare ticket ids, so the id is what an agent
or human has in hand when finishing work — `rhei complete auth.1 --result "…"`
must work as pasted. The positional argument is disambiguated in §2.1; the
`--task` form stays for scripts that pass an explicit plan path.

## 2. Options

| Flag                   | Required | Default | Description                                       |
|------------------------|----------|---------|---------------------------------------------------|
| `--task <ID>`          | No       |         | Ticket identifier: project-qualified (`auth.1`) or rhei-local (`1`). Alternative to the positional ticket id; exactly one of the two must name the ticket. See §2.1. |
| `--result <MSG>`       | One of   |         | Literal result message for the task. Rejected when empty or whitespace-only. |
| `--result-file <PATH>` | One of   |         | Read the result message from a UTF-8 file, or from stdin when `PATH` is exactly `-`. See §2.2. |
| `--no-callbacks`       | No       | false   | Skip execution of `on_leave`/`on_enter` callbacks |

### 2.1. Ticket Targets

A ticket target accepts either the project-qualified ticket id (`auth.1`) or a
rhei-local shorthand (`1`). A shorthand resolves only when exactly one rhei in
the project contains that ticket; when more than one does, the error names the
qualified candidates. There is no `--rhei` flag — the explicit ticket target is
the scope ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

The positional argument doubles as the plan path (`RHEI_PLAN`, as on every
other command) and the ticket id, resolved in this order:

1. With `--task` present, the positional is the plan path — unchanged legacy
   behavior.
2. Without `--task`, a positional that names an existing file or directory is
   the plan path, and the command fails asking for the ticket. Existence wins
   over id shape so a plan named like an id (`1/`) never silently completes a
   ticket.
3. Without `--task`, a positional that names no existing path, contains no path
   separator, does not end in `.md`, and parses as a ticket id is the ticket;
   the plan is inferred from the working directory exactly as when the
   positional is omitted.
4. Anything else is reported as a plan path that does not exist.

With neither a positional ticket nor `--task`, the error shows the positional
form first: `rhei complete <ticket-id> --result <message>`.

### 2.2. Result Sources

Exactly one of `--result <MSG>` and `--result-file <PATH>` is required.
Supplying both or neither is an argument error. `--result` always carries its
value literally, including the value `-`; it never reads stdin.

`--result-file` reads the named source in full as UTF-8. The exact value `-`
reads stdin through EOF. Every other value names a filesystem path, so a file
literally named `-` can be selected as `./-` or by another non-sentinel path.
Relative paths resolve from the invocation's working directory, including
when the selected Single-File Plan or Directory Workspace is elsewhere;
absolute paths are used as given. This input-path rule is independent of the
fixed output-artifact routing in §4.2–4.3.

The selected message must contain something other than whitespace. Rhei may
trim only to make that test; it does not trim or otherwise rewrite accepted
text. It preserves every valid UTF-8 byte, including backticks, quotes,
embedded line feeds, and trailing LF or CRLF sequences, when passing the
message to completion. A missing or unreadable source, invalid UTF-8, or empty
or whitespace-only loaded text is an input error.

## 3. Result File

Each task has a result file at a fixed path, keyed by the **project-qualified**
ticket id under the owning rhei's execution root:

```text
runtime/results/<task-id>.md
```

The `runtime/results/` directory is created if it does not exist. A markdown
link to the result file is appended to the task body (after task content,
before child nodes):

```markdown
> **Result:** [<task-id>](runtime/results/<task-id>.md)
```

`<task-id>` here is the qualified id (`auth.1`), even though the task heading in
the plan file keeps its rhei-local form (`### Task 1:`). Plans completed before
ticket ids gained their rhei prefix keep their rhei-local link and artifact —
no command rewrites an existing result link, and both forms validate
([§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup), [§FS-rhei-plan-language.3.8](rhei-plan-language.spec.md#38-result-block-consistency)).

This keeps task files concise — the result detail lives in a separate artifact under `runtime/`, consistent with how other runtime outputs (findings, verifications, fixes) are stored in directory workspaces.

The file is not optional and it is not `rhei complete`'s private artifact. A
non-empty result at this path is an implicit required artifact of every `final:
true` state, enforced on the edge into that state by whichever verb drives it
([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result), [§FS-rhei-transition-cmd.3.2](rhei-transition-cmd.spec.md#32-terminal-result-on-entry)). This section defines the
path, the link, and the entry format; [§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result) defines the
obligation.

**Terminal finalization** is the work that runs once a transition into a
`final: true` state succeeds, identically on every path:

1. Append `<task-id> <from>@<to>` to `runtime/state-transitions.log` (§3.1).
2. Append the caller's message, when one was carried, to
   `runtime/results/<task-id>.md` in the entry format of §3.2.
3. Ensure `runtime/results/<task-id>.md` exists, so the link below never points
   at a missing file.
4. Remove the `**Assignee:**` line from the task (no-op if absent).
5. Add the `> **Result:**` link to the task body if it is not already there.

A non-terminal transition performs only steps 1 and 2. Step 2 with no message
is a no-op, so a plain non-terminal `rhei transition` creates no result file.

### 3.1. State Transition Ledger

Every task state **move** is appended to one central file:

```text
runtime/state-transitions.log
```

Each line is deterministic and timestamp-free:

```text
<task-id> <from>@<to>
```

This file is the source of truth for task state history across `rhei
transition`, `rhei complete`, `rhei run`, callbacks, system transitions, and
human-gate dashboard transitions.

A **move** is a change of state, which is what every reader of this file already
takes a line to mean: `rhei reset` recovers a ticket's authored state by
rewinding along them ([§FS-rhei-reset.2.2](rhei-reset.spec.md#22-authored-state)) and `rhei viz` draws the path they
record ([§FS-rhei-viz.4](rhei-viz.spec.md#4-surroundings-inspector)). One selected transition is therefore not a line here —
a poll state's self-loop ([§FS-rhei-states.2.2](rhei-states.spec.md#22-semantics)). `rhei run` handles it by
scheduling the next attempt instead of applying it, the task stays in the state
it was in, and nothing is appended. A machine that declares the self-loop is not
declaring an edge this file will show being walked; it is granting the state
permission to repeat. The attempts themselves are in the run event journal, one
`start@`/`end@` pair each ([§FS-rhei-run-tui.1.7](rhei-run-tui.spec.md#17-journal-format)), which is where a reader
counting waits looks.

### 3.2. Result File Format

The result file stores completion result detail. Each completion appends a
message entry:

```markdown
## Result

<message>
```

Any verb that carries a message appends one such entry — `rhei complete
--result`, `rhei complete --result-file`, `rhei transition --result`, and `rhei
run`'s engine-owned failure routes alike — so the file reads the same however
the ticket was driven. Rhei
writes the entry as the heading, a blank line, the message, and a trailing
blank line, so successive entries stay separated. The ordered audit trail of
state transitions lives in `runtime/state-transitions.log`.

For `--result-file`, the accepted source bytes are the `<message>` inside this
Rhei-owned framing. Embedded and trailing LF or CRLF bytes remain unchanged;
the entry's heading, leading blank line, and final blank line are added around
them. The source is a caller message, not an artifact import: if a worker has
already written the task's result file, that artifact remains verbatim and the
new caller message is appended through the normal completion path.

A file a **worker** wrote itself ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) is taken verbatim: Rhei
reads it to decide the obligation is met and never rewrites it. So the two
routes coincide exactly when the worker writes the entry above — same heading,
same blank lines — and a worker that writes something else keeps its own bytes.
That is the intended latitude, not a discrepancy: the format is what Rhei
appends, not a validator applied to what a worker wrote.

Example result file after a task completes:

```markdown
## Result

Added avatar_url column and migration 0042
```

## 4. Behavior

`rhei complete <ticket>` with either result source is exactly:

> the readiness checks a scheduling verb owns (points 4–6) + the inferred
> one-hop terminal target (§4.1) + `rhei transition <ticket> --from <current>
> --to <inferred> --result <selected-message>` on the shared transition path.

1. Select exactly one result source, read file or stdin input when selected,
   decode it as UTF-8, and reject an empty or whitespace-only message before
   the plan, machine, task, or runtime artifacts are read or written. These are
   input checks, not plan checks, so their diagnostic takes precedence even
   when the plan path or ticket is also invalid. Any failure in this phase
   leaves the plan state, result artifacts, and transition ledger unchanged.
   The accepted message itself is not normalized (§2.2).
2. Load the state machine and plan (single file or directory workspace). Validate.
3. Locate the task by ID. Fail if the task does not exist.
4. Reject if the task is already in a terminal state.
5. Reject if the task's current state is a [gating state](rhei-states.spec.md#12-per-state-fields) (`gating: true`) — those can only be exited by an explicit human-initiated `rhei transition`.
6. Reject if any `**Prior:**` of the target task is unsatisfied — resolved
   across the whole project graph and judged the same way readiness judges it
   (terminal-and-not-cancelled, [§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)). The error names every
   blocking prior with its current state. Completing a ticket ahead of its
   prerequisites contradicts the dependency semantics the plan declares, and
   nothing downstream would ever surface it: the ticket becomes terminal, so
   `rhei list --blocked` stops reporting it and the plan reads as healthy.
   A deliberate out-of-order move stays available through the explicit
   human-initiated `rhei transition` ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)), the same
   escape hatch a gating state uses in point 5.
7. Find the completion target: the first non-cancelled terminal state reachable via a declared transition from the current state. Fail if none exists (e.g., from `agent-review-fix` there is no direct path to a terminal state — the agent must transition to `agent-review` first). Any state with cancellation role (§FS-rhei-states.1.4), explicit or inferred from `cancelled`/`canceled`, is excluded from successful completion. The order of transitions in the YAML `transitions` list is significant when selecting the target; editors and formatters should preserve declaration order.
8. Run the shared transition ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)) from the current state
   to that target, carrying the selected message as `--result` does:
   compare-and-swap under the shared
   canonical sibling sidecar while the plan destination remains unlocked,
   the descendants-first guard ([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)), `on_leave`,
   source `outputs:`, target `inputs:`, the terminal-result obligation
   ([§FS-rhei-transition-cmd.3.2](rhei-transition-cmd.spec.md#32-terminal-result-on-entry)), the atomic state write, `on_enter`, the
   ledger line, and the terminal finalization of §3 — in the artifact order
   defined in [Plan Language Specification — State Artifact Contracts](rhei-plan-language.spec.md#310-state-artifact-contracts).
   `rhei complete` does not re-implement any of it and appends exactly one
   result entry per invocation.
9. If callbacks redirect the transition, the effective target must still be a
   non-cancelled terminal completion state; otherwise the command fails. The
   redirect is the machine's decision and has already been applied by the time
   this is known — so has the ledger line, and so has the caller's `--result`,
   wherever the redirect sent the ticket. **This holds for every redirect, not
   only a terminal one.** A `complete --result "…"` whose `on_leave` sends the
   ticket to `review` leaves it in `review` with that message recorded as a
   `## Result` entry, and exits non-zero because the ticket is not finished.
   Two things follow, and both are intended:
   - The move happened and the ledger has it, so the worker's account rides
     with the move rather than being thrown away with the command's exit code.
     This is exactly what `rhei transition --result` on a non-terminal hop
     does ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result), item 1) — results accumulate as `## Result`
     entries by design, and `complete` holds no private variant of the rule.
   - That recorded message then **satisfies the terminal-result obligation at
     the eventual terminal edge**, as any earlier `transition --result` on the
     same ticket does. The ticket will not be refused later for having no
     result; it will carry this one, ahead of whatever the redirected-to state
     goes on to produce.

   A ticket redirected to `cancelled` is the same story with a terminal ending:
   left cancelled *with the caller's message recorded against it*, which is the
   outcome the terminal-result obligation asks for, and `rhei complete` still
   exits non-zero because the caller asked to complete a ticket that was
   instead abandoned.

Everything `rhei complete` used to hold privately now lives on the shared path,
so the same edge driven by `rhei complete --result`, by `rhei transition
--result`, or by `rhei run` (with the worker's result on disk) leaves an
identical ledger line, result file, `> **Result:**` link, and absent
`**Assignee:**`.

State history itself stays in the central ledger: a non-terminal `rhei
transition` writes only `runtime/state-transitions.log`, and creates no
per-task result file when no message was carried.

**Note on child nodes:** In the current hierarchical node model, child nodes
are full stateful task nodes, and so is their parent — completing a parent is
completing a task, not rolling up its children. The descendants-first rule that
governs it is specified once, on the shared transition path
([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)), and is not restated here: `rhei complete` holds
no private copy of it, so the same plan state is refused identically whichever
verb drove the terminal edge.

### 4.1. Completion Target Selection

The command scans declared transitions for a non-cancelled terminal state reachable in one hop from the task's current state. If multiple terminal states are reachable, the first non-cancelled one wins. If only cancellation-role terminals are reachable, the command fails (§FS-rhei-states.1.4).

### 4.2. Single-File Plans

The result file is written relative to the plan file's parent directory. The state change, assignee removal, and result link are applied in the plan file itself.

### 4.3. Directory Workspaces

The result file is written relative to the workspace root. The state change, assignee removal, and result link are applied in the task file under `tasks/`.

## 5. Output

```text
Task <ID> completed: '<from>' -> '<to>' (runtime/results/<ID>.md)
```

## Examples

```bash
# Agent finishes work on task 3 of the `plan` rhei
rhei complete plan.rhei.md --task 3 \
  --result "Added avatar_url column and migration 0042"
# State: pending -> completed
# Result: runtime/results/plan.3.md
# Assignee: removed

# Preserve a Markdown-heavy or multiline result without putting it in argv
rhei complete plan.rhei.md --task 3 --result-file result.md

# The same safe input can be streamed to EOF
generate-result | rhei complete plan.rhei.md --task 3 --result-file -

# Worker in a living workspace completes a review-seed task. The Directory
# Workspace `my-workspace/` is the rhei `my-workspace`, so the ticket is
# `my-workspace.review-seed` however the target was written.
rhei complete ./my-workspace --task review-seed \
  --result "Wrote 4 findings to runtime/findings/consolidated.md"
# State: pending -> completed
# Result: ./my-workspace/runtime/results/my-workspace.review-seed.md
# Task body: > **Result:** [my-workspace.review-seed](runtime/results/my-workspace.review-seed.md)
```

## Relationship to Other Commands

`rhei complete` is the terminal step of the manual-worker loop: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release). It transitions the task to a terminal state, appends a result entry, and releases the claim.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — grammar including `assignee_field` and `result_block`
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Transition Command](rhei-transition-cmd.spec.md) — `rhei transition` behavioral contract
- [Run Command](rhei-run.spec.md) — `rhei run` behavioral contract
- [Reset Command](rhei-reset.spec.md) — `rhei reset` behavioral contract
