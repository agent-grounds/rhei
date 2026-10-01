### Task 1: Read pull request 412
**State:** work
**Provides:** reading

Read the change and `docs/export.spec.md`, and write what the change does,
file by file, to the `reading` export.

### Task 2: Check R1, exports stream their rows
**State:** check
**Prior:** Task 1
**Consumes:** 1:reading
**Provides:** finding

Does the change hold requirement R1: an export streams its rows to the file
rather than building the file in memory?

### Task 3: Check G1, a 1 GB export stays under 200 MB
**State:** check
**Prior:** Task 1
**Consumes:** 1:reading
**Provides:** finding

Does the change meet goal G1: a 1 GB export peaks under 200 MB of memory?

### Task 4: Check N1, no new export format
**State:** check
**Prior:** Task 1
**Consumes:** 1:reading
**Provides:** finding

Does the change keep non-goal N1: it adds no export format?

### Task 5: Write the verdict
**State:** work
**Prior:** Task 2, Task 3, Task 4
**Consumes:** 2:finding, 3:finding, 4:finding

Weigh the three findings and write the verdict on pull request 412: approve,
or request changes naming the points that failed.
