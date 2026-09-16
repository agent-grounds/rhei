Use the current state `{state}` and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Apply final fixes for Task {task_id}: {task_title}.

At the start of this state, record the current directory as the review
scratchpad workspace. All `{input.*.path}` and `{output.*.path}` paths
are relative to that scratchpad. Repository edits may happen elsewhere,
but runtime artifacts must be written back to the scratchpad paths.

Read `{input.final-decision.path}`.
{%- if fix_prepare != "none" %}

Read `{input.workspace-ref.path}` and perform all repository edits in
the recorded workspace path. Do not write Rhei runtime artifacts inside
that prepared workspace.
{%- else %}

Resolve the repository root with `git rev-parse --show-toplevel` and
apply edits in that checkout, not in the review scratchpad.
{%- endif %}

Apply only issues listed under "Accepted Fixes". Keep each change as
narrow as the final decision allows. If a nit or low-value issue would
require a large change, do not implement it; record that it was skipped
because the required scope was too wide. If there are no accepted fixes,
make no repository edits and write a no-op note.

Write `{output.final-fix-note.path}` with:
  - accepted issue ids fixed
  - files touched
  - tests/checks run, or why they were not run
  - any accepted issue intentionally left unfixed and why

Transition to `{% if fix_commit != "none" %}commit-fix{% else %}completed{% endif %}`.
