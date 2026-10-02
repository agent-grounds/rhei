### Task 1: Fix the crash on a 64-bit literal
**State:** supervising

The toolchain must reject the literal with a diagnostic instead of panicking.
Brief each child, and say in your result what was done.

#### Task 1.1: Reproduce the crash
**State:** work

Reproduce the crash from issue 87 and say where it panics.

#### Task 1.2: Decide where the fix goes
**State:** work
**Prior:** Task 1.1

Read the reproducer and decide where the fix goes.

#### Task 1.3: Fix the crash
**State:** work
**Prior:** Task 1.2

Fix the crash where Task 1.2 decided.
