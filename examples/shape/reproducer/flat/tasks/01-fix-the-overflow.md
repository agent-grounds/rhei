### Task 1: Triage the overflow report
**State:** work

Decide whether issue 87 is real and new: search the tracker for an earlier
report of the same panic, and read the input the report gives.

### Task 2: Reproduce the overflow
**State:** work
**Prior:** Task 1
**Provides:** reproducer

Write the smallest input that makes the parser panic, and the command that
feeds it to the parser, to the `reproducer` export.

### Task 3: Fix the parser
**State:** work
**Prior:** Task 2
**Consumes:** 2:reproducer

Make the parser reject the literal with a diagnostic instead of panicking. The
reproducer is the acceptance.

### Task 4: Run the gate
**State:** work
**Prior:** Task 3, Task 2
**Consumes:** 2:reproducer

Run the test suite, and the reproducer against the fixed parser.
