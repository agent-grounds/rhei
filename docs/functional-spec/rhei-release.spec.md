# FS-rhei-release: `rhei release`

Drop a ticket's `**Assignee:**` so work that was claimed but never finished can
be picked up again. Release changes nothing else: not the ticket's state, not
its result artifacts, not the transition ledger. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## Motivation

`rhei next` claims a ticket by writing `**Assignee:**`, and refuses to hand out
work while claims are outstanding — that is what stops two workers taking the
same ticket. But a worker that crashes, is killed, or simply walks away leaves
the claim behind. Without a way to drop it, the queue wedges: `rhei next`
reports the ticket as in progress forever.

The only escapes were editing the markdown by hand, or `rhei reset` — which
rewrites *every* ticket in scope to the initial state and deletes the whole
`runtime/` tree, inside a directory `rhei init` gitignores by default. Recovering
one abandoned claim by destroying every result in the project is not a recovery
path. Release is the narrow, non-destructive counterpart to the claim `rhei next`
makes.

## 1. Usage

```bash
rhei release <TICKET_ID>
rhei release [RHEI_PLAN] --task <TASK_ID>
rhei release [RHEI_PLAN] --all
rhei release [RHEI_PLAN] --all --rhei billing --dry-run
```

The positional slot is a *ticket or plan*, on the shared rule every
single-ticket command follows ([§FS-rhei-usage.2](rhei-usage.spec.md#2-coordination-through-the-state-machine)): an argument naming an
existing path is the plan, an id-shaped argument naming no path is the ticket.
Omitted, the plan resolves to the nearest enclosing project, workspace, or lone
plan ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).

`rhei release auth.1` and `rhei complete auth.1` are the same gesture aimed at
different outcomes — hand the ticket back, or finish it — so they take their
target the same way. Accepting the id only through `--task` here made the
release path read as a typo of the complete path.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--task <ID>` | One of ticket/`--all` | | Ticket to release: project-qualified (`auth.1`) or rhei-local (`1`); alternative to the positional |
| `--all` | One of ticket/`--all` | | Release every claimed non-terminal ticket in scope |
| `--rhei <ID>` | No | whole project | Narrow the sweep to named rheis (repeatable) |
| `--dry-run` | No | false | Report what would be released without changing anything |

`--task` and `--all` are mutually exclusive, and one is required. Passing
neither would make a bare `rhei release` ambiguous between "nothing" and
"everything" — on a command that mutates claims, that guess must not be made.

## 3. Behavior

1. Load the plan and resolve the state machine exactly as `rhei list` does.
2. Resolve the target set:
   - `--task <id>` selects one ticket. A ticket that holds **no** claim is an
     error, not a silent success: releasing a ticket nobody claimed almost
     always means the wrong id was typed.
   - `--all` selects every claimed ticket in scope whose state is
     **non-terminal**. An assignee on a finished ticket records who completed
     it rather than blocking anyone, so a sweep never erases it.
3. Print one line per target naming the ticket and the claim being dropped.
4. With `--dry-run`, stop here and change nothing.
5. Remove the `**Assignee:**` line from each target's task file, atomically,
   leaving every other line untouched.

Release never transitions a ticket, never writes a result, never appends to
`runtime/state-transitions.log`, and never deletes runtime artifacts.

### 3.1. Releasing from a non-initial state

Automatic `rhei next` selection claims tickets from the state machine's initial
state ([§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next)). Under a
machine whose claim also advances the state, a released ticket is unclaimed but
not automatically selectable — it sits in the state its abandoned run left it
in. A later explicit `rhei next --task <id>` may re-claim that non-initial
ticket and, when it is an eligible passive state, may advance its one applicable
non-terminal edge under the explicit-claim rule.

Release reports that distinction rather than moving the ticket. Rolling the
state back automatically would discard a transition that genuinely happened:
its `on_leave`/`on_enter` callbacks ran, its artifacts may exist, and its ledger
entry is written. Whether that work is salvageable, explicitly re-claimed, or
should be moved elsewhere is the operator's call, and the state is the only
remaining evidence of it.

The note is **not** printed for two tickets it would be wrong about:

- a ticket in a **supervising** state ([§FS-rhei-supervision.3.4](rhei-supervision.spec.md#34-manual-workers)), which
  `rhei next` claims exactly where it stands — moving it back to the profile's
  initial state is the one thing that would make it unclaimable;
- a ticket whose machine declares no state by the suggested name, where the
  command the note prints could not run.

## 4. Relationship to Other Commands

- `rhei next` writes the claim this command drops.
- `rhei complete` also removes the assignee, but as part of finishing the
  ticket — it transitions to a terminal state and writes a result. Release is
  for work that did *not* finish.
- `rhei reset` returns tickets to the initial state and deletes runtime output
  across its whole scope. Release touches one field on the tickets it names.

## Related Specifications

- [Next Command](rhei-next.spec.md) — claim semantics and the assignee marker
- [Complete Command](rhei-complete.spec.md) — the finishing path
- [Reset Command](rhei-reset.spec.md) — the destructive scope-wide reset
- [Panta](rhei-panta.spec.md) — target resolution and `--rhei` narrowing
