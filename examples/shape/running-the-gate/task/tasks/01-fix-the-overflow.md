### Task 1: Implement the overflow fix
**State:** work

Make the parser reject the literal with a diagnostic instead of panicking.

### Task 2: Run the gate
**State:** gate
**Prior:** Task 1
**Provides:** verdict

Run the suite and keep its own last line as the `verdict` export.

### Task 3: Ship the fix
**State:** work
**Prior:** Task 2
**Consumes:** 2:verdict

Merge the fix once the gate's verdict is green.
