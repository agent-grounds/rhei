# FS-rhei-gate-cmd: `rhei gate`

Read and answer the brief of a ticket that waits at a gate
([§FS-rhei-gate-brief](rhei-gate-brief.spec.md#fs-rhei-gate-brief-gate-briefs)). `rhei gate` is the one surface every caller — a person at a
terminal, a dashboard, a script — uses to learn what a gate asks and to answer
it, so none of them needs to know what the workflow's states mean: the brief
says what is asked, and the machine's routes say where each answer leads.
[§GOAL-rhei-outcomes](../goals.md#goal-rhei-outcomes-goals)

## 1. Usage

```bash
rhei gate show   [TICKET_OR_PLAN] [--task <ID>] [--state-machine <PATH>] [--json]
rhei gate answer [TICKET_OR_PLAN] [--task <ID>] [--state-machine <PATH>] \
                 --from <STATE> --answers <FILE|-> [--dry-run] [--json]
rhei gate list   [ROOT] [--state-machine <PATH>] [--json]
rhei gate schema [request|answer|show|result|list]
```

### 1.1. Ticket Targets

`show` and `answer` take the ticket the way `rhei transition` does
([§FS-rhei-transition-cmd.2.1](rhei-transition-cmd.spec.md#21-ticket-targets)): the positional is a ticket or a plan, `--task` names the
ticket and leaves the positional to the plan, and a rhei-local shorthand
resolves only when one rhei holds that ticket. Output always names the
project-qualified id. `--state-machine` overrides the automatic `states.yaml`
lookup, as it does on every command.

## 2. `rhei gate show`

Print the ticket's current brief ([§FS-rhei-gate-brief.4](rhei-gate-brief.spec.md#4-the-brief)). It reads and writes
nothing else: no lock, no runtime file, no callback. A ticket whose state is not
`gating: true` is refused, exit 1, naming the state it is in.

A request that is missing or invalid is not an error here: the brief degrades
([§FS-rhei-gate-brief.4.2](rhei-gate-brief.spec.md#42-degraded-brief)) and says why in `problems`, and the command exits 0.

### 2.1. Output (`--json`)

One object, on stdout:

```json
{
  "version": 1,
  "ticket": "agora.1",
  "ticket_title": "Discuss: gate routing in rhei or ephor",
  "title": "Rule on 3 disputed entries",
  "state": "human-judgment",
  "visit": 1,
  "since": "2026-11-22T10:14:03Z",
  "plan": "/abs/path/to/agora/index.rhei.md",
  "root": "/abs/path/to/agora",
  "source": "request",
  "summary": "The discussion finished and every participant endorsed …",
  "display": { "path": "runtime/agora/verdict.md", "absolute": "/abs/path/to/agora/runtime/agora/verdict.md",
               "readable": true, "problem": null },
  "attachments": [ { "path": "runtime/agora/charter.md", "absolute": "/abs/…", "readable": true, "problem": null } ],
  "questions": [ { "id": "approve", "kind": "boolean", "prompt": "Accept the verdict as the record?", "required": true } ],
  "destinations": [ { "state": "completed", "description": "The ruling is recorded", "condition": null, "final": true } ],
  "routes": [ { "to": "completed", "when": { "approve": true } } ],
  "digest": "sha256:<64 hex>",
  "answer_path": "runtime/gates/agora.1/human-judgment.1.answer.json",
  "problems": []
}
```

| Field | Meaning |
|-------|---------|
| `ticket`, `ticket_title` | The project-qualified id and the ticket's title |
| `title`, `summary` | [§FS-rhei-gate-brief.4.3](rhei-gate-brief.spec.md#43-title); `summary` is `null` when there is none |
| `state` | The gate's canonical state name, without a visit suffix |
| `visit`, `since` | [§FS-rhei-gate-brief.4.4](rhei-gate-brief.spec.md#44-visit-and-since) |
| `plan` | Absolute path of the plan document that holds the ticket's state |
| `root` | Absolute execution root; every relative path in the brief is under it |
| `source` | `request`, `questions` or `synthesized` ([§FS-rhei-gate-brief.4](rhei-gate-brief.spec.md#4-the-brief)) |
| `display` | `null` when the brief has none; otherwise the file and whether it is readable, with a `problem` when it is not |
| `attachments` | The request's attachments in the same shape; `[]` when there are none |
| `questions` | The effective questions in order, synthesized ones included, each with `required` and `options` filled in |
| `destinations` | One entry per distinct target of the state's declared outgoing edges, in declaration order, describing the edge that governs that move ([§FS-rhei-transition-cmd.3](rhei-transition-cmd.spec.md#3-behavior)): its target, the target state's description (`null` when it has none), the edge's `condition` (or `exit code <n>`, or `null`), and whether the target is `final: true` |
| `routes` | The brief's routes; `[]` when the destination is asked as the `destination` question |
| `digest` | [§FS-rhei-gate-brief.4.5](rhei-gate-brief.spec.md#45-digest) |
| `answer_path` | Where an answer at this visit will be recorded ([§FS-rhei-gate-brief.7.1](rhei-gate-brief.spec.md#71-the-answer-file)) |
| `problems` | Every reason the brief degraded, one string each; `[]` when it did not |

A question's `options` and `help` are omitted where its kind or its author gave
none; every other field is always present.

### 2.2. Output (text)

Without `--json`, the brief is printed for a person: the ticket and title, the
state and visit, the summary, the display file and attachments with any that
cannot be read marked, each question with its options, the destinations, and
the problems. It ends with the command that answers it:

```text
rhei gate answer agora.1 --from human-judgment --answers <file|->
```

## 3. `rhei gate answer`

Answer the brief and move the ticket. The answer document
([§FS-rhei-gate-brief.6](rhei-gate-brief.spec.md#6-the-answer)) is read from `--answers`, a path or `-` for stdin. The
command:

1. Resolves the ticket and loads the machine. A `--from` that is not a
   `gating: true` state is `not_a_gate`.
2. Checks that the ticket is in `--from`, or `conflict`.
3. Reads the current brief, the degraded one included.
4. Validates the document and every answer against the brief, `invalid` and
   `stale` ([§FS-rhei-gate-brief.6.1](rhei-gate-brief.spec.md#61-values)), reporting every error found.
5. Picks the destination: the routes, or the `destination` answer
   ([§FS-rhei-gate-brief.5](rhei-gate-brief.spec.md#5-routes)), or `unrouted`.
6. Records the answer file and takes the edge through the ordinary transition
   path, under the same plan lock as the state write
   ([§FS-rhei-gate-brief.7](rhei-gate-brief.spec.md#7-recording)), with the rendered answers as its result message. A
   refusal there is `refused`, and the answer file is removed. The one
   exception is the compare-and-swap, checked again under the lock: a ticket
   that another caller moved between steps 2 and 6 is `conflict`, so of two
   answers to one visit exactly one moves the ticket. For the same reason a
   `stale` digest is reported as `conflict` when the ticket has left `--from`
   by the time it is found: the brief was read after the other answer moved
   the ticket.

Steps 1 to 5 report every error they find; once one of them refuses, the
later ones are not run, except that step 4's `invalid` and `stale` errors are
reported together. Exit 0 when the ticket moved, 1 on any refusal.

An answer is a manual release from a gate, so a move into a state with a
`visits` budget while a run is live is followed by the same advisory on stderr
as `rhei transition` gives ([§FS-rhei-transition-cmd.3.4](rhei-transition-cmd.spec.md#34-live-gate-release-into-a-visits-budget)); the JSON on stdout is
unchanged.

### 3.1. `--dry-run`

`--dry-run` runs every check a real answer runs — compare-and-swap on `--from`,
the digest, the answers, the routes, the edge's existence and condition, and the
source outputs, target inputs and terminal result that can be known without
running callbacks — and then writes nothing and runs no callback. Its exit code
and output are those the real answer would give, with `"dry_run": true`.

It takes the plan lock to read the ticket, as a real answer does, and stops
where the real answer would write its answer file: no answer file, state,
ledger, result or budget charge is written, and no `on_leave` redirect is
known, so `to` is the destination. The plan's lock file is the one file it may
create. A refusal that only a callback or the budget would give is therefore
not predicted.

### 3.2. Output (`--json`)

```json
{
  "version": 1,
  "ok": true,
  "dry_run": false,
  "ticket": "agora.1",
  "from": "human-judgment",
  "to": "completed",
  "route": { "index": 0, "when": { "approve": true } },
  "answer_path": "runtime/gates/agora.1/human-judgment.1.answer.json",
  "result": "Accept the verdict as the record?: yes\nAnswer: runtime/gates/agora.1/human-judgment.1.answer.json",
  "errors": []
}
```

`route` is `null` when the destination came from the `destination` question.
`to` is where the ticket went, after any `on_leave` redirect. `result` is the
rendered answers ([§FS-rhei-gate-brief.7.2](rhei-gate-brief.spec.md#72-the-result-message)). On refusal, stdout still
carries the object, with `"ok": false`; `to`, `route`, `answer_path` and
`result` are `null` where they are unknown, and `errors` lists each as
`{ "kind": <kind>, "question": <id> | null, "message": <text> }`, with the kinds
of [§FS-rhei-gate-brief.6.2](rhei-gate-brief.spec.md#62-refusals). Without `--json` the same errors are printed to stderr, one
per line.

## 4. `rhei gate list`

List every ticket under `ROOT` that sits in a gating state. `ROOT` is a plan
file, a workspace or a Panta project, and is the nearest enclosing one when
omitted, as for other commands. It is read-only, like `show`.

```json
{
  "version": 1,
  "root": "/abs/root",
  "gates": [
    { "ticket": "agora.1", "ticket_title": "…", "title": "Rule on 3 disputed entries",
      "state": "human-judgment", "plan": "/abs/…/index.rhei.md", "root": "/abs/execution/root",
      "visit": 1, "since": "2026-11-22T10:14:03Z", "source": "request",
      "required": 3, "holds": 6, "problems": [] }
  ]
}
```

Each entry's fields mean what they mean in `show` ([§FS-rhei-gate-cmd.2.1](rhei-gate-cmd.spec.md#21-output---json)), and
add:

- `required`: how many questions of the brief are required.
- `holds`: how many non-terminal tickets cannot become ready until this one
  leaves the gate — its open descendants, and the tickets whose `**Prior:**`
  chain reaches it.

Entries are ordered by `since`, oldest first, then `null`; ties keep plan order.
A root with no gate prints `"gates": []` and exits 0. Without `--json`, one line
per gate names the ticket, the state, the title, `required` and `holds`, or a
line saying no ticket waits at a gate. A rhei that would not load is a warning
on stderr, as on `rhei list` ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

## 5. `rhei gate schema`

Print the JSON Schema (draft 2020-12) of one gate document
([§FS-rhei-gate-brief.9](rhei-gate-brief.spec.md#9-schemas)): `request`, `answer`, `show` (the brief), `result` (the
answer's output) or `list`. With no name it prints
`{ "version": 1, "schemas": { "request": …, "answer": …, "show": …, "result": …, "list": … } }`.
An unknown name exits non-zero and lists the five.

Exit 0 is how a caller learns that this rhei supports gate briefs: a rhei
without them has no `gate` subcommand and exits non-zero.

## 6. Errors

A ticket id no rhei holds, or a shorthand more than one holds, is the shared
resolver's diagnostic ([§FS-rhei-errors.1.3](rhei-errors.spec.md#13-near-misses)). A machine that fails to load is
refused before anything is read, with the validation errors
([§FS-rhei-gate-brief.1.2](rhei-gate-brief.spec.md#12-validation)). Diagnostics go to stderr and documents to stdout
([§FS-rhei-usage.2.2](rhei-usage.spec.md#22-command-surface)).
