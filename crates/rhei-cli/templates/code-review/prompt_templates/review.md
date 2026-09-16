Use the current task state and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Review Task {task_id}: {task_title}.

Read the part scope from the task body and consult the architectural
overview at `runtime/manifests/coordinate-architecture.md`. Treat this
workspace as a scratchpad: resolve the Git toplevel before reading
repository files, but keep runtime artifact paths anchored to this
workspace.
{%- if review_focus %}

Organize findings into these focus subsections. Every subsection must
appear, even if empty (write `- none`):
{%- for f in review_focus %}
  - {{ f }}
{%- endfor %}
{%- else %}

Produce a general review covering correctness, regressions, and risks.
{%- endif %}

Write findings to `{output.part-findings.path}` in this shape:

  # Part Review: {task_title} - target {target}
{%- if review_focus %}
{%- for f in review_focus %}

  ## {{ f }}

  - R-<short-id>: <one-line finding>
    - Severity: high | medium | low
    - File: <path>
    - Detail: <what is wrong or risky, and why>
    - Evidence: <specific code/spec/test evidence>
{%- endfor %}
{%- else %}

  - R-<short-id>: <one-line finding>
    - Severity: high | medium | low
    - File: <path>
    - Detail: <what is wrong or risky, and why>
    - Evidence: <specific code/spec/test evidence>
{%- endif %}

Keep findings specific and actionable. Transition to `completed` when
the findings file is written.
