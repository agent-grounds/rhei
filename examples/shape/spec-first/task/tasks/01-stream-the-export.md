### Task 1: Write the spec and the failing test
**State:** work
**Provides:** contract

Write point 3 of `docs/export.spec.md`, "an export streams its rows", and the
test `streams_rows_under_200_mb` that fails today. Name both in the `contract`
export.

### Task 2: Implement the streaming export
**State:** work
**Prior:** Task 1
**Consumes:** 1:contract

Make the export stream its rows. The contract is the definition of done, and
this task may not edit it.

### Task 3: Review the change against the contract
**State:** work
**Prior:** Task 2, Task 1
**Consumes:** 1:contract

Hold the change to the spec point and the test the contract names.

### Task 4: Run the gate
**State:** work
**Prior:** Task 3, Task 1
**Consumes:** 1:contract

Run the test suite, and the contract's test by name.
