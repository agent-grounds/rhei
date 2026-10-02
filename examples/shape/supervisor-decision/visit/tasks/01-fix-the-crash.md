### Task 1: Fix the crash on a 64-bit literal
**State:** supervising

The toolchain must reject the literal with a diagnostic instead of panicking.
Between the children, decide from the reproducer where the fix goes, brief the
fix with that decision, and say in your result what you decided.

#### Task 1.1: Reproduce the crash
**State:** work

Reproduce the crash from issue 87 and say where it panics.

#### Task 1.2: Fix the crash
**State:** work
**Prior:** Task 1.1

Fix the crash where the brief says.
