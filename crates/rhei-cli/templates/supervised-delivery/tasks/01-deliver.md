### Task deliver: {{title}}
**State:** supervising

Deliver what `{{spec_path}}` describes, and be the one node that decides how.

Acceptance for the whole delivery: the spec's normative behaviour is
implemented; every finding of severity `major` or `blocker` is either fixed or
carries a written reason for being rejected or deferred; the tests the coverage
audit asked for exist; and the documentation the change made stale is updated.

Every child below is held until you write its brief at
`runtime/supervise/<task-id>.md`. Nothing beneath this task runs while you run,
and nothing new is dispatched until you return.

#### Task deliver.implement: Implement {{spec_path}}
**State:** implement
**Provides:** report

Write the implementation the spec calls for, then publish the `report` export
so every later step works from one description of what landed.
{% for k in range(1, review_rounds + 1) %}
#### Task deliver.review-{{k}}: Code review round {{k}}
**State:** review
**Prior:** Task deliver.{% if k == 1 %}implement{% else %}fix-{{ k - 1 }}, Task deliver.review-{{ k - 1 }}{% endif %}
**Consumes:** {% if k == 1 %}deliver.implement:report{% else %}deliver.fix-{{ k - 1 }}:resolutions, deliver.review-{{ k - 1 }}:findings{% endif %}
**Provides:** findings

Read the code as it now stands and record round {{k}}'s findings. Every finding
must be reproduced: a command and its output, or the exact code path that
misbehaves.

#### Task deliver.pm-{{k}}: Product review round {{k}}
**State:** pm-review
**Prior:** Task deliver.{% if k == 1 %}implement{% else %}fix-{{ k - 1 }}, Task deliver.pm-{{ k - 1 }}{% endif %}
**Consumes:** {% if k == 1 %}deliver.implement:report{% else %}deliver.fix-{{ k - 1 }}:resolutions, deliver.pm-{{ k - 1 }}:findings{% endif %}
**Provides:** findings

Judge round {{k}} as the person who has to live with this change: user
experience, predictability, and whether the documentation tells the truth. Runs
alongside `deliver.review-{{k}}`; the two never read each other.

#### Task deliver.fix-{{k}}: Fix round {{k}}
**State:** fix
**Prior:** Task deliver.review-{{k}}, Task deliver.pm-{{k}}
**Consumes:** deliver.review-{{k}}:findings, deliver.pm-{{k}}:findings
**Provides:** resolutions

Answer both reviews of round {{k}} in one pass, exactly as the supervisor's
brief scopes them, and publish one `resolutions` export covering both.
{% endfor %}{% for k in range(1, coverage_rounds + 1) %}
#### Task deliver.coverage-{{k}}: Test coverage audit round {{k}}
**State:** coverage
**Prior:** Task deliver.{% if k == 1 %}fix-1{% else %}coverage-fix-{{ k - 1 }}{% endif %}, Task deliver.implement
**Consumes:** deliver.implement:report{% if k > 1 %}, deliver.coverage-fix-{{ k - 1 }}:resolutions{% endif %}
**Provides:** gaps

Name what the change left untested and, for each gap, the test that would close
it. Write no tests here — that is the next step's job.

#### Task deliver.coverage-fix-{{k}}: Close coverage gaps round {{k}}
**State:** fix
**Prior:** Task deliver.coverage-{{k}}
**Consumes:** deliver.coverage-{{k}}:gaps
**Provides:** resolutions

Write the tests the audit asked for and publish one `resolutions` export saying
what each gap became.
{% endfor %}{% for k in range(1, docs_rounds + 1) %}
#### Task deliver.docs-{{k}}: Documentation round {{k}}
**State:** docs
**Prior:** Task deliver.implement, Task deliver.coverage-fix-{{coverage_rounds}}{% if k > 1 %}, Task deliver.docs-{{ k - 1 }}{% endif %}
**Consumes:** deliver.implement:report, deliver.coverage-fix-{{coverage_rounds}}:resolutions
**Provides:** report

Update the documentation the delivery made stale and publish a `report` export
naming every file touched.
{% endfor %}
