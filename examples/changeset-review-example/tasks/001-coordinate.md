### Task coordinate: Coordinate review of PR#42
**State:** split

Resolve `PR#42` to a concrete set of changed files. Follow the split
state's instructions to write the architectural overview and part manifest,
then create sibling review tasks and one aggregate task using this task's
actual node kind and id prefix. The states are `split`, `review`,
`aggregate-reviews`, `validate-review`, `propose-fixes`,
`aggregate-proposals`, `decide` and `human-review`, under exactly those names
wherever this template was placed, and their artifact paths are declared
beside them in states.yaml. The aggregate task validates findings,
compares proposals, records a bounded decision, waits for human approval, and
applies accepted fixes. Resolve the Git toplevel for repository inspection;
keep task files and runtime artifacts under this scratchpad workspace.
