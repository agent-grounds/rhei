### Task 1: Ask the author which encoding to use
**State:** work

Ask on issue 412 which encoding the export should write.

### Task 2: Wait for the author's answer
**State:** wait
**Prior:** Task 1
**Provides:** answer

Wait for the author's reply on issue 412, and keep it as the `answer` export.

### Task 3: Implement the export
**State:** work
**Prior:** Task 2
**Consumes:** 2:answer

Write the export in the encoding the author chose.
