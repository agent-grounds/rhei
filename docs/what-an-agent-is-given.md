# What an Agent Is Given

When `rhei run` starts an agent on a task, what that agent gets is put
together from up to five places. These are the agent and model it runs as,
the MCP servers and skills it can use, and how long it may work. This page
says which place wins when two of them set the same thing, and what happens
when a server the agent needs cannot be reached.

| Place | Where it is written | Who writes it |
|-------|---------------------|---------------|
| Machine settings | `~/.config/rhei/settings.json` | whoever runs this machine |
| Project settings | `.agent-grounds/rhei/settings.json` | the project |
| State | the state's entry in `states.yaml` | the state machine's author |
| Task | the task's metadata lines in the plan | the plan's author |
| Run | flags to `rhei run` | whoever starts the run |

There are two rules, one for a setting the agent can have only one of and one
for a list. Under neither is a disagreement an error: one place wins, or the
entries add up. The only refusals are the three task overrides listed below.

## A Single Value: the Most Specific Place Wins

For each of these, the first place in the order that sets it is the one used:

| Setting | Order, first one set wins |
|---------|---------------------------|
| Agent and model | `--agent` / `--model`, then the task's `**Target:**` or `**Model:**`, then the state's `target` (or its `agent` and `model`), then the project's `defaults`, then the machine's `defaults`. An agent left unset falls back last to the model's `default_agent`. |
| Mode | `--agent-mode`, then the state's `agent_mode`, then `defaults.agent_mode` (project before machine), then the first mode the agent declares. When the identity comes from a `target`, an `all_targets` selector or a task's `**Target:**`, the mode is the one the selector names. |
| Effort | The state's `effort`, and nothing else. A task's `**Model:**` or `**Target:**` keeps the state's effort. |
| Agent timeout | The state's `agent_timeout`, then the timeout of the model's binding for that agent, then the agent profile's `timeout`, then `defaults.agent_timeout`. |

`--agent` and `--model` replace only their own part of the identity, so
`--model` on a state whose target is `claude-code:anthropic:opus` keeps
`claude-code` and the provider. A fanout state still runs once per target or
model it declares: neither flag changes an `all_targets` selector, and
`--model` does not change `all_models`, though `--agent` does
([§FS-rhei-run.2.2](functional-spec/rhei-run.spec.md#22-agent-execution)).

A task overrides the model with `**Model:**`, or the whole identity with
`**Target:**`, and the override follows the task through every state an agent
works it in:

```markdown
### Task 4: Port the parser
**State:** pending
**Model:** opus
```

Three things refuse a task override, all at `rhei validate`, before any agent
starts:

- a task that writes both `**Model:**` and `**Target:**`;
- a task that enters a state with `all_targets` or `all_models`, which runs
  once per declared target and has no single identity to replace;
- a task that enters a state with `target_locked: true`, which says its
  identity is essential to the phase.

The full rules are in [§FS-rhei-plan-language.3.11](functional-spec/rhei-plan-language.spec.md#311-task-execution-overrides) and
[§FS-rhei-agents.1.4](functional-spec/rhei-agents.spec.md#14-resolution-order).

## A List: Entries Add Up

MCP servers and skills are lists, and an invocation gets the entries from all
three places that name them. Each one is built in three steps
([§FS-rhei-task-tooling.3](functional-spec/rhei-task-tooling.spec.md#3-the-effective-set)):

1. `defaults.mcp_servers` (or `defaults.skills`) from the settings;
2. then the state's `mcp_servers` (or `skills`);
3. then the task's `**MCP servers:**` (or `**Skills:**`).

The list is kept to one entry per id:

- **A state entry replaces a defaults entry** with the same id.
- **A task entry never replaces anything.** When the task names an id the set
  already holds, the entry already there keeps its definition. It is required
  if either side requires it, so a task can insist on a server the state
  marked `optional`, but cannot make optional one the state requires.
- **Only the place above can take entries away.** A state's `mcp_servers: []`
  clears the defaults; leaving the field out inherits them. A state's
  `withhold_task_tooling: true` drops what the task named, MCP servers and
  skills together. Neither removes anything else: the task's entries survive
  `[]`, and an id the state or the defaults supply survives withholding.

An example:

```yaml
# states.yaml
states:
  pending:
    agent: claude-code
    mcp_servers:
      - postgres
      - id: grafana
        optional: true
  review:
    agent: claude-code
    mcp_servers: []
    withhold_task_tooling: true
```

```markdown
### Task 7: Summarise the release thread
**State:** pending
**MCP servers:** thunderbird-mail, grafana
```

With `defaults.mcp_servers: [github]` in the settings, Task 7 gets `github`,
`postgres`, `grafana` and `thunderbird-mail` in `pending`. `grafana` is now
required, because the task did not mark it `(optional)`. In `review` it gets
nothing: `[]` cleared `github`, and the state withholds the task's two servers.

Why the lists add up rather than refuse a conflict: the agent can have both,
so there is nothing to choose between. A ticket that needs a mail server can
still pass through a review state that must not read mail, because the state
withholds the server instead of refusing the ticket.

A few more rules that are easy to miss:

- **A task's entries are its own.** Its child tasks do not inherit them, and
  another task in the same state does not get them.
- **Settings files combine differently from states.** The `mcp_servers` and
  `skills` registries merge by id, a project entry replacing the machine's
  entry with the same id. The `defaults.mcp_servers` list does not merge: the
  project's list, when it writes one, replaces the machine's whole. Repeat an
  entry to keep it, or write `[]` to clear the list
  ([§FS-rhei-agents.1.3](functional-spec/rhei-agents.spec.md#13-merge-semantics)).
- **An inline definition wins over the registry.** A state entry that carries
  its own `command`, `url` or `path` uses it even when the registry has an
  entry with the same id. An entry that carries only `id` and `optional` is
  looked up in the registry ([§FS-rhei-states.7.1](functional-spec/rhei-states.spec.md#71-entry-forms)).
- **For Claude Code, this list is all it gets.** Rhei always passes
  `--strict-mcp-config`, so servers from your own Claude Code configuration,
  plugins and connectors are not added. An empty list means no MCP servers at
  all ([§FS-rhei-states.7.3](functional-spec/rhei-states.spec.md#73-runtime-semantics)).

## When a Server Cannot Be Reached

Just before each spawn, `rhei run` checks every server and skill in the list.
It never starts a server itself: the agent does that, once.
([§FS-rhei-agents.6.2](functional-spec/rhei-agents.spec.md#62-the-spawn-preflight))

- A `command` server must name an executable that exists: on `PATH` (the
  entry's own `env.PATH` if it sets one), or, for a path like
  `./bin/server`, relative to the checkout the agent starts in. For an agent
  that attaches servers with `mcp_flag`, a declared `working_directory` must
  exist too, and the path is taken from there.
- A `url` server's host must accept a TCP connection within its
  `startup_timeout`, `10s` unless the entry sets another.
- A skill's `path` must exist and be readable.

What happens when an entry fails depends on whether it is required (the
default) or `optional: true`:

- **Optional:** the entry is dropped with a warning that gives the reason, and
  the agent starts without it:

  ```text
  warning: optional mcp 'grafana' unavailable (grafana.internal:443 accepted no connection within 10s); dropping
  ```

- **Required:** the agent does not start. If the state has an
  `mcp_unavailable` (or `skill_unavailable`) transition that matches, it
  fires, and the ticket moves on:

  ```yaml
  transitions:
    - from: pending
      to: blocked
      mcp_unavailable: [postgres]   # or `true` for any server
  ```

  Otherwise the ticket stays where it is and the run logs why:

  ```text
  error: required tooling unavailable for task plan.7 in state 'pending': mcp:postgres (command 'mcp-postgres' is not on PATH)
  ```

A server that is reached but then fails its handshake, or crashes partway
through, is the agent's to report. It appears in the agent's log.

## Seeing What an Invocation Got

- **The run log.** Each invocation's log header lists what it got on its
  `mcp_servers:` and `skills:` lines, and names a dropped optional entry with
  a `?`. When a state withheld task entries, the header adds
  `mcp_servers_withheld:` or `skills_withheld:`.
- **The agent's environment.** `RHEI_MCP_SERVERS` and `RHEI_SKILLS` list the
  ids that were attached. `RHEI_MCP_<NAME>_AVAILABLE` and
  `RHEI_SKILL_<ID>_AVAILABLE` say `true` or `false` for each declared entry.
- **The prompt.** A state's instructions can branch on
  `{if mcp.<name>.available}`, for a server the state itself declares.
- **`rhei roster`** shows the merged agents and models, and the settings file
  each one came from.
