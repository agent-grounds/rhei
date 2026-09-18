# FS-rhei-summary: `rhei summary`

Read-only Markdown summary of a run, compact enough to paste into a pull
request body: one numbered line per agent invocation the accounting recorded,
then the aggregate token accounting. No local paths, no workspace boilerplate,
and no dependency on a finished run — a step of a live run can summarize the
run it belongs to. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)
[§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)

The existing surfaces each miss this use. `rhei render --format github`
renders the whole plan — workspace boilerplate and full task content. The
per-run report ([§FS-rhei-run-report](rhei-run-report.spec.md#fs-rhei-run-report-per-run-report)) is written only at the end of a run,
covers one `rhei run` session rather than the workspace, and links local log
paths a pull request must not carry. `rhei cost` prints totals but no step
list. The raw material for all of it is already durable under
`runtime/accounting/invocations/`.

## 1. Usage

```bash
rhei summary [RHEI_PLAN_OR_WORKSPACE] [--details] [--rhei <ID>]
rhei summary [RHEI_PLAN_OR_WORKSPACE] --run <ID> --prices <BOOK> [--details] [--rhei <ID>]
```

The positional resolves exactly as `rhei cost`'s does: a plan file, a
workspace directory, or — omitted — the nearest enclosing project, workspace,
or lone plan. `--rhei <ID>` (repeatable) narrows it the same way too. Scope is
one thing for both commands, which is why they move together: the accounting
roots the positional and the flag select are
[§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary). The command reads the plan and those roots and writes
Markdown to stdout. It never writes files, never spawns anything, and never
estimates: a fact that was not recorded is omitted, not guessed.

`--run` and `--prices` are one opt-in operation and must be supplied together;
either flag alone is a usage error. `--run` names one exact durable run id, not
a prefix, and `--prices` names a local `rhei.accounting.prices.v1` book under
the validation rules of §FS-rhei-cost-accounting.5.1. The positional and
repeatable `--rhei` options resolve the accounting roots first; the run
selection stays inside them.

This explicit form summarizes recorded work only. The exact run must have
invocation records in the selected roots and one associated immutable history
entry under `runtime/run-reports/` that proves the run ended. The overwriteable
`runtime/run-report.md` is not sufficient by itself. A descriptor that is
still live, or a held run lock identifying the run, overrides the history
entry and makes the run ineligible while active. The operation reads and
reprices existing measurements in memory: it does not spawn an agent,
remeasure usage, write or migrate an invocation, replace stored pricing, or
copy the supplied book into an accounting root. Ordinary summary invocations
retain the live-workspace behavior above and need no finished-run evidence.

## 2. Output

Three parts, in order.

### 2.1. The lead line

One sentence naming the resolved state machine, the invocation count, the
distinct models, and the task tally:

```text
`supervised-ticket-fix` workflow: 7 agent invocations across 2 models; 4 tasks completed, 4 cancelled.
```

- The workflow name is the resolved state machine's `name:`.
- Agent invocations are the records under the accounting roots the invocation's
  scope selects ([§FS-rhei-panta.6.5](rhei-panta.spec.md#65-cost-and-summary)); the model count is the distinct `model`
  values among them.
- The task tally counts the tasks **of that same scope** per terminal state, in
  machine declaration order; when non-terminal tasks exist, `, N in progress` is
  appended, so a mid-run summary says it is one. One sentence must not describe
  two scopes: `rhei summary <member>` counting the member's invocations beside
  the whole project's tasks reads as a summary of neither.
- With explicit `--run`/`--prices`, the sentence names the exact run id and its
  invocation and model counts. Its task tally covers only task ids represented
  by that run's selected invocation records, using the states recorded by
  those invocations; current tasks, unrelated tasks, and other runs do not
  enter the sentence.

### 2.2. The steps

One numbered entry per invocation record, ordered by `started_at`:

```text
1. `ticket.ticket` supervising (visit 1) — claude-code, anthropic/claude-fable-5 — 2m32s
2. `ticket.ticket.implement` implement — claude-code, anthropic/claude-sonnet-5 — 18m04s — 41.2k in / 3.8k out
```

- The entry carries the task id, the state, the agent, `provider/model`, the
  wall-clock duration, and — only when the record's totals are measured —
  humanized input/output token counts.
- `(visit N)` is printed when the record's `visit` is greater than 1 or when
  more than one record shares the task id, so repeated supervisor visits are
  distinguishable and one-shot steps stay clean.
- Duration is `ended_at - started_at`, humanized (`18m04s`); omitted when
  either timestamp is missing.
- With explicit `--run`/`--prices`, entries come only from that exact run after
  accounting-root filtering and record-identity deduplication. The same
  selected records supply the lead, steps, and accounting table.

### 2.3. The accounting

The aggregate over every record uses the per-run accounting strip's ordered
token rows ([§FS-rhei-run-report.2.1](rhei-run-report.spec.md#21-accounting-strip)):
`total tokens`, `input tokens (incl. cache)`, `input cache read`, `input cache
write`, `output tokens (incl. cache)`, `output cache read`, and `output cache
write`. Cost when priced remains before those rows and coverage remains after
them. Cache parts are already included in their side's total
([§FS-rhei-cost-accounting.3.1](rhei-cost-accounting.spec.md#31-token-dimensions))
and are not added again; an unavailable cache dimension reads `-`, while a
measured zero reads `0`. When no record carries a measured total the table is
replaced by one line:

```text
Token accounting was not measured for this run.
```

Pricing and coverage semantics are the accounting spec's
([§FS-rhei-cost-accounting.5](rhei-cost-accounting.spec.md#5-pricing)); this command adds no pricing of its own.

An explicit alternate-book reading adds these rows before its token rows:

| `Accounting` row | Value |
| --- | --- |
| `price book` | The supplied book's `price_book_id`. |
| `currency` | The supplied book's currency. |
| `pricing` | `priced`, `partial-price`, or `unpriced`. |

When pricing is `priced`, the ordinary `cost` row follows. When it is
`partial-price`, the money row is instead `priced cost (lower bound)`. When it
is `unpriced`, no money row appears: missing replacement rates are not a zero
cost and never fall back to a stored amount. §FS-rhei-cost-accounting.6.2

## 3. `--details`

Wraps the whole output in one collapsed block for a pull request body: the
lead line becomes the `<summary>`, prefixed `AI workflow: `, and the steps and
accounting follow inside, with a blank line after `</summary>` so GitHub
renders the Markdown within:

```text
<details>
<summary>AI workflow: `supervised-ticket-fix`, 7 agent invocations across 2 models; 4 tasks completed, 4 cancelled.</summary>

1. `ticket.ticket` supervising (visit 1) — ...
...

</details>
```

The wrapper is unchanged for explicit `--run`/`--prices`: its inner lead,
steps, provenance, and accounting are the selected-run reading from §2.

## 4. What the summary never contains

1. Local filesystem paths — no log files, workspace directories, or
   home-relative paths; the output must be publishable verbatim. This is why
   `rhei summary` gains `cost`'s scope and its `--rhei` flag but **never** its
   roots line ([§FS-rhei-cost-accounting.8](rhei-cost-accounting.spec.md#8-cli-inspection)): that line names directories, and
   this output goes into a pull request body. An empty summary stays §5's. The
   caller-supplied price-book path is a local filesystem path too; only the
   validated book id and currency may appear in successful stdout.
2. Task content — no briefs, no export bodies, no result text. Task ids and
   states only.
3. Estimated numbers — an unmeasured record contributes no token line, and an
   unpriced run shows no cost.

## 5. Empty and error cases

1. No accounting directory, or no invocation records: the lead line still
   prints (zero invocations, the task tally from the plan) followed by the
   unmeasured line; exit 0. A freshly instantiated workspace is summarizable.
2. A positional that resolves to no plan or workspace fails exactly as
   `rhei cost` does, with the same guidance.
3. In explicit `--run`/`--prices` mode, each of these is a nonzero actionable
   error before Markdown is written:
   - no exact matching run id in the resolved project is **absent**; a prefix
     is not an exact match;
   - an exact run found in that project but only outside the roots selected by
     the positional and `--rhei` is **outside scope**;
   - matching records without one immutable history report are **completion
     not established**;
   - distinct immutable history reports that claim the same exact id are
     **ambiguous**; and
   - live-descriptor or held-lock evidence makes the run **active**, even if a
     matching history report exists.
4. The book is loaded and validated before any Markdown is emitted. Missing,
   unreadable, malformed, wrong-schema, duplicate-entry, and otherwise invalid
   books use §FS-rhei-cost-accounting.5.1's diagnostic that names the supplied
   path. Missing exact rates inside a valid book are accounting outcomes under
   §2.3, not input errors.
