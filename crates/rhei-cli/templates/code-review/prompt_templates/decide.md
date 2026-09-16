Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Decide the final fix plan for Task {task_id}: {task_title}.

Read:
  - `{input.review-issues.path}`
  - `{input.proposal-matrix.path}`
  - all validation files under `runtime/validations/`
  - all proposal files under `runtime/proposals/`

For each candidate issue:
  - decide `accepted`, `rejected`, or `defer`
  - explain validation/proposal discrepancies
  - choose or synthesize one final approach when accepted
  - keep the final approach minimal and testable
  - reject/defer nits and low-value issues whose fix would be too broad

Write `{output.final-decision.path}` in this shape:

  # Final Review Decision - {{change_ref}}

  ## Accepted Fixes

  - I-001 - <summary>
    - Files: <paths>
    - Final approach: <exact implementation direction>
    - Tests/checks: <commands or files>
    - Discrepancy decision: <why this approach won>

  ## Rejected / Deferred

  - I-002 - rejected | defer
    - Reason: <why not fixing now>

Transition to `human-review`.
