# DF-delegated-count-ceiling: A paying machine may hand the two count ceilings to the project

## 1. Status

accepted

Amends the ruling recorded on closed
[agent-grounds/rhei#305](https://github.com/agent-grounds/rhei/issues/305),
that "a machine's value is a ceiling a project or plan cannot raise", by one
exception the machine itself has to choose. Decided on
[agent-grounds/rhei#462](https://github.com/agent-grounds/rhei/issues/462): the
owner's direction, comment
`https://github.com/agent-grounds/rhei/issues/462#issuecomment-6019307216`,
and the wording ruling, comment
`https://github.com/agent-grounds/rhei/issues/462#issuecomment-6021085897`.

## 2. Context

The machine's value is the ceiling of every bound, and where the machine file is
silent the built-in is that value ([§REQ-bounded-neural-work.2](../../requirements/bounded-neural-work.spec.md#2-bounded-by-default-refusing-nothing-that-runs-today)). So a workflow
whose repository checks in `"defaults": { "transition_limit": 1000 }` still
halts at 80 on every runner whose machine file is untouched. A deep
code-coverage pass needs about 220 transitions, and the only way to run it was
to ask every runner to mirror the number in a file outside version control,
which nothing in the repository can enforce.

The ceiling exists because of who pays: a cloned repository's settings file must
not set how much neural work a stranger's machine runs. The report argued from
who reviews the file. Both are true, and they belong to different parties.

## 3. Decision

The machine that pays decides, and it may decide to trust the project. A machine
that sets `defaults.clamp_projects: false` in its own settings file makes each
count bound the project declares — `transition_limit`, `invocations_per_day`, one
key at a time — the ceiling in place of its own, whether higher or lower. A plan
or profile may still lower that ceiling and still cannot raise it, and is
reported as `limited by project settings` when it asks for more
([§FS-rhei-budgets.2](../../functional-spec/rhei-budgets.spec.md#2-where-a-bound-comes-from), §FS-rhei-budgets.2.3).

The default does not move. A machine that does not set the key bounds every
project exactly as before, and `clamp_projects` anywhere but the machine file is
refused, so a repository cannot opt itself out
(§FS-rhei-agents.1.1.1). `spend_per_day` and `invocation_lifetime_max` are never
delegated.

```text
machine 80, project 40, profile 60   ->  40 (requested 60 by the plan, limited by project settings)
machine 80, project 1000, no profile ->  1000 (project)    delegated
machine 80, project 1000, no profile ->  80 (requested 1000 by the project, limited by machine settings)    not delegated
```

**Rejected.**

- *The project wins by default.* It reverses the who-pays rationale of
  §REQ-bounded-neural-work.2 for every machine at once, and makes a pulled
  commit a source of invocation capacity on machines that never chose it.
- *Per-key switches* such as `clamp_projects: ["transition_limit"]`. The
  project's own declarations already say which keys it wants.
- *A mid-day raise applying only from the next window.* A window account stores
  no day limit, so it would need a new journal fact, and a raise would then
  behave differently from a lowering. A delegated raise is measured against the
  same day's consumption, exactly as a machine-file edit is
  (§REQ-bounded-neural-work.4).
- *`deny_unknown_fields` on settings and profiles* as the refusal. It would
  refuse every unknown key in every existing file, a far wider break than one
  key.

## 4. Consequences

The same checked-in workflow can now carry its own sizing, reviewed with it, and
runs the same on every runner that delegated. A runner that did not delegate
still stops at its own number, so one machine stopping at 80 and another
allowing 1000 is two machines' choices, and every view that reports a delegated
bound says whose ceiling it is and which machine file delegated it.

Every settings remedy in a halt now names the file by its path, on every
machine, because "the project settings file" no longer says which of two homes
holds the ceiling (§FS-rhei-budgets.8).
