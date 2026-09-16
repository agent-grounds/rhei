Use the current state `{state}` and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Aggregate fix proposals for Task {task_id}: {task_title}.

Read:
  - `{input.review-issues.path}`
  - all validation files under `runtime/validations/`
  - all proposal files under `runtime/proposals/`

Write `{output.proposal-matrix.path}` in this shape:

  # Fix Proposal Matrix - {{change_ref}}

  - I-001 - <issue summary>
    - Validation summary: <valid/invalid/needs-decision by target>
    - Proposed approaches:
      - <target>: <short proposed action>
    - Agreement: full | partial | none
    - Scope judgment: minimal-fix | defer-no-fix | disputed
    - Discrepancy: <none, or what the final adjudicator must decide>

Mark nits or low-value issues as `defer-no-fix` when every reasonable
proposal would require broad changes. Transition to `decide` when the
proposal matrix is written.
