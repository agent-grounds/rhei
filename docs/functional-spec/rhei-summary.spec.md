# FS-rhei-summary: `rhei summary`

Read-only Markdown summary of a run, compact enough to paste into a pull
request body: one numbered line per agent invocation the accounting recorded,
then the aggregate token accounting. No local paths, no workspace boilerplate,
and no dependency on a finished run — a step of a live run can summarize the
run it belongs to. [§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)
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
rhei summary [RHEI_PLAN_OR_WORKSPACE] --print-checkpoint [--rhei <ID>]
rhei summary [RHEI_PLAN_OR_WORKSPACE] --checkpoint <FILE|-> [--details] [--rhei <ID>]
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
the validation rules of [§FS-rhei-cost-accounting.5.1](rhei-cost-accounting.spec.md#51-price-book-selection). The positional and
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

`--print-checkpoint` and `--checkpoint` freeze a summary so it can be printed
again later. An ordinary summary spans the workspace's whole history, so the
block a `describe` step printed is not the block printed after `ship` has added
records of its own. `--print-checkpoint` prints, instead of Markdown, the
checkpoint document of [§FS-rhei-summary.6](rhei-summary.spec.md#6-the-checkpoint-document): exactly the set of records, spawns
and task states the ordinary summary would cover at that moment. It writes no
file; the caller redirects stdout. `--checkpoint <FILE>` (or `-` for stdin)
prints the summary of exactly that set, inside the roots the positional and
`--rhei` select, whatever has been recorded since:

- the lead, the steps, the accounting and the provenance line are drawn from the
  set's members alone;
- the task tally counts the task states the document recorded, not the plan's
  current states, so a replay prints what the live summary printed when the
  document was taken — byte for byte, provenance line included;
- an unrecorded spawn in the set enters the steps, the invocation count and the
  gap count by its own `task` and `state`, as [§FS-rhei-summary.2.2](rhei-summary.spec.md#22-the-steps) places one.

A spawn record is written when its spawn ends ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)) and an
invocation record when its invocation ends, so the step that runs
`--print-checkpoint`, and any sibling still running, are not in the set. That
is what lets a `describe` step take the document and a later `ship` replay it
with neither of their own records in it. A time cutoff could not: describe's
own record is written after it started, so a cutoff taken at describe admits it.

`--print-checkpoint` and `--checkpoint` refuse each other, and each refuses
`--run` and `--prices`, as usage errors that name both flags.
`--print-checkpoint` refuses `--details` too, because it prints no Markdown.
The `--run`/`--prices` pairing is untouched.

## 2. Output

Four parts, in this order, numbered as they print: the lead, the steps, the
accounting, and the provenance line.

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
- When agent spawns in that scope have no accounting record
  ([§FS-rhei-cost-accounting.6.2.1](rhei-cost-accounting.spec.md#621-agent-spawns-with-no-record)), the invocation count is the known agent
  invocations rather than the records, and the gap follows the model count,
  which still counts the records' models:
  `` `mixed-agents` workflow: 4 agent invocations across 1 model, 2 of them with no accounting record; 2 tasks completed. ``
  The records alone are never presented as the run's invocation count.
- The task tally counts the tasks **of that same scope** per terminal state, in
  machine declaration order; when non-terminal tasks exist, `, N in progress` is
  appended, so a mid-run summary says it is one. One sentence must not describe
  two scopes: `rhei summary <member>` counting the member's invocations beside
  the whole project's tasks reads as a summary of neither.
- For a Panta project, each in-scope task is classified by the state machine of
  the rhei that owns it. If the scope contains tasks from more than one
  distinct machine, the command refuses rather than combining incompatible
  terminal definitions; use `--rhei <ID>` to produce one summary per machine.
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
- An agent spawn with no accounting record ([§FS-rhei-cost-accounting.6.2.1](rhei-cost-accounting.spec.md#621-agent-spawns-with-no-record))
  is an entry too, placed by its `started`. It carries the task id, the state,
  the spawn's `worker`, its duration from `started` and `ended`, and the words
  `no accounting record` in place of a model and token counts, which the spawn
  does not know. It carries no `(visit N)`: a spawn record names no visit, and
  its file name is never matched to find one ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)):
  `` 3. `plan.1` implement — cdx — 1s — no accounting record ``
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

When agent spawns in scope have no accounting record, the coverage row says
how many, after the coverage word that [§FS-rhei-cost-accounting.6.2.1](rhei-cost-accounting.spec.md#621-agent-spawns-with-no-record) demoted:

```text
| coverage | Partial (2 of 4 agent invocations have no accounting record) |
```

A summary with nothing missing prints the row as before.

An explicit alternate-book reading adds these rows before its token rows:

| `Accounting` row | Value |
| --- | --- |
| `price book` | The supplied book's `price_book_id`. |
| `currency` | The supplied book's currency. |
| `pricing` | `priced`, `partial-price`, or `unpriced`. |

When pricing is `priced`, the ordinary `cost` row follows. When it is
`partial-price`, the money row is instead `priced cost (lower bound)`. When it
is `unpriced`, no money row appears: missing replacement rates are not a zero
cost and never fall back to a stored amount. [§FS-rhei-cost-accounting.6.2](rhei-cost-accounting.spec.md#62-coverage-of-a-selection)

### 2.4. The provenance line

The last part of every summary, after a blank line: which rhei build ran,
which commit the checkout started on, which template the plan was laid from,
and which set of records the summary covers.

```text
Provenance: rhei 0.6.4-dev @ d0446a7 · checkout HEAD at start agent-grounds/rhei@a1b2c3d · template `supervised-ticket-fix` 1.0.0 @ agent-grounds/agent-grounds@d292a9b (clean) · checkpoint 3f9a2c41d0b7: run 55177a, 7 invocations
```

`Provenance: rhei <build> · checkout HEAD at start <head> · template <tmpl> ·
checkpoint <id>: <runs><n> invocation[s]`, with its fields read so:

- **`<build>`** is `<version> @ <sha7>`, with ` (dirty)` after it for a dirty
  build, or `<version> @ unknown`, read from each member record's `rhei_build`
  ([§FS-rhei-cost-accounting.3.5](rhei-cost-accounting.spec.md#35-run-attribution)). Distinct builds are joined with `, ` in the
  order of their first use. A member with no `rhei_build`, and an unrecorded
  spawn, contribute `unknown`.
- **`<head>`** is the `checkout_head_at_start` of the member record that started
  first: `<repo>@<sha7>`, `<sha7>` when its `repo` is `null`, or `unknown`
  when that record carries none.
- **`<tmpl>`** is read from the `laid` fence ([§FS-rhei-library.7.1](rhei-library.spec.md#71-the-fence-comment-is-the-whole-of-template-provenance)) of the
  state machine the lead names. With a commit it reads
  `` `<name>` <version> @ <repo>@<sha7> (<state>) `` — `@ <sha7>` when the
  fence's `git:` names no repository; with no commit it reads
  `` `<name>` <version> src:sha256:<12-hex> (<state>) ``; a built-in template
  reads `` `<name>` <version> (built into rhei) ``; and a machine with no `laid`
  fence — one typed by hand, or laid before fences were written — reads
  `unknown`.
- **`<id>`** is the checkpoint id of [§FS-rhei-summary.6.2](rhei-summary.spec.md#62-canonical-bytes-and-the-checkpoint-id), the same whether the
  summary is live or replayed.
- **`<runs>`** names the distinct `run_id`s of the member records, each cut to
  its first six characters, in the order the runs started: `run 55177a, ` for
  one and `runs 55177a, 9b1c03, ` for several. Records with no `run_id` and
  unrecorded spawns name no run, and when no member names one `<runs>` is
  empty. `<n>` counts every member, unrecorded spawns included.

It is the last part rather than one between the steps and the accounting so
that the accounting keeps its number. With `--details` it sits inside the
block, before `</details>`. It is provenance under a trusted orchestrator: it
says what rhei recorded, and it detects no drift and seals nothing.
A strict run detects drift ([§FS-rhei-run.3.8](rhei-run.spec.md#38-strict-inputs)); this line does not.

## 3. `--details`

Wraps the whole output in one collapsed block for a pull request body: the
lead line becomes the `<summary>`, prefixed `AI workflow: `, and the steps, the
accounting and the provenance line follow inside, with a blank line after `</summary>` so GitHub
renders the Markdown within:

```text
<details>
<summary>AI workflow: `supervised-ticket-fix`, 7 agent invocations across 2 models; 4 tasks completed, 4 cancelled.</summary>

1. `ticket.ticket` supervising (visit 1) — ...
...

Provenance: rhei 0.6.4-dev @ d0446a7 · ...
</details>
```

The wrapper is unchanged for explicit `--run`/`--prices`: its inner lead,
steps, price-book rows, accounting, and provenance line are the selected-run reading from [§FS-rhei-summary.2](rhei-summary.spec.md#2-output).

## 4. What the summary never contains

1. Local filesystem paths — no log files, workspace directories, or
   home-relative paths; the output must be publishable verbatim. This is why
   `rhei summary` gains `cost`'s scope and its `--rhei` flag but **never** its
   roots line ([§FS-rhei-cost-accounting.8](rhei-cost-accounting.spec.md#8-cli-inspection)): that line names directories, and
   this output goes into a pull request body. An empty summary stays [§FS-rhei-summary.5](rhei-summary.spec.md#5-empty-and-error-cases)'s. The
   caller-supplied price-book path is a local filesystem path too; only the
   validated book id and currency may appear in successful stdout, and the
   checkpoint file's path never does. A remote the provenance line names is
   **reduced** to `owner/repo`, its host kept in front when it is not
   `github.com` (`gitlab.example.org/owner/repo`), with the scheme, any
   credentials, a `.git` suffix and an `ssh` user stripped. A remote that is a
   local path or a `file://` URL reduces to nothing, and the line prints the
   bare commit.
2. Task content — no briefs, no export bodies, no result text. Task ids and
   states only.
3. Estimated numbers — an unmeasured record contributes no token line, and an
   unpriced run shows no cost.

## 5. Empty and error cases

1. No accounting directory, or no invocation records: the lead line still
   prints (zero invocations, the task tally from the plan) followed by the
   unmeasured line and a provenance line that names only the template,
   `Provenance: template <tmpl> · no invocations recorded`; exit 0. A freshly
   instantiated workspace is summarizable.
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
   books use [§FS-rhei-cost-accounting.5.1](rhei-cost-accounting.spec.md#51-price-book-selection)'s diagnostic that names the supplied
   path. Missing exact rates inside a valid book are accounting outcomes under
   [§FS-rhei-summary.2.3](rhei-summary.spec.md#23-the-accounting), not input errors.
5. With `--checkpoint`, each of these is a nonzero actionable error before
   Markdown is written, naming the checkpoint file as given:
   - a file that cannot be read, is not JSON, or whose `schema` is not
     `rhei.summary.checkpoint.v1`;
   - a **missing member** — an invocation id no record in the selected roots
     carries, or a spawn whose record is gone — named by its id or its spawn
     file name. Invocation records survive only a narrowed `rhei reset`: a full
     reset deletes all of `runtime/`, accounting included, so a checkpoint
     taken before one fails here. A spawn record is removed by any reset and
     rewritten by a later attempt of its visit, so it can vanish sooner.

   The refused flag combinations of [§FS-rhei-summary.1](rhei-summary.spec.md#1-usage) are usage errors.

## 6. The checkpoint document

What `--print-checkpoint` prints and `--checkpoint` reads: one JSON object,
`rhei.summary.checkpoint.v1`, whose schema is
`crates/rhei-cli/schemas/rhei.summary.checkpoint.v1.schema.json`.

### 6.1. Its members

```json
{"invocations":["0f3a…","9b1c…"],"schema":"rhei.summary.checkpoint.v1","spawns":[{"attempt":1,"moves":4,"spawn":"task-ticket.review-review.json","state":"review","task":"ticket.review"}],"tasks":[{"id":"ticket","state":"supervising"},{"id":"ticket.review","state":"completed"}]}
```

- `invocations` — the sorted `invocation_id`s of the records in scope.
- `spawns` — each unrecorded agent spawn in scope ([§FS-rhei-cost-accounting.6.2.1](rhei-cost-accounting.spec.md#621-agent-spawns-with-no-record)),
  as `{task, state, moves, attempt, spawn}`, sorted by `spawn`. `spawn` is the
  spawn record's file name under `runtime/spawns/`, which is the only thing that
  tells two entries of one fanned-out state apart ([§FS-rhei-agents.8.4](rhei-agents.spec.md#84-spawn-records)); the
  other four are what the replay prints and tallies, and its `started`,
  `ended` and `worker` are read from that record again on replay.
- `tasks` — every task in scope, as `{id, state}` with the state it was in at
  that moment, sorted by `id`. These are what a replay's task tally counts.

It names no path, so it may be published or stored beside the pull request.

### 6.2. Canonical bytes and the checkpoint id

The document is printed in its canonical encoding: keys in sorted order, no
insignificant whitespace, arrays sorted as [§FS-rhei-summary.6.1](rhei-summary.spec.md#61-its-members) says, and one
trailing newline. The **checkpoint id** is the first 12 hex digits of the
sha256 of exactly those bytes. A live summary computes the id of the document
`--print-checkpoint` would print at that moment, and a replay computes it from
the document it reads re-encoded canonically, so the two agree.
