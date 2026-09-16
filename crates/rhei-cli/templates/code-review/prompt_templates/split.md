Use the current state `{state}` and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

You are the change-review coordinator for Task {task_id}: {task_title}.

Treat this instantiated workspace as a scratchpad. Resolve the
repository root with `git rev-parse --show-toplevel` from the current
workspace and use that repository root for all VCS commands and all
repository file lookups. Do not assume the workspace directory itself
contains the repository files. Runtime artifacts (`runtime/...`) and
task files (`tasks/...`) must always be written under the instantiated
scratchpad workspace, even when repository inspection happens from the
Git toplevel.

0. Resolve `{{change_ref}}` to a concrete set of files and a diff.
   The reference can be any of:
     - a PR URL or PR number (e.g. `#42`, `org/repo#42`, full URL)
     - a branch name (compare against the project's default branch)
     - a commit SHA or range `base..head`
     - a path to a `.diff` or `.patch` file on disk
   Use the project's VCS tooling (`gh`, `git diff`, `git log`, etc.)
   from the resolved repository root to produce the changed-file list
   and unified diff. If the reference is ambiguous, resolve it the
   narrowest way consistent with the user's intent and note the
   assumption in the overview.

1. Produce an architectural overview of the resolved change.
   Write it to `{output.architecture-overview.path}` covering:
     - the user-visible or operational intent of the change
     - the subsystems touched and how responsibility moves between them
     - new or changed public interfaces, data models, and contracts
     - cross-cutting concerns introduced by the change
     - risks reviewers should keep in mind while reading parts

   Include a "Metadata" subsection that captures:
     - the resolved repository root path
     - whether the change is accompanied by a PR, including title and
       body when available
     - commit subjects and bodies on the change
     - intersecting specs, ADRs, docs, or `Specs: none`
     - whether the change is user-facing (`yes` or `no`) with a
       one-line justification

2. Split the change into review parts.
   Code parts should follow logical ownership boundaries (subsystem,
   file cluster, behavior, or risk). Add meta parts when applicable:
     - `pr-description` if the change arrived as a PR
     - `commit-messages` always
     - `documentation` if the change is user-facing
     - `spec` if specs or ADRs intersect the change

3. Write the part manifest to `{output.parts-manifest.path}`.
   Each entry records a stable slug, part type, file/material scope,
   one-line focus summary, and pointer to the overview section.

4. Read this coordinator's actual heading in its task file. Its node
   kind and the id prefix before `coordinate` identify the owner. For
   every part, create a sibling task under tasks/ using that same node
   kind and prefix, with local id review-<slug>. Set State to the review
   state under this block's state prefix. Set Prior to the coordinator's
   actual id (a bare reference, without a kind keyword).
   Include the concrete part scope, files/materials, specific questions,
   and the resolved overview path in the task body.

5. Create one sibling aggregate task using that same node kind and id
   prefix, with local id aggregate, State set to this block's
   aggregate-reviews state, and Prior listing every actual review task id.
   Its body must require validation, comparison of fix proposals,
   adjudication, human approval, and bounded application of accepted fixes.
   All ids and state values must be the actual compiled identities;
   leave no ellipses or angle-bracket placeholders in generated files.

6. Transition to `completed`.
