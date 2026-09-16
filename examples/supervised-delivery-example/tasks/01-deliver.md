### Task deliver: Deliver subtree supervision
**State:** supervising

Deliver what `docs/functional-spec/rhei-supervision.spec.md` describes, and be the one node that decides how.

Acceptance for the whole delivery: the spec's normative behaviour is
implemented; every finding of severity `major` or `blocker` is either fixed or
carries a written reason for being rejected or deferred; the tests the coverage
audit asked for exist; and the documentation the change made stale is updated.

Every child below is held until you write its brief at
`runtime/supervise/<task-id>.md`. Nothing beneath this task runs while you run,
and nothing new is dispatched until you return.

#### Task deliver.implement: Implement docs/functional-spec/rhei-supervision.spec.md
**State:** implement
**Provides:** report

Write the implementation the spec calls for, then publish the `report` export
so every later step works from one description of what landed.

#### Task deliver.review-1: Code review round 1
**State:** review
**Prior:** Task deliver.implement
**Consumes:** deliver.implement:report
**Provides:** findings

Read the code as it now stands and record round 1's findings. Every finding
must be reproduced: a command and its output, or the exact code path that
misbehaves.

#### Task deliver.pm-1: Product review round 1
**State:** pm-review
**Prior:** Task deliver.implement
**Consumes:** deliver.implement:report
**Provides:** findings

Judge round 1 as the person who has to live with this change: user
experience, predictability, and whether the documentation tells the truth. Runs
alongside `deliver.review-1`; the two never read each other.

#### Task deliver.fix-1: Fix round 1
**State:** fix
**Prior:** Task deliver.review-1, Task deliver.pm-1
**Consumes:** deliver.review-1:findings, deliver.pm-1:findings
**Provides:** resolutions

Answer both reviews of round 1 in one pass, exactly as the supervisor's
brief scopes them, and publish one `resolutions` export covering both.

#### Task deliver.review-2: Code review round 2
**State:** review
**Prior:** Task deliver.fix-1, Task deliver.review-1
**Consumes:** deliver.fix-1:resolutions, deliver.review-1:findings
**Provides:** findings

Read the code as it now stands and record round 2's findings. Every finding
must be reproduced: a command and its output, or the exact code path that
misbehaves.

#### Task deliver.pm-2: Product review round 2
**State:** pm-review
**Prior:** Task deliver.fix-1, Task deliver.pm-1
**Consumes:** deliver.fix-1:resolutions, deliver.pm-1:findings
**Provides:** findings

Judge round 2 as the person who has to live with this change: user
experience, predictability, and whether the documentation tells the truth. Runs
alongside `deliver.review-2`; the two never read each other.

#### Task deliver.fix-2: Fix round 2
**State:** fix
**Prior:** Task deliver.review-2, Task deliver.pm-2
**Consumes:** deliver.review-2:findings, deliver.pm-2:findings
**Provides:** resolutions

Answer both reviews of round 2 in one pass, exactly as the supervisor's
brief scopes them, and publish one `resolutions` export covering both.

#### Task deliver.coverage-1: Test coverage audit round 1
**State:** coverage
**Prior:** Task deliver.fix-1, Task deliver.implement
**Consumes:** deliver.implement:report
**Provides:** gaps

Name what the change left untested and, for each gap, the test that would close
it. Write no tests here — that is the next step's job.

#### Task deliver.coverage-fix-1: Close coverage gaps round 1
**State:** fix
**Prior:** Task deliver.coverage-1
**Consumes:** deliver.coverage-1:gaps
**Provides:** resolutions

Write the tests the audit asked for and publish one `resolutions` export saying
what each gap became.

#### Task deliver.docs-1: Documentation round 1
**State:** docs
**Prior:** Task deliver.implement, Task deliver.coverage-fix-1
**Consumes:** deliver.implement:report, deliver.coverage-fix-1:resolutions
**Provides:** report

Update the documentation the delivery made stale and publish a `report` export
naming every file touched.
