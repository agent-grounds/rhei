# FS-rhei-task-tooling: Tooling One Task Needs

A task may name the MCP servers and skills that its own agent invocations need,
with the task metadata fields `**MCP servers:**` and `**Skills:**`. A state
still attaches tooling to a phase (§FS-rhei-states.7), and every task in that
state gets it. A task's entries are added on top of that, for that task's
invocations only, in every agent state it passes through. The reason is the one
§FS-rhei-plan-language.3.11 gives for `**Model:**` and `**Target:**`:
capability is a property of the work item, while the agent is a property of the
phase. A server one ticket needs, such as a personal mail account, can then
reach that ticket alone, without being handed to every sibling in its state and
without forking a state machine that many tickets share.

A state can refuse what tasks add with `withhold_task_tooling` (§FS-rhei-task-tooling.4). It runs
without the task's entries and does not reject the plan.

Nothing changes for a plan that writes neither field and a state machine that
does not write `withhold_task_tooling`: every invocation gets the set it got
before. The one compatibility edge is the closed metadata block
(§FS-rhei-plan-language.2). A rhei built before these fields existed refuses a
plan that uses them as an unknown metadata field.

## 1. The Fields

```markdown
### Task 1: Summarise this week's release thread from the mail
**State:** pending
**MCP servers:** thunderbird-mail, grafana (optional)
**Skills:** release-notes
```

Each field may appear once. Both come last in the metadata block: after
`**Model:**` and `**Target:**`, the other fields that say what the agent works
with, and `**MCP servers:**` before `**Skills:**`. The value is a list separated
by `, `, the list form `**Provides:**` uses. Each entry is a registry id,
spelled exactly as the key in the merged `mcp_servers`
(§FS-rhei-agents.1.1.4) or `skills` (§FS-rhei-agents.1.1.5) settings registry.
An id may contain no whitespace, `,`, `(` or `)`. An entry followed by
` (optional)` is optional, with the meaning `optional: true` has on a state
entry (§FS-rhei-agents.6.1); without it, the entry is required. The grammar is
in §FS-rhei-plan-language.2.

The fields take registry ids only, never an inline server or skill object. A
plan is reviewed like any other file in the repository. Ids keep launch
commands, URLs and environment variables in settings, out of the plan.

The optional marker is not the `?` a run log writes. In the log, `?` means
"optional and failed to start" (§FS-rhei-agents.8.2). And an unquoted `?` is a
glob character in common shells, so `rhei new --mcp-server grafana?` could fail
before rhei ever saw it.

These lines are parse errors, and each one names the field:

- an empty value, or an empty entry between two separators;
- the same field twice in one block;
- the same id twice in one field;
- an entry that is not an id, or whose text after the id is anything but
  ` (optional)`: `grafana (optinal)`, `grafana(optional)`, `thunderbird mail`;
- either field before `**State:**`, `**Skills:**` before `**MCP servers:**`, or
  any other metadata field after either of them.

As for any recognized field, a blank line does not close the metadata block
for these fields, and either field after task content is a parse error.

## 2. Where They Apply

The entries apply in every state where an agent runs the task, on every visit,
and they are re-read immediately before each spawn. They follow the work item
through its lifecycle, as `**Model:**` and `**Target:**` do.

- Program, gating and final states run no agent, so they ignore the fields
  without error. A ticket that names a server still passes through them.
- A fanout state (`all_targets` or `all_models`) applies the entries to every
  member. Unlike `**Model:**` and `**Target:**`, the fields name no execution
  identity, so nothing about fanout contradicts them.
- A `target_locked` state receives task tooling. It locks the identity, not the
  tools.
- Child tasks do not inherit them. Each task names what its own invocations
  need.

Every surface that reads an invocation's resolved set reads the task's entries
with it, for that task's invocations only: the `--mcp-config` file
(§FS-rhei-mcp-config-file), the `mcp_flag` and `skill_flag` arguments,
`RHEI_MCP_SERVERS` and `RHEI_SKILLS`, the `RHEI_MCP_<NAME>_AVAILABLE` and
`RHEI_SKILL_<ID>_AVAILABLE` variables, the required-or-optional gate (§FS-rhei-task-tooling.5), and
the log header (§FS-rhei-task-tooling.7). A sibling task in the same state gets none of them.

A state's instructions cannot branch on an id that only a task adds. A
`{if mcp.<name>.available}` condition (§FS-rhei-states.4) must still name an id
the state declares, because a state machine is validated without any plan.

## 3. The Effective Set

One invocation's set of each kind is built in three steps:

1. `defaults.mcp_servers` (or `defaults.skills`) from the merged settings;
2. then the state's list, unchanged: a state entry wins over a defaults entry
   with the same id, and a state's `[]` clears the defaults
   (§FS-rhei-states.7.2);
3. then the task's entries, unless the state withholds them (§FS-rhei-task-tooling.4).

The task step comes last, matching where task metadata sits when the agent and
model are resolved (§FS-rhei-agents.1.4). A state's `[]` clears only the
defaults, so a task's entries still arrive in a state that writes it.

When a task names an id the set already holds, the entry already there keeps its
definition, inline or from the registry, and it is optional only if both sides
mark it optional. So a task can add a server, or insist on one the state made
optional. It can never let an agent start without a server the state machine
or the defaults require. Every handler a machine author wrote for their own
servers keeps firing as it does without the task.

An agent profile with no MCP or skill wiring (§FS-rhei-agents.6.1) treats a
task entry as it treats a state entry: a required one blocks the spawn, and an
optional one is dropped with a warning. A ticket that names a server and passes
through such a state should mark it `(optional)`, or that state should withhold
it.

## 4. A State That Withholds

```yaml
states:
  review:
    agent: claude-code
    mcp_servers: [postgres]
    withhold_task_tooling: true
```

`withhold_task_tooling` is a boolean state field, and it defaults to `false`.
When it is `true`, the state's invocations skip step 3 of §FS-rhei-task-tooling.3: the task's
servers and skills are dropped, both kinds together. The state's own list and
the defaults resolve exactly as they would otherwise. An id the state or the
defaults supply is not withheld, because the invocation still gets it.

It withholds; it never rejects. A task that names a server still passes through
a withholding state, and validation says nothing about it: that is the state
machine working as written, and a warning no plan author could act on would
only be noise. The run log records what was held back (§FS-rhei-task-tooling.7).

The field is refused on a program, gating or final state, as `mcp_servers` is,
and it must be a boolean (§FS-rhei-states.1.3).

What is withheld is what rhei hands over: rhei does not write a withheld server
into the `--mcp-config` file or the `mcp_flag` arguments, does not pass a
withheld skill with `skill_flag`, and does not list either in the environment.
An MCP server an agent registers through its own configuration was never handed
over by rhei, so it is not withheld here.

Four choices are deliberate:

- **A field of its own, not a new meaning for `mcp_servers: []`.** `[]`
  already means "do not inherit the defaults". An author who meant only that
  would silently get "no task may add anything" too.
- **Withhold, not reject.** A ticket that needs a mail server must still be
  able to pass through a review state that must not read mail. A rejecting
  state would leave it unable to run at all.
- **Not named `*_locked`.** `target_locked` rejects the plan, so a name
  borrowed from it would promise a refusal that never comes.
- **Servers and skills together.** They are one tooling surface everywhere
  else in the specification, and nothing here needs them split.

## 5. A Required Task Entry That Fails

A task entry that fails its availability check is handled exactly as a state
entry is (§FS-rhei-agents.6.1). A required one blocks the spawn. The engine
then evaluates the current state's `mcp_unavailable` or `skill_unavailable`
transitions (§FS-rhei-transitions.3.7). `true` matches the failure, and an id
list matches only if it names the failed id. If a transition matches, it fires
with the id in `transitionData.unavailable`. If none matches, the task stays
where it is and the error is logged. An optional task entry that fails is
dropped with a warning, and the log writes it with `?`.

The spec defines `true` as "any failure of that kind", and a task entry is no
exception. The alternative, treating every task entry as optional, would make
`(optional)` meaningless, and it would start an agent without a server its
ticket said it needs.

A state machine written before these fields sees nothing new until a ticket
names a server. From then on, an `mcp_unavailable: true` handler can fire for
an id the machine never named. Handlers with an id list behave as they did.

## 6. Validation

`rhei validate`, and every command that validates a plan before it runs,
resolves each task's `**MCP servers:**` ids against the merged `mcp_servers`
registry and its `**Skills:**` ids against the merged `skills` registry, once
per task, through the same lookup a state's list goes through
(§FS-rhei-validate.4). An id with no registry entry is an error that names the
task, the field and the id, in the form an unknown id on a state takes:

```text
Task plan.1 **MCP servers:** references unknown mcp server 'thunderbird-mial'
```

The check is per task and does not depend on the states the task passes
through. An id a withholding state will drop must still resolve.

## 7. What a Reader Sees

**The run log.** The `mcp_servers:` and `skills:` lines of an invocation's log
header (§FS-rhei-agents.8.2) list what that invocation got, task entries
included. A missing line means the invocation got none of that kind. When a
state withheld task entries (§FS-rhei-task-tooling.4), the header adds `mcp_servers_withheld:` or
`skills_withheld:` after them, naming each id the invocation did not get
because of it. The line is written only when something was withheld, so the
log format stays at `v1`, as it did when `family:` was added.

```
mcp_servers: postgres
mcp_servers_withheld: thunderbird-mail
```

**`rhei render`.** `--format json` gives each task `mcp_servers` and `skills`
arrays of `{ "id": "<id>", "optional": <bool> }` in authored order, empty when
the field is absent, following the convention `provides`, `consumes` and
`excludes` use (§FS-rhei-render.3.1). `--format github` keeps the lines in
grammar order, and `--no-metadata` drops them with the rest of the metadata.

**`rhei states`.** A state that authors `withhold_task_tooling: true` prints
`Task tooling: withheld` (§FS-rhei-states-cmd.4). `--json` carries
`"withhold_task_tooling"` only on a state that authors it, with the authored
value (§FS-rhei-states-cmd.5), so the output for a machine that does not write
the field is unchanged.

`rhei list`, `rhei show`, `render --format progress` and `rhei run --dry-run`
show no tooling today, and they show none for a task either.

## 8. Writing the Fields With `rhei new`

`--mcp-server <ENTRY>` and `--skill <ENTRY>` write the two fields
(§FS-rhei-new.1.3). Each flag is repeatable, and each value is one entry in
the field's own form, read by the same entry parser: `--mcp-server grafana` or
`--mcp-server 'grafana (optional)'`. The entries are written on one line, in
the order given, after any `**Model:**` or `**Target:**` line. An id with no
registry entry is refused before any file is written, as validation would
refuse it (§FS-rhei-task-tooling.6). A `--description` line that opens with either field's marker is
refused like every other metadata marker (§FS-rhei-new.3.4).

Claiming a task is unchanged. `rhei next` writes `**Assignee:**` before
`**Model:**` and `**Target:**`, so it also lands before the two new fields, and
`rhei complete` removes it from the same place.
