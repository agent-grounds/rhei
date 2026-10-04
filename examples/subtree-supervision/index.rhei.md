# Rhei: Harden the parser

## Overview

A pre-authored review/fix chain whose parent looks after it *while* it runs.

`Task 1` (in `tasks/01-harden-the-parser.md`) is in `supervising`, a state that
declares `execute_on: descendant-terminal`. That makes the task holding it a
**supervisor**: the orchestrator wakes it after every descendant that finishes
and holds the rest of the subtree in between. Between visits the parent briefs the next step, appends work
the plan turned out to need, or cancels a step the results made unnecessary — and
it finishes only once every child is terminal.

## Notes

- The supervisor's three outgoing edges are tried in declaration order, and the
  order is the design: the exhaustion edge (`visitCount >= visits`), the terminal
  edge (`openDescendants < 1`), then the unconditional self-loop that releases
  the subtree and waits for the next checkpoint.
- Each visit renders `## Checkpoints` — what moved since the last visit, carrying
  what that step left behind — and each child renders `## Supervisor Brief`, the
  direction the supervisor wrote for it under `runtime/supervise/`.
- `workflow.py` stands in for a real coding agent so the example runs with no
  credentials. It writes one brief per visit and the artifacts each state's
  contract declares; the hold/release barrier and the edge selection are the
  engine's, not the mock's.
