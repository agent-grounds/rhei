### Task 1: Triage issue 87
**State:** work

Judge whether issue 87 is real.

### Task 2: Classify the issue
**State:** classify
**Prior:** Task 1
**Provides:** kind

Judge which kind issue 87 is, label it, and export the kind as `kind`.

### Task 3: Route the issue
**State:** work
**Prior:** Task 2
**Consumes:** 2:kind

Route issue 87 to the queue for its kind.
