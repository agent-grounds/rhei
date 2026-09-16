Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Validate candidate review issues for Task {task_id}: {task_title}.

Read `{input.review-issues.path}` and inspect the referenced code,
specs, tests, and diff. For each issue, independently classify it:
`valid`, `invalid`, or `needs-decision`.

Write `{output.validation.path}` in this shape:

  # Review Validation - target {target}

  - I-001: valid | invalid | needs-decision
    - Reason: <short evidence-based rationale>
    - Correction: <if the issue statement should be narrowed or changed>

Transition to `propose-fixes` when the validation file is written.
