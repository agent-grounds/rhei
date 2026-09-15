# FS-rhei-next: `rhei next`

Select and optionally claim the next eligible task from a plan.

## 1. Usage

```bash
rhei next <RHEI_PLAN> [--peek] [--task <ID>] [--rhei <RHEI_ID>]
```

## 2. Options

| Flag               | Required | Default | Description                                                        |
|--------------------|----------|---------|--------------------------------------------------------------------|
| `--peek`           | No       | false   | Print the next claimable task without transitioning it             |
| `--task <ID>`      | No       |         | Target a specific ticket instead of auto-selecting. See §2.1.      |
| `--rhei <RHEI_ID>` | No       | all     | Narrow candidate selection to the named rheis (repeatable). §2.2.  |

### 2.1. Ticket Targets

`--task` accepts either the project-qualified ticket id (`auth.1`) or a
rhei-local shorthand (`1`). A shorthand resolves only when exactly one in-scope
rhei contains that ticket; when more than one does, the error names the
qualified candidates. Output, artifacts, and ledgers always use the qualified
id regardless of how the target was written ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

A ticket named explicitly with `--task` must itself be in scope: targeting a
ticket outside `--rhei` is an error rather than a silent widening.

### 2.2. Project Scope (`--rhei`)

`rhei next` reads the whole project by default, since every load yields a
Panta-rooted graph and a bare rhei is the single rhei of its implicit Panta.
`--rhei <RHEI_ID>` is repeatable and narrows which tickets are **candidates**;
it never narrows where their priors resolve, so a candidate may still be
blocked by a prior in a rhei outside the scope. The no-work diagnostic names
the scope and marks such a prior as out of scope ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)).

An id that names no rhei in the project is an error listing the available rhei
ids. Claim mode writes `**Assignee:**` into the owning rhei's file, resolved
through the source map.

## 3. Default Behavior (Claim Mode)

Without `--peek`, `rhei next` atomically claims the next claimable task: it assigns the task to the current agent and prints the task instructions. The task's state is **not** advanced — the agent works in the current state and uses `rhei transition` or `rhei complete` to advance when ready. This is the standard entry point for agents beginning work.

Initial states are not all treated the same: an initial state that declares
runnable autonomous work (`program`, `agent`, `target`, `all_targets`, `model`,
or `all_models`) is claimed and presented in place. A non-runnable initial
state is auto-advanced only when its first applicable forward transition targets
another non-terminal state. If its first applicable forward transition targets
a terminal state, `rhei next` claims and presents the initial state in place so
the agent can do the work before `rhei complete` finishes it. This keeps the
built-in `pending` -> `completed` machine claimable without completing work at
claim time.

A task is *claimable* when:

1. Every descendant task node it has — child, grandchild, or deeper — is in a
   terminal state. A leaf task node satisfies this trivially.
2. All tasks listed in its `**Prior:**` field are in successful terminal
   states (`final: true` and not the normalized `cancelled` state).
3. The task has no `**Assignee:**` field (not already claimed by another agent).
4. Its current state is not terminal (`final: true`) and not gating (`gating: true`).
5. All required `inputs` declared on the task's current state exist.
6. No `poll:` deadline stands in the way: if the current state declares a
   `poll:` block and `metadata.tasks.<id>.pollNextAttemptAt.<state-name>` is
   later than the current wall-clock time, the task is not claimable until the
   interval elapses ([§FS-rhei-states.2](rhei-states.spec.md#2-polling-states)). This is the same exclusion `rhei run`
   applies to the ready set ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)).
7. Its current state is the machine's initial state for that task node. Claim
   mode enters a ticket at the top of its workflow; one already partway through
   is being worked by whoever holds it, so automatic selection passes over it.
   `--task` names a ticket explicitly and bypasses selection (§3.4).

Rules 1, 2, 4, 5 and 6 are the *ready set* — the one definition `rhei run`
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)) schedules from and `rhei list --ready` reports
([§FS-rhei-list.3.1](rhei-list.spec.md#31-what---ready-means)). Rules 3 and 7 are this command's own narrowing, about
availability rather than readiness, which is why a ticket `rhei list --ready`
names can still be one `rhei next` declines to claim.

Rule 1 is the eligibility half of the non-leaf model
([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)): a non-leaf task node is a task in its own right —
it owns its state, its work, and its result — and nothing advances it on its
children's behalf, so it must be claimable. It becomes claimable exactly when
its subtree is terminal, which is also when `rhei run` will schedule it
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)). Until then a parent and its own descendant are never worked
at the same time.

Rule 1 governs `--task` too; see §3.4.

Rule 1 has one declared refinement. A task in a *supervising* state is
claimable while its subtree is held and nothing beneath it is in flight, and a
descendant of a supervising task is claimable only while every supervising
ancestor has released it ([§FS-rhei-supervision.3.2](rhei-supervision.spec.md#32-readiness)). Such a descendant is
reported as held by its supervisor rather than as blocked.

### 3.1. Behavior

1. Load the state machine and plan. Validate.
2. Scan every task node in plan order, leaf and non-leaf alike. For each task
   that satisfies the descendant, dependency, assignee, and state eligibility
   rules above, resolve the current state's required `inputs`.
3. If any required input file for the first otherwise-claimable task is
   missing, stop immediately and fail with an explicit missing-artifact error.
   Do not skip ahead to later tasks.
4. Select the first candidate in plan order.
5. Acquire a file lock on the plan file.
6. Re-read and re-validate the task's claimability under the lock, including
   re-checking required `inputs` (guards against concurrent claims and moved
   files). The re-read parses the selected task's file under the same node
   kinds the scan did. In a directory workspace those are the kinds the
   workspace index declares in `structure.nodeKinds`
   ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)),
   and for a basin ticket, the basin having no index of its own, the kinds the
   project manifest declares
   ([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)) —
   never the omitted-`structure` default of `Task` alone. A plan whose declared
   kinds omit `task` is therefore claimed exactly like one that declares it: a
   task the scan selected is never lost at re-read, and the re-read rejects a
   heading keyword the plan did not declare.
7. If the selected task is in a non-runnable initial state whose first
   applicable forward transition targets another non-terminal state, send that
   edge and the claim intent through the shared transition executor. The
   executor applies `on_leave`, validates a redirect before persistence,
   validates source outputs and effective-target inputs
   ([§FS-rhei-transitions.4.5](rhei-transitions.spec.md#45-artifact-enforcement)),
   and resolves the assignee from the effective non-terminal state. A leave
   rejection leaves the source untouched, and a redirect to a terminal state
   remains a refusal rather than finishing work during a claim. Otherwise keep
   the task in its current state and resolve the assignee there.
8. Set `**Assignee:** <current-agent>` on the task, where `<current-agent>` is
   the agent id resolved for the rendered state via the
   [agent resolution order](rhei-agents.spec.md) (state `agent:` field →
   project settings → global settings). When no agent is configured, write
   the reserved assignee value `manual` so the task still leaves the claimable
   set durably and concurrent `rhei next` calls cannot claim it twice.
9. For an auto-advancing claim, retain the original task and metadata bytes and
   the transition ledger's pre-append length. Write the effective state and
   metadata atomically, run `on_enter` against that target-state view, write the
   resolved assignee while the claim still owns the task, then append exactly
   one transition entry. Ledger appends are serialized so this append can be
   reversed without truncating a different writer's successful entry. The
   state, ownership, and transition entry together establish the claim's
   commit boundary. For an already-runnable initial task, revalidate under the
   lock and atomically write its assignee once; staying in the same state
   creates no transition entry.
10. Release the lock, resolve the task's exclusions, then build the state's effective prompt text from its selected
   `prompt_template`, if any, plus inline `instructions` and `personality`,
   then resolve runtime template variables (see
   [Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)
   and [Prompt Templates](rhei-states.spec.md#44-prompt-templates)).
    Apply the same source-payload filter as `rhei run` before any bytes are
    read ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)). Because `rhei next` does not spawn the worker, its
    `## Exclusions` section always reports `composition only; paths remain
    readable outside Rhei-composed context`, even when the selected profile has
    a `deny_read` adapter.
11. Print the task id, title, current state, and resolved instructions to stdout.

If no claimable task exists, print a status summary (see [No Tasks Ready](#5-no-tasks-ready)).

An auto-advancing claim is not committed until its effective state, resolved
ownership, and transition bookkeeping have all been recorded. An ordinary
returned artifact-validation, state-persistence, assignee-persistence, or
ledger error before that boundary restores the original task and metadata
bytes and the original ledger contents before releasing the claim lock. This
includes a failure after ledger preflight and a partial ledger append. The task
is then unchanged and retryable after the cause is removed; `rhei reset` is not
required.

An `on_enter` failure first restores the source state and original ownership,
then follows the configured recovery contract
([§FS-rhei-transitions.4.9](rhei-transitions.spec.md#49-error-handling-configuration)).
When that contract commits a declared recovery transition, the recovery's own
state and bookkeeping are the outcome. Rhei does not undo arbitrary external
callback effects. If a second, independent I/O error prevents restoration, the
diagnostic reports both errors and every affected path and does not claim that
the task is retryable. Abrupt process or machine termination is outside this
ordinary-return guarantee.

Prompt construction and output happen after the commit boundary. A rendering
failure therefore leaves the durable claim in place. `--no-callbacks` changes
only callback execution; `--peek`, `--task`, and `--rhei` keep the selection,
read-only, and scope behavior defined here. The claim intent is exclusive to
`next`: ordinary `transition`, `complete`, dashboard human-gate transitions,
and `run` retain their existing persistence, callback, recovery, and output
contracts, sharing only serialized ledger appends.

### 3.2. Output (claim mode)

The first line reports the claim, because taking the claim is what the command
just did. When the claim also advanced the state it names both; when the ticket
stays put it names the assignee written:

```text
Task auth.1 claimed: 'draft' -> 'pending'
Task auth.1 claimed by manual (stays in 'pending')
```

Both names are the state as the **machine** declares it, with the `-<visit>`
suffix a counted loop writes into `**State:**` dropped (§4.1).

The second form is not an edge case — it is *every* claim under the built-in
machine, whose initial state is also its working state (§3). Reporting only
`Task auth.1 (already in 'pending')` there described the state the caller could
already see and said nothing about the `**Assignee:**` write, which is the
whole point of claiming: the durable mark that stops a second worker taking the
same ticket. `rhei release` already announces the mirror-image action
(`Released Task auth.1 (was assigned to manual)`).

In `--json`, the same fact is a `claimed_as` field, present exactly when this
invocation took the claim — absent under `--peek` and absent when the ticket was
already assigned, so a scripted worker can tell a claim from a look without
re-reading the plan.

Prompt templates are expanded from `prompt_template.values` before runtime
variables, and inline `instructions` / `personality` are appended after
selected template text. Runtime variables in the effective prompt are resolved
before output. See
[Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)
for the full variable namespace and resolution rules.

```text
Task <ID>: <title>
State: <current-state>

<resolved effective instructions from state definition and selected prompt template>
```

### 3.3. Missing Artifact Error

If the task that would otherwise be claimed is missing one or more required
input artifacts for its current state, `rhei next` fails and prints an explicit
error instead of silently skipping the task.

Example:

```text
Error: Task auth.review-cache-key cannot be claimed in state agent-review-fix.
Missing required input artifact: findings (runtime/findings/auth.review-cache-key.md)
```

### 3.4. Claiming a Non-Leaf Ticket with `--task`

`--task` names a ticket explicitly and bypasses selection, but not the
descendant rule (§3, rule 1). Targeting a non-leaf ticket whose subtree is
still open fails, naming every open descendant and pointing at what is
claimable instead:

```text
Error: Task plan.2 cannot be claimed while 1 descendant task(s) are still open.
       Open descendants: Task plan.2.3 (pending)
  help: claim what is ready instead: rhei next plan.rhei.md --task plan.2.3
```

The help names the first ticket that *is* claimable, so the refusal ends in a
runnable command rather than in an explanation. When nothing else is claimable
it says so and points at `rhei list`.

Once every descendant is terminal the refusal no longer applies and the parent
is claimed like any other ticket. There is no cascade to wait for: nothing
advances a parent when its children advance ([§FS-rhei-plan-language.3](rhei-plan-language.spec.md#3-semantic-constraints)), so a
refusal that told the caller to wait for the children to "advance the parent"
would describe a mechanism that does not exist.

Claiming does not advance state (§3), so claiming a parent is exactly that —
taking the ticket. `rhei transition` and `rhei complete` move it afterwards,
both subject to the descendants-first guard on the shared transition path
([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)).

## 4. Peek Mode (`--peek`)

With `--peek`, `rhei next` performs a read-only scan and prints the next task that *would* be claimed, without modifying the plan or acquiring a lock. This is safe for PM-style navigation, scripting, and inspection.

Peek mode does **not**:

- Acquire a file lock
- Modify any state
- Append to result files
- Set or clear `**Assignee:**`

Peek mode still resolves required `inputs` for the first otherwise-claimable
task. If any are missing, `--peek` fails with the same missing-artifact error as
claim mode.

### 4.1. Output (peek mode)

Peek prints what claim mode prints, minus the claim: the same heading, the same
instructions, and the same context sections, under a first line that says the
ticket was not advanced.

```text
Task <ID> — current state: '<state>' (read-only peek; not advanced)
Agent: <agent>  |  Model: <model>

Personality: <the state's personality, when it declares one>

## Task <ID>: <title>

<the ticket's body, when it has one>

  - <Kind> <child-ID>: <child title> [<child state>]

--- Instructions (<state>) ---
<the state's instructions, template variables resolved>
```

`<state>` is the state in its machine form: the `-<visit>` suffix a counted
loop writes into `**State:**` ([§FS-rhei-plan-language.3.2](rhei-plan-language.spec.md#32-state-validity)) is dropped in the
first line and in the instructions banner alike. That is the only form
`rhei transition --from` accepts and the form `## Position` prints, so one
screen spells a state one way; the visit is shown in `## Position` instead.
`--json` is unchanged and still reports the authored value in `from_state` and
`state`.

The `Agent:` line is printed only when an agent or model resolves, and the
`Personality:` block only when the state declares one. After the instructions
come the mid-term memory sections in the run prompt's order — `## Position`,
`## Plan History`, `## Previous Visits`, and `## Rhei Navigation` — each
omitted exactly when the run prompt would omit it ([§FS-rhei-memory.5](rhei-memory.spec.md#5-surfaces)), together
with the supervision sections that already travel this way
([§FS-rhei-supervision.3.4](rhei-supervision.spec.md#34-manual-workers)).

If no claimable task exists, the same status summary is printed as in claim mode.

## 5. No Tasks Ready

When no claimable task is found, `rhei next` (with or without `--peek`) prints a
status message that explains why no claim was possible and what the next human
action is:

| Condition | Message |
|-----------|---------|
| All tasks in terminal states | `Plan complete. All <N> task(s) are in terminal states.` |
| One or more otherwise-ready tasks are in a gating state | `Blocked: <N> task(s) waiting on human action: Task <ID> (<state>), ...` |
| All otherwise-ready non-terminal tasks are claimed | `No tasks available to claim. <N> task(s) are currently in progress: Task <ID> (<state>, assignee <ASSIGNEE>), ...` |
| A ready task is mid-workflow rather than in its profile's initial state | `No tasks can be auto-claimed: Task <ID> is mid-workflow in state '<state>'. Pick one of its outgoing transitions explicitly.` followed by one `rhei [--state-machine=<states>] transition <plan> --task <ID> --from=<state> --to=<target>` command per currently applicable outgoing transition, with shell quoting applied to copied arguments |
| Non-terminal tasks are blocked by prerequisites | `no tasks are ready to claim: <N> task(s) blocked by incomplete prerequisites: Task <ID> waiting on Task <PRIOR> (<state>), ...` |
| Non-terminal tickets are held by a supervisor whose visit is pending or in flight ([§FS-rhei-supervision.3.4](rhei-supervision.spec.md#34-manual-workers)) | `no tickets are ready to claim: <N> ticket(s) held by a supervisor: Task <ID> held by supervisor Task <P> (<state>), ...` |
| Under `--rhei`, in-scope tasks are blocked by prerequisites; a blocking prior outside the scope is marked as such ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)) | `no tasks are ready to claim in the --rhei scope (<ids>): <N> task(s) blocked by incomplete prerequisites: Task billing.2 waiting on Task auth.1 (pending, outside the --rhei scope).` |
| Under `--rhei`, all in-scope tasks are in terminal states | `Scope complete. All <N> task(s) in the --rhei scope (<ids>) are in terminal states.` |

Every row speaks about a task the caller can act on directly, so the categories
are computed over *workable* tasks — leaves, plus non-leaf tasks whose subtree
is already terminal (§3, rule 1). A parent therefore never masquerades as
gated, in-progress, or mid-workflow while its children are the real work. No
row is needed for a parent whose subtree is open: some non-terminal leaf sits
under it, and that leaf is workable, so one of the rows above already names the
actionable ticket.

There is no "leaf work complete, rollups remain" message. It existed because a
parent could never be claimed, so the only way to surface one was to wait until
every leaf in scope was terminal and then name it. Under the eligibility rule a
parent whose subtree is terminal is simply claimable, and `rhei next` returns
it as the next ticket — mid-plan, as soon as its own children finish, without
waiting for unrelated branches. A dependent still blocked on such a parent is
reported by the prerequisite row, which names the parent and its state.

These distinct messages allow a PM or orchestrator to tell apart a finished
plan, a human gate, fully in-flight work, manual transition selection, and
ordinary prerequisite blocking. See [States Specification — State
Definition](rhei-states.spec.md#12-per-state-fields) for the `gating: true`
field (e.g., `human-review` in the default machine; custom machines may define
additional gating states such as `security-review` or `legal-review`).

## Relationship to Other Commands

`rhei next` is the claim step of the manual-worker loop: `next` (claim) → work → `transition` (advance as needed) → `complete` (finish, record result, release). `--peek` is the read-only variant that inspects the next claimable task without taking it.

See [How Rhei Is Used — Command Surface](rhei-usage.spec.md#22-command-surface) for the full table comparing all five coordination commands.

## 6. Agent Context

When a state declares an `agent` field (or an agent is resolved from project/global settings), `rhei next` includes the agent identifier in its JSON output:

```json
{
  "task_id": "auth.3",
  "title": "Implement caching layer",
  "state": "pending",
  "agent": "claude-code",
  "model": "impl-fast",
  "model_provider": "anthropic",
  "model_name": "claude-sonnet-4-6",
  "instructions": "..."
}
```

The `agent`, `model`, `model_provider`, and `model_name` fields are omitted
from JSON output when no agent or model is configured.

The mid-term memory sections `rhei run` composes travel the same way: the text
output renders them after the instructions, and JSON carries each as a string
field named after its section — `position`, `plan_history`,
`previous_visits`, and `navigation` — present exactly when the run prompt
would carry that section. [§FS-rhei-memory.5](rhei-memory.spec.md#5-surfaces)

In text output mode, the agent is shown after the state line:

```text
Task auth.3: Implement caching layer
State: pending
Agent: claude-code (impl-fast = anthropic/claude-sonnet-4-6)

<resolved instructions>
```

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) — roles and coordination patterns
- [States Specification](rhei-states.spec.md) — state machine format
- [Agents Specification](rhei-agents.spec.md) — agent configuration, invocation, and timeout
- [Mid-Term Memory](rhei-memory.spec.md) — the prompt sections a manual worker gets too
- [Transitions Specification](rhei-transitions.spec.md) — state transition system
- [Transition Command](rhei-transition-cmd.spec.md) — `rhei transition` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Run Command](rhei-run.spec.md) — `rhei run` behavioral contract
- [Release Command](rhei-release.spec.md) — dropping a claim this command wrote
- [Reset Command](rhei-reset.spec.md) — `rhei reset` behavioral contract
