# Rhei: Analyze & Dispatch Example

## Overview

A single coordinator task analyzes `docs/functional-spec` and, based on that analysis,
writes the follow-up tasks into this workspace **at run time** — the number and
shape of those tasks is decided by the agent, not fixed here. Each dispatched
work item is handled independently (and in parallel), and an optional report
task waits for all of them.

## Analysis brief

Read each *.spec.md file under the subject directory. For every spec that does
NOT already reference a runnable example or fixture, create one work item to
add or link an example for it. One task per spec; skip specs that already link
an example. Use the spec's base filename as the task slug.

## Agents

| Role | Agent |
|---|---|
| Coordinator (analysis + task creation) | `claude-code[yolo]:anthropic:claude-opus-4-7` |
| Work item / report | `claude-code[yolo]:anthropic:claude-opus-4-7` |

The coordinator creates at most 6 work-item tasks.
