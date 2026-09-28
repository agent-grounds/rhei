# FS-rhei-render: `rhei render`

Render a Rhei plan or Directory Workspace into a selected read-only output
format. Rendering is for inspection, export, and scripting; it does not validate
state-machine reachability beyond the parse/load step and does not modify
runtime state. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

## 1. Usage

```bash
rhei render <RHEI_PLAN_OR_WORKSPACE> --format json
rhei render <RHEI_PLAN_OR_WORKSPACE> --format json --pretty
rhei render <RHEI_PLAN_OR_WORKSPACE> --format github --no-metadata --no-content
rhei render <RHEI_PLAN_OR_WORKSPACE> --format progress --no-color
```

`<RHEI_PLAN_OR_WORKSPACE>` may be a single `.rhei.md` file, a Directory
Workspace root, or a Panta project directory; omitted, the target is resolved by
walking up from the current directory ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)). A member rhei renders
its project narrowed to that rhei.

## 2. Options

| Flag | Required | Applies to | Description |
|------|----------|------------|-------------|
| `--format <FORMAT>` | Yes | all | Output format: `json`, `github`, or `progress` |
| `--pretty` | No | `json` | Pretty-print JSON instead of compact JSON |
| `--no-color` | No | `progress` | Disable ANSI color in progress output |
| `--no-metadata` | No | `github` | Omit metadata in GitHub Markdown output |
| `--no-content` | No | `github` | Omit subtask content in GitHub Markdown output |

## 3. Formats

### 3.1. JSON

`--format json` emits the parsed plan AST as JSON. Compact JSON is the default;
`--pretty` emits indented JSON for human inspection.
Each task exposes its authored `excludes` as an ordered array of typed entries;
an absent field renders as an empty array, preserving the unchanged-plan AST
shape convention used by `provides` and `consumes`.

The top-level `states` field is the machine of the document as authored — for a
project, the manifest default. A merged project runs **one machine per rhei**
([§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest)), so that single field cannot tell a consumer what
a task's state name means: a project holding two instantiated templates emits
tasks whose states come from three different machines under one `"states"`.
Rendering a project therefore adds a `rheis` array, in presentation order, that
attributes each rhei to the machine it actually runs:

```json
"states": "rhei",
"rheis": [
  { "id": "auth",    "states": "rhei",        "states_declared": false },
  { "id": "billing", "states": "spec-review", "states_declared": true  }
]
```

`states` on an entry is the *effective* machine — the rhei's own declaration,
or the project default it inherited — and `states_declared` distinguishes the
two. A consumer resolves any task by taking the first segment of its qualified
id and looking it up here. The key is absent, not empty, for a plan that is not
a merged project: a single-file plan or a lone Directory Workspace has one
machine, and the existing `states` field already names it.

When JSON format is selected, command errors are rendered as a single JSON
object on stderr so machine consumers do not need to parse two diagnostic
shapes.

The top-level `frontmatter` field carries the plan's **whole parsed frontmatter
document** — `metadata` with its `tasks` map, and whatever else the document
declared beside it — with the `metadata.tasks` keys re-keyed from the rhei-local
ids the file authored to the project-qualified ids every other surface names
([§FS-rhei-transitions.2.2](rhei-transitions.spec.md#22-metadata-storage-example),
[§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)).
The key is `null` for a plan that declared no frontmatter.

It is deliberately **unfiltered**, which is where it parts from `rhei list
--json`. `list` answers a query about tickets and publishes the author's layer
alone, so the keys rhei writes are held back from it
([§FS-rhei-transitions.2.5](rhei-transitions.spec.md#25-keys-rhei-writes),
[§FS-rhei-list.4.2](rhei-list.spec.md#42-json---json)). `render` exports the
document, and a document with its counters removed is not the document: the whole
point of this format is that what comes out could be read back. So a task's
`stateVisits`, its `supervision` block and every other registered key appear
here, and a consumer reading them is reading rhei's runtime bookkeeping, which is
not a stable contract for anything but this export.

#### 3.1.1. YAML values in JSON

Frontmatter is YAML and this output is JSON, so one conversion is defined here
and used by every JSON surface that publishes frontmatter — this field and the
`metadata` field of `rhei list --json`
([§FS-rhei-list.4.2](rhei-list.spec.md#42-json---json)) — so that the two can
never disagree about what a stored value looks like.

- A **mapping key** becomes its YAML text as a JSON string: `12:` is emitted as
  `"12"`, `true:` as `"true"`. JSON has only string keys, and the author's
  spelling is the one thing every reader can agree on.
- A **tagged value** becomes a one-key object naming the tag: `!custom hello`
  is emitted as `{"!custom": "hello"}`.
- Everything else converts as its JSON counterpart: mappings to objects,
  sequences to arrays, strings, finite numbers, booleans and null to themselves.

Two YAML shapes have no JSON image at all, and each is an **error** rather than a
silent substitution:

- a key that is not a scalar — a sequence or a mapping used as a key, which JSON
  cannot name;
- a float that is not finite — `.inf`, `-.inf`, `.nan`, which JSON has no number
  for.

The error names the plan, the project-qualified task id whose metadata holds the
value, and the key: a scalar key is named in quotes, and a non-scalar key is
reported by the shape that was used as one, because it has no name to give. Every
such value in the document is reported in one run
([§FS-rhei-errors.1.1](rhei-errors.spec.md#11-message)), the command exits
non-zero, and no document is emitted. Both cases used to pass silently — a
non-scalar key dropped the entire `frontmatter` value to `null`, and a non-finite
float became `null` — which is the loss this error replaces.

Making either shape a `rhei validate` error instead was considered and
**deliberately deferred**: it would refuse plans that validate today, which is a
compatibility question of its own and larger than the surface it would protect.

Each rendered task has an optional string field `inherits`. When authored, its
normalized value is either `"none"` or `"<name> from <axis>"`, matching
§FS-rhei-plan-language.3.14. The key is absent, not `null` or an empty string,
when `**Inherits:**` was omitted. This field reports authored task metadata;
it does not expand or duplicate an inherited state-machine rule.

### 3.2. GitHub Markdown

`--format github` emits Markdown suitable for GitHub issue-style review. By
default it includes plan metadata and subtask content. `--no-metadata` and
`--no-content` independently remove those sections.
`**Excludes:**` is metadata: the default output preserves the line in grammar
order, while `--no-metadata` omits it with the other task metadata. Progress
output remains a state/dependency summary and does not add exclusions.

### 3.3. Progress

`--format progress` emits a human-readable progress report, led by a completion
summary: `4/9 tickets done (44%)`, counting every ticket at every depth against
the resolved state machine's **final** states. The summary is omitted — never
guessed — when no state machine resolves, because "done" is a property of the
machine and a custom one need not have a state called `completed`.

Color is enabled only when stdout is a terminal and `NO_COLOR` is unset;
`--no-color` disables color regardless of terminal detection.

### 3.4. Rendering a merged project

A Panta project merges every rhei's tickets into one flat, project-qualified
task list ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)). The text formats — `github` and `progress` — must
put each ticket back under the rhei that owns it: a run of rhei headings
followed by every rhei's tickets in one undifferentiated list is not a document
a reader can use, and a rhei with no content section of its own leaves a heading
with nothing beneath it.

Each rhei renders as one block: its title as the heading, its own content
sections beneath that (without the `Rhei <id> / ` merge prefix), then its
tickets. The heading carries the rhei id when it differs from the title
(compared case-insensitively, so `Billing`/`billing` stays bare) —
`Q3 Launch (design)` — because the title and the id diverge freely (the id is
the file stem, the title is the `# Rhei:` header) and tickets are addressed by
id everywhere else: a reader of the progress report must be able to connect
`design.2` in a `rhei list` or error message back to the block that explains
it. A rhei that holds no tickets says so rather than rendering an empty
heading. Manifest-level content sections stay above the blocks, where they
describe the project rather than any one rhei.

A plan that is not a merged project — a single-file plan, a Directory Workspace
loaded on its own — keeps its authored shape: content sections, then one
`## Tasks` chapter.

`--format json` keeps its flat shape: it emits the AST, and the merged section
titles are part of that AST. Its only project-specific addition is the `rheis`
attribution array (§3.1).

## 4. Behavior

1. Load the plan from the file, workspace, or project.
2. Parse it into the Rhei AST defined by the plan language. [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
3. Narrow to the rhei the target named, when it named one ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)).
4. Render the parsed plan in the selected format.
5. Print the rendered document to stdout.

`rhei render` does not acquire task locks, run callbacks, spawn agents, spawn
programs, write runtime files, or rewrite plan files.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) - source syntax and AST shape
- [List Command](rhei-list.spec.md) - filtered task inspection
- [Validate Command](rhei-validate.spec.md) - full semantic validation before execution
