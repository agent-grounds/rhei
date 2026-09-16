### Task coordinate: Coordinate review of {{change_ref}}
**State:** split

Resolve `{{change_ref}}` to a concrete set of changed files. Follow the split
state's instructions to write the architectural overview and part manifest,
then create sibling review tasks and one aggregate task using this task's
actual node kind and compiled id prefix. Use the compiled state names and
artifact paths declared in states.yaml. The aggregate task validates findings,
compares proposals, records a bounded decision, waits for human approval, and
applies accepted fixes. Resolve the Git toplevel for repository inspection;
keep task files and runtime artifacts under this scratchpad workspace.
