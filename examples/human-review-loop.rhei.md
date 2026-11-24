# Rhei: Human Review Content Refresh

## Context
Refresh customer-facing content. Its human checkpoints, the reviewer questions
of Task 2 and the final sign-off of Task 4, are ordinary tasks in the dependency
chain, not gate states: no `states.yaml` sits beside this plan, so the built-in
`rhei` machine judges it and every task is `pending` or `completed`. For a plan
that stops at a human gate, see `hourly-human-intervention-example/`.

## Notes
This example demonstrates:
- numeric task identifiers, with child tasks numbered under their parent
- a `**Prior:**` dependency chain across tasks: 2 after 1, 3 after 2, 4 after 3
- child task bodies the parser captures
- the built-in `rhei` states, `pending` and `completed`, with no `states.yaml` beside the plan

## Tasks

### Task 1: Audit existing content
**State:** completed

Review the current landing page, pricing page, and help center copy.

#### Task 1.1: Snapshot current messaging
**State:** completed
Capture current headlines and calls to action for comparison.

### Task 2: Prepare revised copy
**State:** pending
**Prior:** Task 1

Draft updated messaging and leave open questions for stakeholders.

#### Task 2.1: Draft homepage hero
**State:** completed
Propose updated headline, supporting text, and primary call to action.

#### Task 2.2: Summarize reviewer questions
**State:** completed
List questions for product and legal review in a short bullet list.

### Task 3: Apply approved edits
**State:** pending
**Prior:** Task 2

Update the content files after review feedback is resolved.

#### Task 3.1: Patch markdown sources
**State:** pending
Apply approved copy updates to the documentation repository.

### Task 4: Final sign-off
**State:** pending
**Prior:** Task 3

Coordinate final checks before publication.

#### Task 4.1: Confirm launch checklist
**State:** pending
Verify approvals, screenshots, and rollback notes are present.
