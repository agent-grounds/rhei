Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Prepare the final-fix workspace for Task {task_id}: {task_title}.

Read `{input.final-decision.path}`. Treat this review workspace as a
scratchpad. Resolve the repository root with `git rev-parse
--show-toplevel` and use that checkout as the source for isolation.
Runtime artifacts must still be written under the original scratchpad
workspace, not inside any prepared branch, worktree, or fork checkout.

Create an isolated workspace using mode `{{fix_prepare}}`:
{%- if fix_prepare == "branch" %}
  - Create and switch to branch `fix/{task_id}` from the review base.
{%- elif fix_prepare == "worktree" %}
  - Create a worktree under `runtime/worktrees/{task_id}/` on branch
    `fix/{task_id}`.
{%- elif fix_prepare == "fork" %}
  - Fork the upstream repository, clone it under `runtime/forks/{task_id}/`,
    and create branch `fix/{task_id}` from the review base.
{%- endif %}

Record the resulting location to `{output.workspace-ref.path}`:

  mode: {{fix_prepare}}
  path: <absolute path where final-fix must work>
  branch: <branch name>
  base: <commit or branch>

Transition to `final-fix` once the workspace is ready.
