# FS-rhei-states-cmd: `rhei states`

Print the resolved state machine so humans and scripts can inspect available
states, transitions, profiles, node policy, artifacts, agents, programs, and
snapshot configuration before executing a plan. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## 1. Usage

```bash
rhei states
rhei states <RHEI_PLAN>
rhei states --rhei billing
rhei states --json
rhei --state-machine <PATH> states
```

`rhei states` reports the machine the target plan actually runs under, resolved
exactly as every other command resolves it ([§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)). An
omitted `<RHEI_PLAN>` resolves to the nearest enclosing project, workspace, or
lone plan, matching `rhei list` and `rhei run`. A command that reported the
built-in default while the project ran a declared machine would misdescribe
every state name the author is about to type.

## 2. Options

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--json` | No | false | Emit the state machine as JSON instead of human-readable text |
| `--rhei <ID>` | No | whole project | Narrow to named rheis (repeatable), as on `list`, `next`, `run`, and `reset` |
| `--state-machine <PATH>` | No | resolved from the plan | Global option selecting an explicit states YAML file |

`--rhei` narrows the report to the machines governing the named rheis, on the
project-wide-by-default rule every command follows ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). It is
the flag this command needs most: a project holding several instantiated
templates holds several machines, and "what states does *billing* have?" is
otherwise answered by reading past every other rhei's. An id naming no rhei in
the project is an error listing the available ids. Under narrowing every block
names its rheis, including one running the project default — narrowed, "the
default" is no longer a claim about the rest of the project.

## 3. Behavior

1. Load the explicit states YAML file when `--state-machine` is supplied.
   [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
2. Otherwise resolve the target plan and load the machine its `**States:**`
   declaration selects, including a declaration inherited from
   `index.panta.md`. [§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)
3. Fall back to the built-in default state machine when no plan or project
   resolves — there is then nothing to declare a machine.
4. Render a complete inspection view of the machine. For a project whose rheis
   declare their own machines ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)), render the project default
   first, then each additional distinct machine, each introduced by its own
   `Source:` line naming the rheis that run under it — one project, several
   processes, all inspectable from one command.
5. Print the result to stdout.

Two rheis share a rendered block only when their machines are **identical**,
field for field — not merely when they share a `name` and `version`. Machine
identity is content, everywhere the distinct set is walked: this command's
blocks, its JSON array, the completion candidates, the flow legend, and the
per-machine settings-reference validation that `validate` and `run` perform.

A template bakes its instantiation inputs into the `states.yaml` it writes, so
instantiating one template twice — the ordinary way to review two specs, audit
two subjects, or run two release checklists — produces two machines that differ
in their state instructions while both keep the template's declared name and
version. Grouping by name collapsed them and attributed one arbitrary member's
file to all of them: `rhei states` then printed one rhei's baked-in prompts
under a `Source:` line naming another rhei's file, and confidently answered
"what will my agents be told to do?" wrongly for every rhei but one. The
validation walk skipped the collapsed machines' agent, skill, and MCP
references entirely.

Discovery failures are non-fatal only when the target was inferred: an
auto-discovered plan that fails to load reports a warning on stderr and prints
the built-in default, so the command stays usable while the author is repairing
that very plan. An explicitly named plan propagates its load error.

The command is read-only. It does not validate task state, run callbacks, spawn
agents, spawn programs, or write runtime files.

### 3.1. Source Line

Text output opens with a `Source:` line naming the resolved states file, or
`the built-in default state machine` when no file backs it. The resolution
rules have several outcomes and the rendered machine alone does not distinguish
them.

## 4. Text Output

Text output includes:

- State machine name and version.
- Model list when present.
- Prompt-template list when present, including which prompt fields each
  template defines.
- Profile initial states and allowed state sets.
- Node policy when present.
- Each state with description and flags such as `final`, `gating`, and
  `concurrent`.
- Per-state execution details such as visits, the supervision trigger
  ([§FS-rhei-supervision.1.1](rhei-supervision.spec.md#11-the-execute_on-field)), polling, targets, models, agent, agent mode,
  authored effort (`Effort: <value>`), timeouts, program presence, MCP servers,
  skills, snapshots, inputs, outputs,
  personality, and instructions. A supervising state's `execute_on:` is spelled
  as *when the supervisor wakes*, on an `Executes on:` line: `every finished
  child`, `every child transition`, `every finished descendant`, or `every
  descendant transition — one invocation per hop`. The reader's question is
  which moves under the task bring it back, and the bare value answers that
  only to someone who already knows the grammar. `--json` keeps the value
  itself, `"execute_on": "<scope>-<event>"`.
- A polling state's `Poll:` line carries its cadence and, when the state
  declares one, the person it waits on: `Poll: interval=10m,
  max_attempts=60, waiting_on=author`. The label is appended only when
  `poll.waiting_on` is authored ([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)), so a machine-backoff
  poll prints exactly what it printed before.
- Per-state prompt-template reference when present.
- Declared transitions and annotations for callbacks, conditions, and timeouts.

## 5. JSON Output

`--json` emits a pretty JSON object with stable top-level fields:

```json
{
  "name": "default",
  "models": [],
  "prompt_templates": {},
  "profiles": null,
  "node_policy": null,
  "version": 1,
  "states": [],
  "transitions": []
}
```

Each state's `poll` object mirrors the authored block: `interval`,
`max_attempts`, and `waiting_on` only when the state declares it
([§FS-rhei-states.2.5](rhei-states.spec.md#25-waiting-on-a-person)). Presence is the meaning of that field, so it is
*omitted* rather than emitted as `null` when absent — which also makes the
JSON for every machine authored without it byte-for-byte what it was.

Each state that authors `effort` includes an `"effort": "<value>"` member.
When the field is omitted, JSON omits the member rather than emitting
`"effort": null`; text likewise adds no empty `Effort:` line. This preserves
the inspection shape of machines that do not opt in.

When JSON output is selected, command errors are rendered as a single JSON
object on stderr.

## Related Specifications

- [States Specification](rhei-states.spec.md) - state machine schema and defaults
- [Transitions Specification](rhei-transitions.spec.md) - transition schema and callbacks
- [Agents Specification](rhei-agents.spec.md) - agent/model execution fields
- [Programs Specification](rhei-programs.spec.md) - deterministic program states
- [Snapshots Specification](rhei-snapshots.spec.md) - snapshot state fields
