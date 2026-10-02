### Task 1: Implement the overflow fix
**State:** build
**Provides:** verdict

Make the parser reject the literal with a diagnostic instead of panicking, then
run the gate and keep its own last line as the `verdict` export.

### Task 2: Ship the fix
**State:** work
**Prior:** Task 1
**Consumes:** 1:verdict

Merge the fix once the gate's verdict is green.
