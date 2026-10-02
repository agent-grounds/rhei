### Task 1: Search for duplicate candidates
**State:** lookup
**Provides:** candidates

Look up the issues that might be duplicates of issue 87, and export them as
`candidates`.

### Task 2: Judge whether issue 87 is a duplicate
**State:** work
**Prior:** Task 1
**Consumes:** 1:candidates

Judge issue 87 against the candidates, and write the verdict.

### Task 3: Fix the parser
**State:** work
**Prior:** Task 2

Make the parser reject the literal with a diagnostic instead of panicking.
