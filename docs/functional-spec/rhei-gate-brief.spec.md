# FS-rhei-gate-brief: Gate Briefs

A gating state ([§FS-rhei-states.1.2](rhei-states.spec.md#12-per-state-fields)) is where a ticket waits on a person.
Until now the machine said nothing about what that person is asked: the state
waited, and whoever came by moved the ticket with `rhei transition` and one
typed reason. A gate brief makes the ask part of the workflow. The state names
the file to show, the questions to put, and which answer leads where, and rhei
checks all three before a person is asked anything. A caller then reads one
brief and sends one answer, whatever the workflow is, and the workflow — not
the caller — decides what the answer means.
[§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)

The brief is read and answered through `rhei gate` ([§FS-rhei-gate-cmd](rhei-gate-cmd.spec.md#fs-rhei-gate-cmd-rhei-gate)). Every
path in a brief, a request, or an answer is relative to the ticket's execution
root ([§FS-rhei-plan-language.3.10](rhei-plan-language.spec.md#310-state-artifact-contracts)); JSON uses snake_case keys and carries
`"version": 1`, and a reader refuses any other version.

## 1. Declaring a Gate

A `gating: true` state may carry a `gate:` block:

```yaml
human-judgment:
  gating: true
  gate:
    title: "Rule on the agora's disputed entries"
    summary: "The discussion for {task_title} finished; rule on what is left."
    display: runtime/agora/verdict.md
    request: runtime/gates/{task_id}.request.json
    routes:
      - { to: shipping, when: { approve: true } }
      - { to: revising, when: { approve: false } }
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `title` | string | No | One-line title of the ask. A template ([§FS-rhei-gate-brief.1.1](rhei-gate-brief.spec.md#11-template-variables)). The request's title wins over it ([§FS-rhei-gate-brief.4.3](rhei-gate-brief.spec.md#43-title)). |
| `summary` | string | No | What was done and what is asked, in prose. A template. The request's summary wins over it. |
| `display` | string | No | Path template of the one file the person reads before answering. An implicit required input ([§FS-rhei-gate-brief.2](rhei-gate-brief.spec.md#2-implicit-inputs)). |
| `request` | string | No | Path template of the request file the state before the gate writes ([§FS-rhei-gate-brief.3](rhei-gate-brief.spec.md#3-the-request-file)). An implicit required input. Excludes `questions`. |
| `questions` | Question array | No | Static questions, in the request's question format ([§FS-rhei-gate-brief.3.1](rhei-gate-brief.spec.md#31-question)). Excludes `request`. |
| `routes` | Route array | No | Which answer leads to which destination ([§FS-rhei-gate-brief.5](rhei-gate-brief.spec.md#5-routes)). |

A state without the block is still a gate. Its brief is synthesized
([§FS-rhei-gate-brief.4.1](rhei-gate-brief.spec.md#41-synthesized-questions)), so every gate can be shown and answered the same way.

### 1.1. Template Variables

`display` and `request` are paths, and take the artifact-path variables that
name the ticket and the state: `{task_id}` (project-qualified), `{task_id_local}`,
`{rhei_id}` and `{state}` ([§FS-rhei-states.3](rhei-states.spec.md#3-artifact-contracts)). The variables an artifact path
takes from an execution identity or a counted visit — `{visit_count}`,
`{target}`, `{agent}`, `{model}` and their parts — have no value at a gate,
which runs nothing and is read before and after any visit is counted, so a gate
path refuses them.

`title` and `summary` are prose, and take those four plus the instruction
variables a gate can resolve without a run: `{task_title}`, `{plan_title}`,
`{plan_path}` and `{meta.<key>}` ([§FS-rhei-states.4.1](rhei-states.spec.md#41-variable-namespace)).

An identifier-shaped `{name}` in any of the four fields that is not in its list
is a validation error when the machine loads, naming the field, the state and
the variable. Brace content that is not identifier-shaped is literal text. A
`{meta.<key>}` the ticket does not carry is left as written, as it is in
instructions ([§FS-rhei-states.4.2](rhei-states.spec.md#42-resolution-rules)): which keys a ticket carries is not known when
the machine loads.

### 1.2. Validation

Each of these is an error when the machine loads, and every command that reads
the machine refuses it:

1. `gate:` on a state that is not `gating: true`.
2. A field the table above does not name, or a field of the wrong type.
3. `request` and `questions` both present.
4. `display` or `request` empty, absolute, or escaping the execution root with
   `..` — the rule artifact paths follow ([§FS-rhei-states.3](rhei-states.spec.md#3-artifact-contracts)).
5. A template variable outside its field's list ([§FS-rhei-gate-brief.1.1](rhei-gate-brief.spec.md#11-template-variables)).
6. A static question that breaks [§FS-rhei-gate-brief.3.1](rhei-gate-brief.spec.md#31-question), or `questions: []`.
7. A route whose `to` is not the target of an edge this state declares, an exact
   `from: <state>` edge or a `from: "*"` edge.
8. A `when` key that names no static question, names a `text` question, or
   expects a value of the wrong type or outside the question's options
   ([§FS-rhei-gate-brief.5.1](rhei-gate-brief.spec.md#51-matching)). Under `request`, the keys are checked against the
   request on entry instead ([§FS-rhei-gate-brief.2](rhei-gate-brief.spec.md#2-implicit-inputs)), since its questions do not exist
   yet.

## 2. Implicit Inputs

`display` and `request` are required inputs of the gate state that the author
does not list under `inputs:`. They are checked where the state's declared
inputs are checked, on every path into the state — `rhei transition`,
`rhei complete`, `rhei run`, `rhei next` and `rhei gate answer` — and a missing
file refuses the edge exactly as a missing declared input does
([§FS-rhei-states.3](rhei-states.spec.md#3-artifact-contracts)). A gate cannot be reached with nothing to show.

On the same edge, after the files are found, the request is read and checked:

1. It parses as JSON and satisfies [§FS-rhei-gate-brief.3](rhei-gate-brief.spec.md#3-the-request-file).
2. It carries `display` only when the block declares none, and the file it names
   exists.
3. Every route's `when` holds against the request's questions, by the rule
   static questions are held to at load ([§FS-rhei-gate-brief.1.2](rhei-gate-brief.spec.md#12-validation)).

A request that fails refuses the edge, and the refusal lists every problem
found, not only the first. The ticket stays in the state that wrote the request,
so a malformed request never reaches the person: no gate is entered, and nobody
is asked to answer it. Under `rhei run` the refusal is an edge a finished visit
could not take, as a missing declared input is: the run warns that the ticket
could not advance, with every problem, and leaves it stalled in the writer's
state for the rest of the run, whose report says so ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)). Within
the run the writer gets no further attempt. A later `rhei run` gives it the next
attempt of the same visit only while the visit's completion condition is unmet
([§FS-rhei-agents.3.2](rhei-agents.spec.md#32-completion-condition)) and its attempt budget lasts ([§FS-rhei-agents.3.2.3](rhei-agents.spec.md#323-attempt-budget)). A
request the writer declares as an output meets that condition, so such a request
is repaired by hand, or removed so that the next attempt writes it again, before
the gate can be entered.

## 3. The Request File

The state before the gate writes the request when the questions depend on the
work — one question per disputed entry, say:

```json
{
  "version": 1,
  "title": "Rule on 3 disputed entries",
  "summary": "One paragraph on what was done and what is being asked.",
  "display": "runtime/agora/verdict.md",
  "attachments": ["runtime/agora/charter.md"],
  "questions": [
    { "id": "d1", "kind": "choice", "prompt": "Entry 4: cache per plan or per root?",
      "options": [ { "value": "plan", "label": "Per plan" },
                   { "value": "root", "label": "Per root" } ] },
    { "id": "approve", "kind": "boolean", "prompt": "Accept the verdict as the record?" },
    { "id": "note", "kind": "text", "prompt": "Reasoning to carry forward", "required": false }
  ]
}
```

| Field | Type | Required | Rule |
|-------|------|----------|------|
| `version` | integer | Yes | `1` |
| `title` | string | Yes | Non-empty after trimming, one line |
| `summary` | string | No | Free text |
| `display` | string | No | Allowed only when the block declares no `display`; must exist on entry |
| `attachments` | string array | No | Paths that need not exist; `rhei gate show` says whether each is readable |
| `questions` | Question array | Yes | At least one; ids unique |

Paths follow [§FS-rhei-gate-brief.1.2](rhei-gate-brief.spec.md#12-validation)'s rule: relative, and not escaping the root.
Unknown fields are refused, here and in every nested object, so a misspelt
field is an error rather than a silent default.

### 3.1. Question

| Field | Type | Required | Rule |
|-------|------|----------|------|
| `id` | string | Yes | `^[a-z][a-z0-9_-]*$`, unique in the gate, and not a reserved id |
| `kind` | string | Yes | `boolean`, `choice`, `multi` or `text` |
| `prompt` | string | Yes | Non-empty after trimming |
| `required` | boolean | No | Default `true` |
| `options` | Option array | For `choice` and `multi` only | `choice` needs at least two, `multi` at least one; values unique. Refused on `boolean` and `text`. |
| `help` | string | No | A longer explanation shown under the prompt |

The reserved ids are `destination` and `reason`, the two questions rhei asks
itself ([§FS-rhei-gate-brief.4.1](rhei-gate-brief.spec.md#41-synthesized-questions)). Four kinds only: they are the kinds a form can draw
without knowing the workflow.

### 3.2. Option

An option is `{ "value": <string>, "label": <string>, "description": <string> }`.
`value` is required and non-empty; `label` defaults to `value`; `description` is
optional.

## 4. The Brief

The brief is what a person is shown at one visit of one ticket to a gate: the
title, the summary, the display file, the attachments, the questions, the
destinations and the routes. Its questions come from one of three **sources**:

- `request` — the block names a request file, and it is present and valid.
- `questions` — the block declares static questions.
- `synthesized` — there is no block, the block asks nothing, or the request is
  missing or invalid ([§FS-rhei-gate-brief.4.2](rhei-gate-brief.spec.md#42-degraded-brief)).

A question from the first two sources is **authored**.

### 4.1. Synthesized Questions

So that every caller has one code path, a brief always has questions and every
gate takes an answer:

- When the brief has no routes — the block declares none, or there is no
  block — rhei appends a required `choice` question with id `destination` and
  prompt `Where should <ticket> go?`. Its options are the distinct targets of
  the state's declared outgoing edges, exact and wildcard, in declaration
  order: `value` and `label` are the state name, and `description` is the
  target state's description. The answer's `destination` is the target.
- When the brief has no authored questions, rhei also appends a required `text`
  question with id `reason` and prompt `Why?`: today's one typed reason, as a
  question.

A brief from no block at all therefore asks exactly `destination` and `reason`.

### 4.2. Degraded Brief

A block whose `request` file is missing or invalid **when the brief is read** —
a ticket that entered the gate before its machine gained the block, or a file
removed by hand since — does not leave the person with a gate they cannot move.
The brief degrades to the synthesized one: `destination` and `reason`, no
routes, source `synthesized`. Each problem with the request is stated in the
brief's `problems` ([§FS-rhei-gate-cmd.2.1](rhei-gate-cmd.spec.md#21-output---json)), and `rhei gate answer` accepts the
degraded brief. The block's `display`, `title` and `summary` still apply.

### 4.3. Title

The brief's title is the request's `title`, then the block's `title`, then the
ticket's title, first non-empty wins. Its summary is the request's `summary`,
then the block's `summary`, else none.

### 4.4. Visit and Since

`visit` is how many times this ticket has entered this state, counted from the
durable transition ledger ([§FS-rhei-complete.3.1](rhei-complete.spec.md#31-state-transition-ledger)) of the ticket's execution root
and, for a project, of the project root: one per `<task-id> <from>@<state>`
movement. It is at least 1, so a ticket authored in the gate is at its first
visit. A reset that removes the ledger starts the count again. It is the entry
number agent logs are named by ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)), so a gate's visit and the
log of the visit that led to it are counted the same way.

`since` is when the ticket last entered the state, as an RFC 3339 UTC time, or
`null` when that is unknown. The ledger has no times, so it is read from the run
journal ([§FS-rhei-run-tui.1.7](rhei-run-tui.spec.md#17-journal-format)): the ticket's latest journal line, provided the
ledger's latest movement for the ticket is its entry into this state and that
line names that movement (`<from>→<state>`) or the end of the visit it left
(`end@<from>`). A move no run journalled — a manual transition — has no time.

### 4.5. Digest

The digest is `sha256:` and 64 lowercase hex digits, computed over the brief's
ticket, state, visit, title, display path, questions and routes. Callers copy it
and never compute it; an answer carries it back, and a brief that changed in
between — a rewritten request, a new visit — refuses the answer as `stale`
([§FS-rhei-gate-brief.6.2](rhei-gate-brief.spec.md#62-refusals)).

## 5. Routes

A route is `{ "to": <state>, "when": { <question id>: <expected value> } }`.
Routes are evaluated in declaration order and the first match wins. A route
matches when every key of its `when` matches; a missing or empty `when` always
matches, which makes it the fallback. No route matching refuses the answer as
`unrouted`, and nothing is written.

A gate with routes asks no `destination`: the routes decide. A gate without
them asks it, and the answer names the destination itself.

### 5.1. Matching

- `boolean` and `choice` match by equality. The expected value is a boolean for
  `boolean` and one of the question's option values for `choice`.
- `multi`: an expected string matches when the answer contains it; an expected
  array matches when the answer holds exactly that set. Every expected value is
  one of the options.
- `text` questions may not appear in `when`.
- An optional question that was not answered matches nothing.

## 6. The Answer

What a caller passes to `rhei gate answer` ([§FS-rhei-gate-cmd.3](rhei-gate-cmd.spec.md#3-rhei-gate-answer)):

```json
{
  "version": 1,
  "request_digest": "sha256:9f2c…",
  "answers": { "d1": "root", "approve": true, "note": "Keep the mapping on rhei's side." }
}
```

`request_digest` is required, and is the brief's digest copied unchanged.
Unknown fields are refused. The recording fields of
[§FS-rhei-gate-brief.7.1](rhei-gate-brief.spec.md#71-the-answer-file) are optional here, so the recorded file is an answer
document too.

### 6.1. Values

- `boolean` takes `true` or `false`; `choice` takes one option value; `multi`
  takes an array of distinct option values; `text` takes a string.
- A required question must be answered. A required `text` answer must be
  non-empty after trimming, and a required `multi` answer must hold at least one
  value.
- An optional question may be absent, and `null` counts as absent.
- An id the brief does not ask is refused.

### 6.2. Refusals

An answer is refused with one or more errors, each of one kind, all reported
together:

| Kind | Meaning |
|------|---------|
| `invalid` | The document's shape, an id, a kind, a missing required answer, or an option value |
| `stale` | `request_digest` differs from the current brief's digest |
| `conflict` | The ticket is not in `--from` |
| `unrouted` | No route matches |
| `not_a_gate` | The ticket's state is not a gating state |
| `refused` | The transition path refused the edge: a condition, a callback, an artifact, or the terminal result |

## 7. Recording

An accepted answer is recorded, and the destination is taken through the
ordinary transition path — compare-and-swap on `--from`, the edge's condition
and callbacks, source outputs, target inputs and the terminal-result check —
exactly as `rhei transition` takes it ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)). The
answer takes the declared edge of [§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)'s step 6;
it is never a forced move.

### 7.1. The Answer File

The answer is written to `runtime/gates/<task-id>/<state>.<visit>.answer.json`
under the ticket's execution root, with the project-qualified id and the gate's
canonical state name. The state is in the name because a ticket may pass more
than one gate and `visit` is counted per state
([§FS-rhei-gate-brief.4.4](rhei-gate-brief.spec.md#44-visit-and-since)), so two gates' first answers never share a file:

```json
{ "version": 1, "ticket": "agora.1", "state": "human-judgment", "visit": 1,
  "to": "completed", "route": { "index": 0, "when": { "approve": true } },
  "request_digest": "sha256:…", "answers": { "approve": true },
  "answered_by": "<OS account>", "answered_at": "2026-11-24T12:00:00Z" }
```

`route` is `null` when the destination came from the `destination` question.
`answered_by` is the OS account that ran the command, and `answered_at` the
RFC 3339 UTC time it answered. `to` is the target the ticket enters: the
destination, or the state an `on_leave` callback redirected it to.

The file is written under the plan lock, after every check of the transition has
passed — the compare-and-swap, the edge and its condition, the `on_leave`
callbacks, the source outputs, the target inputs and the terminal result — and
before the budget charge and the state write. So an `on_enter` callback can read
it, and an `on_leave` callback cannot. When the move is refused after the file is
written — by the budget, a failed state write, or an `on_enter` callback whose
failure rolls the state back — the file is removed before the lock is released,
and a file it replaced (a ledger reset restarts the visit count) is put back, so
a refused answer leaves no trace.

### 7.2. The Result Message

The answer, rendered, is the transition's result message: one line per answered
question, `<prompt>: <label or value>` (a `multi` answer's labels joined by
`, `), then a last line naming the answer file's path. A line break inside a
prompt, a label or a `text` answer is folded into one space, with the blank
around it, so a reader can take the lines one per question; the answer file
keeps the text as it was written. It is never empty, and it satisfies the
terminal-result obligation ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) exactly as `--result` does.

### 7.3. The `gate-answer` Input

From the state an answer moves a ticket into, onward, the ticket carries an
implicit optional input named `gate-answer` that resolves to its latest answer
file. `{input.gate-answer.path}`, `{input.gate-answer.exists}` and
`RHEI_INPUT_GATE_ANSWER_EXISTS` work as they do for any optional input
([§FS-rhei-states.3.1](rhei-states.spec.md#31-optional-inputs)), `{gate_answer}` is shorthand for the path, and the prompt's
handoff section lists the file. The agent that continues reads the ruling from
the plan, not from whoever relayed it.

- **Latest** is the file under `runtime/gates/<task-id>/` with the latest
  `answered_at`, then the latest modification time.
- Before any answer, `{input.gate-answer.exists}` is `false`, and
  `{input.gate-answer.path}` and `{gate_answer}` are left as written, as an
  unknown variable is. A program gets `RHEI_INPUT_GATE_ANSWER_EXISTS` always,
  and `RHEI_INPUT_GATE_ANSWER_PATH`, relative to the execution root, only when
  there is a file.
- A state that declares an input named `gate-answer` keeps its own:
  the declared one wins, and `{gate_answer}` is its path.
- `{if input.gate-answer.exists}` is valid in every state, declared or not
  ([§FS-rhei-states.4.2](rhei-states.spec.md#42-resolution-rules)).

#### 7.3.1. The Handoff Section

When the latest answer moved the ticket into the state it is now in — its `to` is
that state, and the ledger's latest move into that state came from the answer's
gate — the agent's prompt carries a section beside the declared handoffs:

```text
## Gate Answer from <state>

A person answered the `<state>` gate (visit <n>) and moved this task here.
The answer is recorded in `<path>`. ...
```

It names the file and, fenced as data, the answers it records. A file that an
exclusion keeps out of the prompt ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)) is named
without its answers.

## 8. Leaving a Gate by `rhei transition`

At a gate whose brief asks at least one **authored** required question,
`rhei transition --from <gate>` is refused, and the refusal names
`rhei gate answer <ticket> --from <gate> --answers <file|->` and
`rhei gate show <ticket>` ([§FS-rhei-transition-cmd.3.5](rhei-transition-cmd.spec.md#35-gates-with-a-brief)). The live dashboard's gate
release applies the same refusal ([§FS-rhei-viz.5.1.3](rhei-viz.spec.md#513-releasing-a-plain-gate)); the dashboard and the run TUI
offer an answer at such a gate in its place, through the core this command
uses ([§FS-rhei-viz.5.1.2](rhei-viz.spec.md#512-answering-a-briefed-gate) [§FS-rhei-run-tui.1.5.5](rhei-run-tui.spec.md#155-live-actions-intervene-and-human-gate)). `rhei complete`, which never
leaves a gate, names the same two commands when it refuses one
([§FS-rhei-complete.4](rhei-complete.spec.md#4-behavior)). A gate with no block, or whose brief asks only synthesized
questions — a degraded brief included — transitions exactly as before.
`--force` keeps its meaning ([§FS-rhei-transition-cmd.6](rhei-transition-cmd.spec.md#6-operator-forced-missing-edge-recovery)), and autonomous commands still
never leave a gating state.

## 9. Schemas

The request, the answer, the brief `rhei gate show --json` prints, the result
`rhei gate answer --json` prints, and the list `rhei gate list --json` prints
each have a published JSON Schema (draft 2020-12): `rhei.gate.request.v1`,
`rhei.gate.answer.v1`, `rhei.gate.show.v1`, `rhei.gate.result.v1` and
`rhei.gate.list.v1`. `rhei gate schema` prints them
([§FS-rhei-gate-cmd.5](rhei-gate-cmd.spec.md#5-rhei-gate-schema)), and `rhei schema` lists them beside the accounting schemas
([§FS-rhei-cost-accounting.8.1](rhei-cost-accounting.spec.md#81-published-accounting-schemas)).
