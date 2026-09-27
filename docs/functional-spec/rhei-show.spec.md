# FS-rhei-show: `rhei show`

Print one task's heading and its body as stored, by id. `rhei show` is the read
that answers "what does this ticket say" — for a duplicate check, a triage
decision, a supervisor's brief, or a script. It resolves nothing about state and
modifies nothing: no lock, no runtime write, and no state-machine validation, so
a plan broken somewhere else is still a plan whose tickets can be read.
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

Every other read that carries a body carries a document around it. `rhei render`
emits the whole plan ([§FS-rhei-render](rhei-render.spec.md#fs-rhei-render-rhei-render)), `rhei list` emits no body at all
([§FS-rhei-list.4.1](rhei-list.spec.md#41-text-default)), and `rhei next --peek` prints the claim screen — the
state's instructions and four memory sections — around the one it does print
([§FS-rhei-next.4.1](rhei-next.spec.md#41-output-peek-mode)), and refuses a ticket outside the ready set altogether
([§FS-rhei-next.4](rhei-next.spec.md#4-peek-mode---peek)). A finished ticket is the commonest thing anyone reads,
because finished work is what later work is checked against, and it was the one
read with no verb.

## 1. Usage

```bash
rhei show <TICKET_OR_PLAN> [--task <ID>] [--json]
```

The positional is a ticket or a plan, resolved in [§FS-rhei-complete.2.1](rhei-complete.spec.md#21-ticket-targets)'s order:
with `--task` the positional is the plan path, an existing path is the plan, and
an id-shaped argument naming no path is the ticket with the plan inferred from
the working directory ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). The ticket is mandatory — a positional
that names only a plan is an error naming the ticket form ([§FS-rhei-show.5](rhei-show.spec.md#5-errors)).

| Flag          | Required | Description                                                     |
|---------------|----------|-----------------------------------------------------------------|
| `--task <ID>` | No       | Name the ticket, leaving the positional free to name the plan.   |
| `--json`      | No       | Emit one JSON object instead of the text form ([§FS-rhei-show.4](rhei-show.spec.md#4-output---json)).              |

Ticket-id arguments complete from the plan like every other
([§FS-rhei-completions](rhei-completions.spec.md#fs-rhei-completions-rhei-completion-ux-specification)).

## 2. Ticket Target

The target is the project-qualified ticket id (`auth.1`) or a rhei-local
shorthand (`1`), resolved exactly as [§FS-rhei-next.2.1](rhei-next.spec.md#21-ticket-targets) resolves one: a shorthand
resolves only when exactly one in-scope rhei holds that ticket, and the heading
prints the qualified id however the target was written. There is no `--rhei`
flag — the explicit ticket target is the scope, as on `rhei complete`
([§FS-rhei-complete.2.1](rhei-complete.spec.md#21-ticket-targets)).

Any ticket resolves, in any state. `rhei show` asks nothing about readiness,
assignment, or terminality, because none of those is a property of the prose.
That is the difference from `rhei next --peek`, whose ready-set gate is a promise
about *claiming* ([§FS-rhei-next.4](rhei-next.spec.md#4-peek-mode---peek)) and correctly refuses a ticket nobody may
claim.

## 3. Output (text)

Exactly one heading line, one blank line, then the body:

```text
## Task probe.7: the tool mishandles a case that takes prose to describe

- Context: probe 7 — tool 0.5.1-dev at deadbeef, run in a flat panta project
  whose plan holds ten tickets, each with a lifecycle record under it.
- Expected: a verb that prints exactly what was asked for, by id.
```

And nothing else. No state line, no agent or model line, no children, no
instructions, no metadata fields, no trailing summary. The heading is
`## Task <qualified-id>: <title>` whatever the node's kind is, the spelling
`rhei next --peek` and the run prompt already use ([§FS-rhei-next.4.1](rhei-next.spec.md#41-output-peek-mode)), so one
ticket is not named two ways across two screens.

The body is the task's content as stored: the authored bytes, in their authored
order and indentation, with nothing interpreted and nothing removed. Rhei knows
no boundary inside a body, so text a hook appended under the prose — a lifecycle
record, a result block — is part of the body and is printed with it. A caller
that wants less cuts it itself.

Surrounding blank lines are not content. The printed body is the stored content
with leading and trailing whitespace trimmed, followed by one newline — the
convention `rhei next --peek` already prints a body under ([§FS-rhei-next.4.1](rhei-next.spec.md#41-output-peek-mode)),
so the same ticket reads the same way on both screens. A task whose body is
empty or whitespace prints the heading alone, with no blank line after it, and
exits 0: a ticket with nothing written under it is an answer, not a failure.

Text goes to stdout and diagnostics to stderr ([§FS-rhei-usage.2.2](rhei-usage.spec.md#22-command-surface)).

## 4. Output (`--json`)

One object, on stdout, with exactly three fields:

```json
{
  "id": "probe.7",
  "title": "the tool mishandles a case that takes prose to describe",
  "content": "- Context: probe 7 — tool 0.5.1-dev at deadbeef, run in a flat panta project\n  whose plan holds ten tickets, each with a lifecycle record under it."
}
```

`id` is the qualified id, `title` the task's title, and `content` the body —
byte-identical to what [§FS-rhei-show.3](rhei-show.spec.md#3-output-text) prints below the heading, minus that form's closing
newline. `content` is always present and is `""` for an empty body. Nothing
else: no `state`, `kind`, `assignee`, `prior`, `parent`, or `depth`. Those are
`rhei list --json`'s eight stable fields ([§FS-rhei-list.4.2](rhei-list.spec.md#42-json---json)), and two surfaces
that must agree about a ticket's state is one too many. Three fields is a set
that can grow; a fourth added now could not be taken back.

Nothing is written to stderr on success, so `rhei show <id> --json | jq -r
.content` is the body and nothing else.

## 5. Errors

A ticket id no rhei holds is [§FS-rhei-errors.1.3](rhei-errors.spec.md#13-near-misses)'s near miss — `closest ids: …`,
or a pointer to `rhei list` where nothing is close — under
`help: list the task ids in this plan with: rhei list <plan>`, which names the
command that reveals what is missing ([§FS-rhei-errors.1.2](rhei-errors.spec.md#12-help)). A shorthand more
than one in-scope rhei holds names the qualified candidates unchanged
([§FS-rhei-next.2.1](rhei-next.spec.md#21-ticket-targets)). Both are the resolver every ticket-taking command shares,
so a stale id reads the same here as it does on `rhei complete`.

An id-shaped positional is never reported as a missing path. The plan-only
positional that `list`, `render` and `validate` take answers an id-shaped
argument with "this argument takes a plan or project, not a ticket id", which is
the right sentence for those commands and the opposite of the truth for this
one: `rhei show probe.7` is the invocation this specification is about. `show`
therefore resolves its positional as a ticket target ([§FS-rhei-complete.2.1](rhei-complete.spec.md#21-ticket-targets)),
never as a plan-only one.

A positional that names an existing plan and no ticket is an error asking for
the ticket, in the form that works when pasted: `rhei show <ticket-id>`, or
`rhei show <plan> --task <ticket-id>`.

## Related Specifications

- [List Command](rhei-list.spec.md) — which tickets exist, and their fields
- [Render Command](rhei-render.spec.md) — the whole plan as a document
- [Next Command](rhei-next.spec.md) — the claim screen, and its ready-set gate
- [Complete Command](rhei-complete.spec.md) — the shared `<TICKET_OR_PLAN>` positional
- [CLI Errors and Guidance](rhei-errors.spec.md) — near misses and help lines
