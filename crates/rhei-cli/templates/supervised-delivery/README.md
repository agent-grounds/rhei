# `supervised-delivery`

A delivery pipeline with one node in charge. The root task `deliver` sits in a
**supervising** state (`execute_on: child-terminal`, §FS-rhei-supervision.1.1),
so the orchestrator wakes it after every child that finishes and holds the rest
of the subtree in between. The pipeline below is therefore a set of steps the
supervisor *sends*, one decision at a time — not a conveyor belt that runs on
its own.

```text
supervisor prepares
    -> implement
    -> [ code review  ||  product review ] -> fix     x review_rounds
    -> coverage audit -> fix                          x coverage_rounds
    -> documentation                                  x docs_rounds
    -> supervisor writes the delivery result
```

Every round is a real task, unrolled at instantiation with
`{% raw %}{% for k in range(1, review_rounds + 1) %}{% endraw %}`. The
supervisor cancels the rounds the results made unnecessary, so the round counts
are ceilings rather than a schedule.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `spec_path` | string (positional 1) | *required* | The spec, issue, or design note that says what to deliver. |
| `title` | string | `Supervised delivery` | Title of the workspace and of the root task. |
| `supervisor_target` | execution-target | `claude-code[yolo]:anthropic:claude-opus-4-7` | Runs the supervising state. Every routing decision is its call. |
| `implementer_target` | execution-target | `claude-code[yolo]:anthropic:claude-opus-4-7` | Writes the implementation. |
| `reviewer_target` | execution-target | `codex[xhigh]:openai:gpt-5.5` | Runs the code-review rounds. Prefer a different agent from the implementer. |
| `pm_target` | execution-target | `claude-code[yolo]:anthropic:claude-opus-4-7` | Runs the product-review rounds. |
| `fixer_target` | execution-target | `claude-code[yolo]:anthropic:claude-opus-4-7` | Applies review fixes and closes coverage gaps. |
| `coverage_target` | execution-target | `codex[xhigh]:openai:gpt-5.5` | Audits test coverage. |
| `docs_target` | execution-target | `claude-code[yolo]:anthropic:claude-opus-4-7` | Updates documentation. |
| `review_rounds` | number | `2` | Ceiling on code-review + product-review + fix rounds. |
| `coverage_rounds` | number | `1` | Ceiling on coverage-audit + fix rounds. |
| `docs_rounds` | number | `1` | Ceiling on documentation rounds. |
| `ci_commands` | array of string | `[]` | Commands that must be green before a fix, coverage, or docs step reports success. Empty means the agents discover the project's checks. |
| `review_focus` | array of string | `[]` | Extra subsections every code review must answer. |
| `supervisor_session` | boolean | `false` | Give the supervisor a snapshot session so each visit continues the last. Only with a session-capable target such as `pi`. |

`review_rounds`, `coverage_rounds`, and `docs_rounds` are validated as positive
integers; `0` is refused, because a phase with no rounds has no anchor for the
phase after it.

## How each task walks the machine

| Task | States |
|---|---|
| `deliver` | `supervising` × N → `completed` (or `human-review` when the visit budget runs out) |
| `deliver.implement` | `implement` → `completed` |
| `deliver.review-k` | `review` → `completed` |
| `deliver.pm-k` | `pm-review` → `completed` |
| `deliver.fix-k` | `fix` → `completed` |
| `deliver.coverage-k` | `coverage` → `completed` |
| `deliver.coverage-fix-k` | `fix` → `completed` |
| `deliver.docs-k` | `docs` → `completed` |

The state diagram lives at the top of
[`states.yaml`](states.yaml). The supervisor's three outgoing edges are tried in
declaration order: exhaustion (`visitCount >= visits`), terminal
(`openDescendants < 1`), then the unconditional self-loop that releases the
subtree and waits for the next checkpoint.

## The release gate is the brief

Every child state declares a **required** input at
`runtime/supervise/<task-id>.md`. That file is the supervisor's brief
(§FS-rhei-supervision.5.2), and a child whose brief does not exist is not
dispatched. So the supervisor literally sends each agent: it reads what the
last step left behind, decides, writes the brief, and returns — and the engine
releases exactly the steps that now have one. The engine also renders each
brief into that child's prompt under `## Supervisor Brief`.

The only pair the supervisor ever briefs together is the code review and the
product review of one round.

## The structured channel is plan exports

Steps hand each other work product through `**Provides:**` / `**Consumes:**`
(§FS-rhei-plan-language.3.12), so the channel is declared in the plan and
injected into prompts. These declarations are data-flow, not a filesystem
visibility boundary: workers can still read undeclared sibling exports under
`runtime/exports/`. Concurrent scheduling and explicit briefs can reduce
accidental cross-reading, but cannot enforce blindness once an export exists.
Each export is one file holding exactly one fenced `json` block, at
`runtime/exports/<task-id>/<name>.md`. The same paths are declared as the
state's `outputs:`, so the completion condition refuses to finish a step that
did not publish its export.

`findings` — written by `review-k` and `pm-k`:

```json
{
  "round": 1,
  "role": "code-review",
  "verdict": "changes-requested",
  "findings": [
    {
      "id": "R1-01",
      "severity": "blocker",
      "category": "correctness",
      "file": "src/parser.rs",
      "line": 214,
      "summary": "one sentence naming the defect",
      "repro": "the command that shows it, and its output",
      "fix": "one line of direction",
      "spec": "the specification point violated, or null"
    }
  ]
}
```

`role` is `code-review` or `product`; `verdict` is `approve` or
`changes-requested`; `severity` is `blocker`, `major`, `minor`, or `nit`. The
**code-review** role must reproduce every finding — `repro` is non-empty or the
finding is not filed. The **product** role judges user experience,
predictability, and documentation against the spec and the project's goals; it
is told to exercise the change the way a user would rather than read the diff
for it (the house style of `docs/functional-spec/pm-review-2026-04-22.md`), and
may leave `repro` empty for a judgement about wording or docs.

`resolutions` — written by `fix-k` and `coverage-fix-k`:

```json
{
  "round": 1,
  "resolutions": [
    { "id": "R1-01", "status": "fixed", "commit": "sha or null", "note": "what changed, or the argument against changing it" }
  ]
}
```

One entry per finding it was given, `status` one of `fixed`, `rejected`,
`deferred`.

`gaps` — written by `coverage-k`:

```json
{
  "round": 1,
  "gaps": [
    { "id": "C1-01", "area": "...", "what_is_untested": "...", "test_to_add": "file and case", "severity": "major" }
  ]
}
```

`report` — written by `implement` and `docs-k`:

```json
{
  "summary": "what changed, in behaviour terms",
  "commits": ["sha"],
  "files": ["path/one.rs"],
  "ci": { "cargo test --workspace": "pass" },
  "notes": "deferrals and assumptions"
}
```

The schemas are shipped as prompt fragments under `prompt_templates/` and
referenced from `states.yaml` with `prompt_template:`, so the two reviewers
share one contract and each state adds only its own scope.

## How the supervisor decides

1. **Visit 1** — read `spec_path` and the project's `CLAUDE.md` / `AGENTS.md`,
   write `runtime/supervision/preparation.md` (acceptance criteria, risk areas,
   one paragraph of focus per role), brief `deliver.implement`, return.
2. **After `implement`** — brief `deliver.review-1` *and* `deliver.pm-1`. They
   run concurrently; this is the only pair briefed together.
3. **After both `review-k` and `pm-k`** — brief `deliver.fix-k`, naming which
   finding ids it must fix. The supervisor may downgrade or skip a finding, but
   the brief says which and why.
4. **After `fix-k`** — if any finding of severity `major` or `blocker` is not
   `fixed` and `k < review_rounds`, brief `review-(k+1)` and `pm-(k+1)`.
   Otherwise cancel every remaining review/product/fix round and brief
   `coverage-1`.
5. **After `coverage-k`** — brief `coverage-fix-k`. **After `coverage-fix-k`** —
   another coverage round, or cancel the rest and brief `docs-1`.
6. **After `docs-k`** — another documentation round, or finish: write
   `runtime/results/<deliver-id>.md` (what shipped, rounds used and cancelled,
   findings by `fixed` / `rejected` / `deferred`, CI status) and return, so the
   `openDescendants < 1` edge fires.

Rules the supervisor does not bend: brief one phase at a time; never cancel a
step that has already started; every cancel carries
`rhei transition <id> --from <current-state> --to cancelled --result "<why>"`;
never transition its own task. `--from` is the compare-and-swap guard and is
required — the child's current state is the one in brackets beside it in the
supervisor's task list, so a cancel needs no extra read.

**`fix-1` and `coverage-fix-1` are never cancelled.** A cancelled task does not
satisfy anyone's prerequisite (§FS-rhei-states.1.4), so the coverage phase is
chained to `fix-1` and the documentation phase to `coverage-fix-1` — the two
steps that always run. Chaining them to the *last* round instead would strand
the next phase the moment the supervisor cancelled a round it did not need.

## Run it with `--parallel 2`

```bash
rhei run <workspace> --parallel 2
```

`review-k` and `pm-k` are different states with the same `**Prior:**`, so
`--parallel 2` overlaps them. With `--parallel 1` the workspace still runs
correctly; the two reviews serialize and the supervisor spends one extra visit
per round, which its visit budget already allows for.

## The snapshot caveat

A supervisor is at its best when each visit continues the previous transcript,
which needs `snapshot:` — and of the built-in agent profiles only `pi` declares
a snapshot session layout, through a `target:` that resolves both a provider
and a model. `claude-code`, `codex`, `gemini`, `cursor`, and `kilocode` reject
the block as a hard `unsupported-snapshot-session` validation error. So the
block is emitted only when `supervisor_session=true`, and the default is
`false`: with `claude-code` the supervisor runs **each visit cold**, carried by
its checkpoints, its briefs, and the preparation note it wrote on visit 1. That
is why this template writes so much down.

## Instantiate

```bash
rhei instantiate supervised-delivery docs/functional-spec/rhei-run.spec.md \
  --set title="Deliver detached runs" \
  --set review_rounds=2 \
  --set supervisor_target=pi:anthropic:claude-sonnet-4-5 \
  --set supervisor_session=true \
  --output panta/deliver-detached-runs
```

Array inputs need a values file:

```bash
rhei instantiate supervised-delivery \
  --values crates/rhei-cli/templates/supervised-delivery/.example-values.yaml \
  --output panta/my-delivery
```

## Example

A pre-rendered instantiation is checked in at
[`examples/supervised-delivery-example/`](../../../../examples/supervised-delivery-example/).
