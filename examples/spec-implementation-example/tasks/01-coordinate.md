### Task coordinate: Resolve spec set and fan out per-spec + e2e-aggregate tasks
**State:** coordinate

Multi-spec mode. Resolve `main..HEAD` to the changed `*.spec.md` files
using the project's VCS tooling (`git diff`, `gh`, etc.). Every changed
spec file becomes one per-spec implementation task.

For every spec in scope, append one `impl-<slug>` task under `tasks/`
(starting from `02-`, since `01-coordinate.md` already exists) with
`**State:** implement` and `**Prior:** Task coordinate`, and write a
single-line `runtime/manifests/<spawned-id>-spec.txt` containing that
spec's repo-relative path.

Append exactly one `e2e-aggregate` task with `**State:** e2e-write` and
`**Prior:**` listing every `impl-<slug>` task you created. This task drives
the shared end-to-end coverage loop after every per-spec task completes.

Write the assignments manifest (per-task slug + spec path) to
`runtime/manifests/coordinate-spec-assignments.md`. Transition to
`completed` once the manifest, per-task spec files, per-spec tasks, and
the e2e-aggregate task are all in place.
