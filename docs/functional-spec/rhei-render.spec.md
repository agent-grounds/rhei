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

Each rendered task has an optional string field `inherits`. When authored, its
normalized value is either `"none"` or `"<name> from <axis>"`, matching
§FS-rhei-plan-language.3.13. The key is absent, not `null` or an empty string,
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
