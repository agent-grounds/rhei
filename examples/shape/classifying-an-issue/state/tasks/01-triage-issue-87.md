### Task 1: Triage issue 87
**State:** classify
**Provides:** kind

Judge whether issue 87 is real and which kind it is, then label it and export
the kind as `kind`.

### Task 2: Route the issue
**State:** work
**Prior:** Task 1
**Consumes:** 1:kind

Route issue 87 to the queue for its kind.
