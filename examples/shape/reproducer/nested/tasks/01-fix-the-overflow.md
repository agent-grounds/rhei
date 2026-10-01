### Task 1: Triage the overflow report
**State:** work

Decide whether issue 87 is real and new. The two steps below are how.

#### Task 1.1: Search for a duplicate
**State:** work

Search the tracker for an earlier report of the same panic.

#### Task 1.2: Reproduce the overflow
**State:** work
**Prior:** Task 1.1
**Provides:** reproducer

Write the smallest input that makes the parser panic, and the command that
feeds it to the parser, to the `reproducer` export.

### Task 2: Fix the parser
**State:** work
**Prior:** Task 1, Task 1.2
**Consumes:** 1.2:reproducer

Make the parser reject the literal with a diagnostic instead of panicking. The
reproducer is the acceptance.

### Task 3: Run the gate
**State:** work
**Prior:** Task 2, Task 1.2
**Consumes:** 1.2:reproducer

Run the test suite, and the reproducer against the fixed parser.
