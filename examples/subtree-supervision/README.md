# Subtree Supervision Example

A parent task that looks after its children **while they run** instead of only
integrating them at the end.

`Task 1` sits in `supervising`, a state that declares
`execute_on: descendant-terminal`. That one field turns the task holding it into
a **supervisor**: the value is a scope (`child` or `descendant`) and an event
(`terminal` or `transition`), and this one wakes the task after every
descendant that reaches a terminal state, at any depth, holding the rest of the
subtree in between. Between visits the parent briefs the next step, appends work
the plan turned out to need, or cancels a step the results made unnecessary.

This is the chain from
[the supervision spec's example](../../docs/functional-spec/rhei-supervision.spec.md#7-example),
runnable with mock agents so it needs no credentials.

## Run it

```bash
cargo xtask examples run subtree-supervision
```

That copies this directory to a temporary workspace and runs it there. To run a
copy yourself:

```bash
cp -r examples/subtree-supervision /tmp/ss
rhei run /tmp/ss --no-tui
```

The `states.yaml` in the rhei's own directory is its machine, so no
`--state-machine` flag is needed.

## What you should see

Nine invocations, the supervisor scheduled *between* its children and never
beside one:

```
ss.1   supervising  visit 1     briefs 1.1, then releases the subtree
ss.1.1 review     visit 1     writes runtime/review/ss.1.1.md, finishes
ss.1   supervising  visit 2     reads the checkpoint, briefs 1.2, releases
ss.1.2 fix        visit 1
ss.1   supervising  visit 3
ss.1.3 review     visit 1
ss.1   supervising  visit 4
ss.1.4 fix        visit 1
ss.1   supervising  visit 5     openDescendants == 0 → writes its result
```

(the id prefix is the workspace directory's name)

`runtime/logs/subtree-supervision.log` records exactly that order, and
`runtime/supervise/` holds the four briefs the supervisor wrote.

## The three edges, in order

Transitions are tried in declaration order, and for a supervising state the
order *is* the design:

| # | Edge | Condition | Meaning |
|---|------|-----------|---------|
| 1 | `supervising → human-review` | `visitCount >= visits` | the budget ran out; a human decides |
| 2 | `supervising → completed` | `openDescendants < 1` | the subtree closed; write the result |
| 3 | `supervising → supervising` | *(none)* | release the subtree and wait for the next checkpoint |

The unconditional self-loop is the **release** edge: without it the supervisor
would run once and never wait for its children. It is also what ends a *manual*
visit — `rhei transition /tmp/ss --task 1 --from supervising --to supervising`
releases the subtree and drops the worker's claim.

## Watching the barrier

While the supervisor is owed a visit, nothing beneath it is dispatched or
claimable. Stop the run mid-chain and ask:

```bash
rhei next /tmp/ss --task 1.2
```

and you get `Task ss.1.2 is held by supervisor Task ss.1 (supervising)`, naming
the ticket to work instead.

## Session continuity

Each visit is meant to *continue* the supervisor's own transcript, which needs
an agent with session support. Of the built-in profiles only `pi` has one today,
and only through a `target:` that resolves a provider and a model:

```yaml
  supervising:
    execute_on: descendant-terminal
    target: pi:anthropic:claude-sonnet-4-5
    snapshot:
      emit:    { name: supervisor, on: always }
      inherit: { name: supervisor, from: self }
```

Every other built-in profile must omit the `snapshot:` block — declaring it is a
hard validation error. Supervision still works without it: the supervisor starts
each visit cold and is carried by its checkpoints and its briefs. This example
uses the mock agent and therefore declares no snapshot block.

## Files

- `index.rhei.md` — the workspace index and its notes
- `tasks/01-harden-the-parser.md` — the parent and its four children
- `states.yaml` — the supervising state and its three edges
- `workflow.py` — the mock agent standing in for a real one. It is Python, not
  a shell script, so the example runs wherever `python3` is on `PATH`. It
  resolves the execution root, task id, and terminal-result path from the
  delivered prompt rather than its environment, because an agent's cwd is the
  repository checkout, not the plan directory; the settings bootstrap reads
  that same prompt map before it execs the script.
- `.agent-grounds/rhei/settings.json` — registers that mock as the default agent

## See also

- [Subtree Supervision Specification](../../docs/functional-spec/rhei-supervision.spec.md)
- [`analyze-and-dispatch`](../analyze-and-dispatch-example/) — a parent that
  *appends* children and then integrates them once, with no supervision
