### Task 1: Review pull request 412 against its spec
**State:** supervising

Read the change and `docs/export.spec.md`, then hold the change to each point
below. Brief each check with what the reading found that it needs. Once every
check is in, write the verdict: approve, or request changes naming the points
that failed.

#### Task 1.1: Check R1, exports stream their rows
**State:** check

Does the change hold requirement R1: an export streams its rows to the file
rather than building the file in memory?

#### Task 1.2: Check G1, a 1 GB export stays under 200 MB
**State:** check

Does the change meet goal G1: a 1 GB export peaks under 200 MB of memory?

#### Task 1.3: Check N1, no new export format
**State:** check

Does the change keep non-goal N1: it adds no export format?
