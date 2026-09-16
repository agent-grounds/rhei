Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Aggregate review findings for Task {task_id}: {task_title}.

1. Read every file under `runtime/findings/` produced by the review
   tasks listed in this task's `**Prior:**`.
2. Deduplicate findings that describe the same defect across parts or
   targets. Preserve disagreements and weak evidence; do not discard
   them silently.
3. Write candidate issues to `{output.review-issues.path}` in this
   shape:

   # Candidate Review Issues - {{change_ref}}

   - I-001 (severity: high | medium | low) - <one-line issue>
     - Status: candidate
     - Files: <paths>
     - Reviewers: <which targets reported it>
     - Evidence: <best concrete evidence>
     - Disagreements: <none, or what differed across reviewers>

4. Transition to `validate-review`.
