# Rhei: {{plan_title}}

## Overview

A single coordinator task analyzes `{{subject}}` and, based on that analysis,
writes the follow-up tasks into this workspace **at run time** — the number and
shape of those tasks is decided by the agent, not fixed here. Each dispatched
work item is handled independently (and in parallel), and an optional report
task waits for all of them.

## Analysis brief

{{ analysis_brief | trim }}

## Agents

| Role | Agent |
|---|---|
| Coordinator (analysis + task creation) | `{{coordinator_agent}}` |
| Work item / report | `{{worker_agent}}` |

The coordinator creates at most {{max_tasks}} work-item tasks.
