Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Propose fixes for Task {task_id}: {task_title}.

Read `{input.review-issues.path}` and the validation files under
`runtime/validations/`. For every candidate issue that at least one
validator marked `valid` or `needs-decision`, propose the smallest
fix that would address it. If the issue is a nit, style preference, or
low-value cleanup that would require a large or risky change, mark it as
"defer/no-fix" in the proposal instead of designing a broad edit. Do not
modify repository files.

Write `{output.fix-proposal.path}` in this shape:

  # Fix Proposal - target {target}

  - I-001
    - Proposed action: <concrete change>
    - Files to edit: <paths>
    - Tests/checks: <commands or files>
    - Risk: low | medium | high
    - Scope judgment: minimal-fix | defer-no-fix
    - Notes on validation disagreement: <none or summary>

Transition to `aggregate-proposals` when the proposal file is written.
