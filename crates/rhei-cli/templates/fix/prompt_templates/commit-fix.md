Use the current state `{state}` and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Commit or publish final fixes for Task {task_id}: {task_title}.
At the start of this state, record the current directory as the review
scratchpad workspace. All `{input.*.path}` and `{output.*.path}` paths
are relative to that scratchpad. Commit/push/PR commands may run in the
repository checkout, but runtime artifacts must be written back to the
scratchpad paths.
{%- if fix_prepare != "none" %}

Operate inside the workspace recorded at `{input.workspace-ref.path}`.
{%- else %}

Resolve the repository root with `git rev-parse --show-toplevel` and
run the commit step there.
{%- endif %}

Commit mode: `{{fix_commit}}`.
{%- if fix_commit == "commit" %}
  - Stage only files listed in `{input.final-fix-note.path}`.
  - Create one local commit summarizing the accepted fixes.
{%- elif fix_commit == "push" %}
  - Stage only files listed in `{input.final-fix-note.path}`.
  - Create one local commit and push the branch.
{%- elif fix_commit == "pr" %}
  - Stage only files listed in `{input.final-fix-note.path}`.
  - Create one local commit, push the branch, and open a pull request.
    The PR body must reference the final decision artifact and summarize
    accepted issues fixed.
{%- endif %}

Record the result to `{output.commit-ref.path}`:

  mode: {{fix_commit}}
  commit: <sha>
  remote_ref: <branch pushed, or empty>
  pr_url: <URL, or empty>

Transition to `completed`.
