# FS-rhei-plan-language: Rhei Plan Language Specification

This document specifies the grammar and semantics of the Rhei Plan language — a structured subset of Markdown for hierarchical task management shared by humans and AI agents.

## Overview

The Rhei Plan language is a **context-sensitive subset of Markdown** designed for:

- AI agent memory
- AI agent coordination
- Human oversight of work progress

## Quick Start

### Write a plan

Create a `.rhei.md` file. At minimum, a plan needs a title, a `## Tasks` section, and at least one task with a `**State:**` field:

```markdown
# Rhei: Add user avatars

## Tasks

### Task 1: Add avatar column to users table
**State:** pending

Add a nullable `avatar_url` column to the `users` table and generate a migration.

### Task 2: Build upload endpoint
**State:** pending
**Prior:** Task 1

Create a POST /api/users/:id/avatar endpoint that accepts an image, stores it in S3, and writes the URL to the new column.

### Task 3: Display avatars in the UI
**State:** pending
**Prior:** Task 2

Render the avatar in the profile header and comment list. Fall back to initials when no avatar is set.
```

`**Prior:**` declares dependencies — Task 2 cannot be claimed until Task 1 is
in a successful terminal state as defined by the active state machine
(`final: true` and not normalized `cancelled`; in the built-in `rhei` machine,
`completed`). Tasks without `**Prior:**` are immediately dependency-ready.

Tasks may also be decomposed hierarchically when the plan's optional
`structure.maxLevels` setting allows more than one level:

```markdown
# Rhei: Stabilize release
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Release readiness
**State:** pending

#### Task 1.1: Verify release notes
**State:** pending

##### Task 1.1.1: Compare changelog and notes
**State:** pending

#### Bug 1.2: Fix null-cache panic
**State:** pending
```

### Validate the plan

```bash
rhei validate plan.rhei.md
```

The CLI checks syntax, state validity, dependency integrity, and that the dependency graph has no cycles.
When the active state machine declares artifact contracts, runtime commands also
enforce required state inputs and outputs as described in the states and
transitions specifications.

### Have an agent execute it

Tell an agent with the `rhei-plan-worker` skill to work the plan:

```bash
/rhei-plan-worker plan.rhei.md
```

The worker reads the plan, loads the state machine, and enters a loop: claim
the next eligible task with `rhei next`, work in that task's current state,
use `rhei transition` when the workflow requires an explicit state change (for
example `draft` to `pending`), finish with `rhei complete` when the task reaches
a terminal outcome, and repeat. This manual worker flow is distinct from
`rhei run` agent mode, where spawned agents do the work of the current state and
the `rhei run` orchestrator performs the transition after the subprocess exits.
The worker stops when no eligible tasks remain or a gating state such as
`human-review` requires a human decision.

The plan file is the single source of truth — multiple agents or humans can read it to see what is done, what is in progress, and what is blocked.

For programmatic execution with `rhei run`, see [Pattern 6: Programmatic State Transitions](rhei-usage.spec.md#38-pattern-6-programmatic-state-transitions).



## 1. Plan Formats

A rhei can be authored as either a **Single-File Plan** or a **Directory
Workspace**. Both are *rhei-level* formats: each describes one flow. A project
groups its rheis under the virtual **Panta** root ([§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)); the project
container format is defined in [Panta Project](#15-panta-project). A bare rhei
loaded on its own is treated as the single rhei of an implicit Panta
([§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture)).

### 1.1. Single-File Plan (1 Agent, or low concurrency)

The single-file format is a hierarchical structure:

| Component | Heading Level | Format | Required |
|-----------|---------------|--------|----------|
| Rhei Title | H1 (`#`) | `# Rhei: <title>` | Yes |
| States Declaration | — | `**States:** <state-machine-name>` | No (defaults to `rhei`) |
| Content Sections | H2 (`##`) | `## <section-name>` | No |
| Tasks Section | H2 (`##`) | `## Tasks` | Yes |
| Root Node | H3 (`###`) | `### <kind> <id>: <title>` | No |
| Child Node | H4-H6 (`####`-`######`) | `<heading> <kind> <id>: <title>` | No |

A `## Tasks` section holding **no** task nodes is a valid, empty rhei — the
section heading is required, its contents are not. This mirrors the Directory
Workspace rule below (§1.2) rather than contradicting it: the two formats
describe the same rhei, and a rhei that is empty in one of them cannot be
illegal in the other. It is the state a rhei passes through between being
created and receiving its first ticket, which is exactly what `rhei new`
creates ([§FS-rhei-new.2](rhei-new.spec.md#2-creating-a-rhei)). As in a workspace, `rhei validate` reports the
emptiness as a **warning** naming the rhei, so a rhei whose tickets were
accidentally deleted is not silently green.

When present, the `**States:**` field must be the first non-empty line after
the `# Rhei:` title. Its value is the `name` of the state machine defined in the
associated states configuration (see [States Specification](rhei-states.spec.md)).
State-machine resolution is defined in
[State Machine Resolution](#13-state-machine-resolution).

When frontmatter omits a `structure` block, the default structure is:

```yaml
structure:
  maxLevels: 2
  nodeKinds: [task]
```

This establishes the default hierarchical structure for the current language
revision. See [ADR 0002](../adr/0002-hierarchical-task-nodes.md) for the rationale
behind the nested task-node model and for migration rules from pre-revision
plans.

### 1.2. Directory Workspace (Agent Teams, High Concurrency)

To prevent Git merge conflicts when multiple agents or humans work in parallel across disparate branches, a Rhei plan can be structured as a directory. This functions similarly to distributed issue trackers.

A Directory Workspace consists of:

1. **`index.rhei.md`**: The root configuration. Contains the `Rhei Title`, `States Declaration`, and any `Content Sections`. It does **not** contain a `## Tasks` section.
2. **`tasks/` directory**: A folder containing workspace task `.md` files.
3. **Workspace Task Files**: Files within `tasks/` that contain one or more
   node definitions (starting directly with `### <kind> <id>:`). They do not
   require the `# Rhei:` header.

Task-file discovery is recursive and deterministic. Implementations must load
non-hidden files matching `tasks/**/*.md`, where neither the file name nor any
directory segment under `tasks/` starts with `.`. Paths are ordered by their
normalized workspace-relative path using `/` separators and case-sensitive
lexicographic comparison. A discovered task file must parse as
`workspace_task_file` and contain at least one root task node; empty Markdown
files and prose-only Markdown files under `tasks/` are invalid task files.

A workspace that discovers **no** task files is a valid, empty rhei — not an
error. It is the state a workspace passes through between being created and
receiving its first ticket, and the same state a living workspace starts from
before its coordinator appends work. Failing the load instead would let one
freshly created directory break `rhei list` and `rhei validate` for every
sibling rhei in the project, so an empty rhei is treated exactly as an empty
project is ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)): read commands report zero tickets and succeed.
Because a mistyped `tasks/` directory is otherwise indistinguishable from a
deliberately empty one, `rhei validate` reports the emptiness as a **warning**
naming the rhei and where its task files must live ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior)).

In a Directory Workspace, all tasks are parsed and merged into a single global
task graph at runtime. Dependency validation (`**Prior:**`) resolves globally
across all discovered task files under `tasks/`. The merged plan order is the
discovered file order, then each file's authored preorder task order. Commands
that scan "in plan order" use this merged order for scheduling and rendering.

To prevent creation collisions in highly distributed swarms, letter-prefixed
`IDENTIFIER` values rather than sequential `NUMBER` task ids are strongly
recommended for Directory Workspaces. Because the grammar requires an
`IDENTIFIER` to start with a letter, distributed ids should use forms such as
`task-550e8400-e29b-41d4-a716-446655440000` rather than a bare UUID or hash.

### 1.3. State Machine Resolution

State-machine resolution is normative for all commands:

1. `--state-machine <path>` loads the specified YAML file and overrides
   automatic lookup. If the target plan or rhei declares `**States:**`, or
   inherits a Panta default declaration, the loaded file's `name` must match that
   effective value; if the effective field is omitted, the loaded file's `name`
   becomes the active state-machine name for this invocation.
2. For a rhei inside a Panta Project, both the presence and the value of its
   `**States:**` declaration are resolved per rhei. After rule 1, a declaration
   in the rhei itself gives its execution root first refusal: a matching
   `<rhei>/states.yaml` wins even when the declared name equals the resolved
   project default. An absent local file, or a valid local file with another
   name, falls back to that already-resolved default in the same-name case and
   otherwise continues through the project-level name-match rules. An invalid
   local candidate is a load error, not a fallback. If the rhei omits
   `**States:**`, it inherits the resolved project machine wholesale and its
   own root is not consulted. Thus restating a custom default is equivalent to
   omission only for the effective machine *name*, not for definition-file
   precedence ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)). Restating the project's effective
   built-in `rhei` default is the one exception where that equivalence is
   total: rule 4 governs that case, and the rhei's own root is not consulted
   there either.
3. When `**States:**` is omitted after Panta inheritance has been applied, the
   plan or rhei uses the built-in `rhei` state machine. Sibling, workspace, or
   project `states.yaml` files are ignored in this case.
4. When `**States:** rhei` is declared and no override is supplied, a matching
   `states.yaml` named `rhei` may be used only from the invocation's local
   lookup location: beside a standalone single-file plan, at a standalone
   Directory Workspace root, or at the Panta project root for the project's
   effective default. A member whose own declaration differs from the
   project's effective default instead resolves the same way rule 5 describes
   for a non-`rhei` declaration — its own execution root first, then a unique
   `name`-match among the project's candidate roots. Otherwise — including
   when that candidate search finds nothing for a member whose Panta default
   names a different machine — the plan falls back to the built-in `rhei`
   state machine. In particular, when a Panta project's effective default is
   `rhei`, a member that inherits or restates that default does not let a
   matching file found only in a member rhei root replace the built-in fallback.
5. When a non-`rhei` `**States:** <name>` is declared and no override is
   supplied, the CLI resolves the file from a sibling `states.yaml` for a
   single-file plan, from `<workspace>/states.yaml` for a Directory Workspace,
   from the declaring rhei's execution root for any member rhei's own
   declaration, or from the already-resolved project machine when the
   declaration was inherited from `index.panta.md` — falling back, where rule
   2 requires project-level name lookup, to a unique `name`-match among the
   project's candidate roots ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)).
   This candidate-root fallback does not apply to the effective Panta project
   default `rhei` governed by rule 4. A resolved file's `name` must match
   `<name>`.
6. A declared non-`rhei` state machine without a matching auto-discovered file
   is a validation error; it never falls back to the built-in machine.

### 1.4. Directory Workspace Metadata

YAML frontmatter for a Directory Workspace belongs in `index.rhei.md`. Workspace
task files start directly with task definitions and must not introduce
independent frontmatter blocks, so the workspace has exactly one authoritative
`metadata.tasks.<id>` map.

Runtime-managed metadata that is defined in the transitions specification, such
as `metadata.tasks.<id>.stateVisits.<state-name>`, is therefore read from and
written to the frontmatter in `index.rhei.md`, keyed by the global task id.

Persistence ownership is normative:

- Markdown task fields remain the source of truth for `**State:**`,
  `**Prior:**`, `**Assignee:**`, and `> **Result:**`. In a
  Directory Workspace, those fields are persisted in the workspace task file
  that contains the task.
- After initial authoring, task state changes must be performed through Rhei
  commands (`rhei transition`, `rhei complete`, `rhei run`, or another command
  that explicitly owns state transitions). Operators and agents must not edit
  `**State:**` lines directly, because direct edits bypass transition
  validation, compare-and-swap conflict checks, callbacks, artifact checks,
  counted-visit metadata, and the result-file audit trail.
- YAML frontmatter under `metadata.tasks.<id>.*` stores auxiliary per-task
  metadata only, such as `stateVisits` counters and custom callback data. It
  must not become a second source of truth for state, dependencies, assignee,
  or result links.
- Runtimes may project the current markdown assignee into callback-facing APIs
  such as `task.metadata.assignee` for convenience, but that value remains a
  view over the markdown `**Assignee:**` line rather than a separately
  persisted frontmatter field.

This keeps task descriptions, `**Assignee:**` changes, and `> **Result:**`
blocks localized to task files, which preserves most of the concurrency benefit
of the workspace format. However, features that persist data through
frontmatter-backed task metadata still serialize through `index.rhei.md`, so
metadata-heavy workflows reintroduce a narrow shared-write hotspot until a
workspace-local metadata format is specified.

### 1.5. Panta Project

A **Panta Project** is the directory that groups a project's rheis under the
single virtual Panta root ([§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)). It mirrors the Directory Workspace
shape one level up: where a workspace is a directory of task files, a Panta is a
directory of rheis. The directory is conventionally named `panta/`, but any name
works — a directory is a Panta Project when it contains `index.panta.md`.

A Panta Project consists of:

1. **`index.panta.md`**: the project manifest — `# Panta: <title>`, an optional
   `States Declaration`, and any `Content Sections`. It contains no `## Tasks`
   section and no authored nodes.
2. **Rhei entries**: the rheis live directly in the project directory, beside the
   manifest. Each entry is a direct-child Single-File Plan (`*.rhei.md`) or a
   direct-child Directory Workspace subdirectory. Discovery scans only the
   project directory's immediate children — it does not descend into grouping
   folders — and is deterministic, using the same non-hidden, `/`-normalized,
   case-sensitive ordering rules as workspace task-file discovery
   ([§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)). The `runtime/` artifact tree is never a rhei
   entry.
3. **`basin/` directory** (optional): loose tickets captured without a domain
   rhei. This directory is loaded as a synthetic Directory Workspace rhei with id
   `basin`; its contents are authored as workspace task files. The synthetic
   basin has no authored `index.rhei.md`, inherits the Panta default state
   declaration, and stores per-ticket runtime artifacts relative to `basin/`.

Each rhei's id is derived from its location in the project directory: the
filename stem for a Single-File Plan (`auth.rhei.md` -> `auth`) or the directory
name for a Directory Workspace rhei (`billing/` -> `billing`). The id must be a
valid `IDENTIFIER` and must be unique across the project. `basin` is permanently
reserved for the synthetic basin rhei: a domain rhei with id `basin` is a
validation error whether or not the `basin/` directory exists.

At load time all rheis, including the synthetic `basin` rhei when present, merge
into one graph rooted at Panta. Ticket ids are namespaced by their rhei id and
`**Prior:**` resolves across the whole project; see
[Identifier Uniqueness](#35-identifier-uniqueness) and [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture).

## 2. Grammar (EBNF)

```ebnf
(* ============================================== *)
(* DOCUMENT STRUCTURE                             *)
(* ============================================== *)

rhei_document   = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section },
                  tasks_section ;

rhei_header     = "# Rhei: ", title, NEWLINE ;

states_field    = "**States:** ", state_machine_name, NEWLINE ;

state_machine_name = title ;

content_section = "## ", section_title, NEWLINE, { markdown_block } ;

(* ============================================== *)
(* DIRECTORY WORKSPACE STRUCTURE                  *)
(* ============================================== *)

workspace_index = rhei_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section } ;

workspace_task_file = [ { blank_line } ], task_level_1, { task_level_1 } ;


(* ============================================== *)
(* PANTA PROJECT STRUCTURE                        *)
(* ============================================== *)

(* A Panta Project is a directory layout, not a single token stream. The
   productions below describe its individual files; the project tree itself is
   defined structurally in section 1.5. `index.panta.md` parses as
   panta_manifest. Each rhei entry in the project directory is a Single-File Plan
   (rhei_document) or a Directory Workspace (a workspace_index plus
   workspace_task_file files). Entries under the optional `basin/` directory
   parse as workspace_task_file and are loaded as the synthetic `basin` rhei. *)

panta_manifest  = panta_header, { blank_line },
                  [ states_field, { blank_line } ],
                  [ frontmatter, { blank_line } ],
                  { content_section } ;

panta_header    = "# Panta: ", title, NEWLINE ;


(* ============================================== *)
(* YAML FRONTMATTER (optional metadata)           *)
(* ============================================== *)

(* YAML frontmatter stores custom task metadata such as retryCount,
   stateVisits, priority, callback data, and optional plan structure settings.
   Core task fields such as state, prior, assignee, and result links remain in
   markdown. It appears between two `---` fences and is parsed as YAML, not
   Markdown. See the Transitions Specification for the metadata access
   contract. *)
frontmatter     = "---", NEWLINE,
                  { yaml_line },
                  "---", NEWLINE ;

yaml_line       = ? any line that is not exactly "---" ?, NEWLINE ;


(* ============================================== *)
(* TASK TREE                                      *)
(* ============================================== *)

tasks_section   = "## Tasks", NEWLINE, { blank_line },
                  task_level_1, { task_level_1 } ;

task_level_1    = node_header_level_1, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_2 } ;

task_level_2    = node_header_level_2, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_3 } ;

task_level_3    = node_header_level_3, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ], { task_level_4 } ;

task_level_4    = node_header_level_4, NEWLINE, metadata, { task_markdown_block },
                  [ result_block ] ;

node_header_level_1 = "### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_2 = "#### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_3 = "##### ", node_kind_keyword, " ", task_id, ": ", title ;
node_header_level_4 = "###### ", node_kind_keyword, " ", task_id, ": ", title ;

node_kind_keyword = IDENTIFIER ;

task_id         = task_id_segment, { ".", task_id_segment } ;

task_id_segment = NUMBER | IDENTIFIER ;


(* ============================================== *)
(* METADATA FIELDS                                *)
(* ============================================== *)

(* State field is mandatory and must appear first.
   Prior, Assignee, and an optional execution override follow; the `complete`
   command strips the assignee on completion. A task may declare at most one
   execution override — either `**Model:**` or `**Target:**`, never both. See
   section 3.11. *)
(* The metadata block is closed: `**State:**`, `**Prior:**`, `**Provides:**`,
   `**Consumes:**`, `**Excludes:**`, `**Assignee:**`, `**Model:**`, and `**Target:**` are the
   only fields. A `**<name>:**` line
   with any other name, appearing in the block before a blank line has
   separated it from the heading, is a parse error naming the unknown field.
   Accepting it as content silently discards it, and the field authors most
   often mistype is `**Prior:**` — a dropped dependency leaves a plan that
   validates green and executes in the wrong order, with nothing to point at.
   Past a blank line the same line is ordinary bold text and is kept as task
   content. *)
metadata        = state_field, [ prior_field ], [ provides_field ],
                  [ consumes_field ], [ excludes_field ], [ assignee_field ],
                  [ execution_override ] ;

assignee_field  = "**Assignee:** ", title, NEWLINE ;

(* Per-task execution override. `**Model:**` swaps only the model dimension and
   keeps each state's agent/mode/provider; `**Target:**` replaces the full
   inline execution identity. The target selector grammar is the one defined in
   §FS-rhei-states (the four `<agent>[<mode>]:<provider>:<model>` forms).
   Semantic validation is deferred to section 3.11. *)
execution_override = model_field | target_field ;

model_field     = "**Model:** ", model_id, NEWLINE ;

target_field    = "**Target:** ", target_selector, NEWLINE ;

model_id        = { ANY_CHAR - NEWLINE }+ ;

target_selector = { ANY_CHAR - NEWLINE }+ ;

(* Result block links a terminal task to its runtime result/audit file.
   It is inserted by the `complete` command after task content and before child
   tasks. The link text is the task id itself, and the target is always
   runtime/results/<task-id>.md. The grammar is form-agnostic: `complete`
   writes the project-qualified id, and the legacy rhei-local id still parses
   in plans completed before ticket ids gained their rhei prefix. Text and
   target must agree — see section 3.8. *)
result_block    = "> **Result:** ", "[", task_id, "](", result_path, ")", NEWLINE ;

result_path     = "runtime/results/", task_id, ".md" ;

state_field     = "**State:** ", state_value, NEWLINE ;

prior_field     = "**Prior:** ", task_ref_list, NEWLINE ;

task_ref_list   = task_ref, { ", ", task_ref } ;

(* The kind keyword is decoration: a reference resolves on its task_id alone,
   so the bare form is accepted too. It exists because every surface that
   *prints* a dependency prints the id by itself — `rhei list` renders
   `(prior: auth.2)` — and an author who pastes that back must not be met with
   a parse error. Writers emit the kind-prefixed form; readers accept both. *)
task_ref        = [ node_kind_keyword, " " ], task_id ;

(* Task exports: the producer names what it publishes, the consumer names the
   task and export it reads. Both are plan-level; neither appears in the state
   machine. See section 3.12. *)
provides_field  = "**Provides:** ", export_name_list, NEWLINE ;

consumes_field  = "**Consumes:** ", export_ref_list, NEWLINE ;

excludes_field  = "**Excludes:** ", exclusion_list, NEWLINE ;

export_name_list = export_name, { ", ", export_name } ;

export_ref_list = export_ref, { ", ", export_ref } ;

export_ref      = task_id, ":", export_name ;

exclusion_list  = exclusion, { ", ", exclusion } ;

exclusion       = "checkout=", exclusion_path
                | "artifact=", exclusion_path
                | export_ref ;

(* Authored paths always use `/`. A final `/` makes the target recursive;
   otherwise it names one exact file, including when it does not yet exist. *)
exclusion_path  = path_component, { "/", path_component }, [ "/" ] ;

path_component  = exclusion_char, { exclusion_char } ;

exclusion_char  = ? any Unicode character except NEWLINE, "/", "\\", and "," ? ;

(* An export name keys a path segment, so it excludes separators and spaces. *)
export_name     = ( LETTER | DIGIT ), { LETTER | DIGIT | "." | "_" | "-" } ;

(* State values have two rendered forms:
   - bare form for canonical names that match IDENTIFIER exactly
   - backticked form for any other canonical state name, including names
     with spaces or punctuation such as `human review`, `qa/review`, or
     `security.review`
   States with a declared `visits` budget may append `-<n>` (where n >= 2)
   to the rendered value to make later visits visible in markdown. A `-1`
   suffix is never rendered — the first visit is the unsuffixed base name.
   Exact-match resolution against loaded state names happens before any
   `-<n>` suffix is interpreted as a counted visit. Inside backticks, `\\`
   escapes a backslash and `\`` escapes a literal backtick. *)
state_value     = IDENTIFIER, [ "-", VISIT_NUMBER ]              (* pending, review-2 *)
                | "`", quoted_state_text, "`" ;                 (* `human review`, `qa/review`, `human review-2` *)

(* VISIT_NUMBER encodes a counted-visit suffix. It must be >= 2 so that the
   unsuffixed base name is the only valid rendering of visit 1. No leading
   zeros. *)
VISIT_NUMBER    = LEADING_2_9, { DIGIT }
                | "1", DIGIT, { DIGIT } ;

LEADING_2_9     = "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

quoted_state_text = quoted_state_char, { quoted_state_char } ;

quoted_state_char = ESCAPED_BACKSLASH
                  | ESCAPED_BACKTICK
                  | ? any Unicode character except NEWLINE, "`", and "\" ? ;


(* ============================================== *)
(* TERMINALS                                      *)
(* ============================================== *)

title           = { ANY_CHAR - NEWLINE }+ ;

(* Section titles must not equal "Tasks" — that prefix is reserved
   for the tasks_section. *)
section_title   = { ANY_CHAR - NEWLINE }+ - "Tasks" ;

(* A task_markdown_block is ordinary task body content. The `> **Result:** `
   prefix is reserved for the dedicated result_block production so the grammar
   can distinguish result metadata from freeform prose. *)
task_markdown_block = ( blank_line
                      | task_body_line, NEWLINE ) ;

(* A markdown_block is any non-structural content line for use in sections.
   Blank lines are markdown_blocks. *)
markdown_block  = ( blank_line
                  | non_structural_line, NEWLINE ) ;

task_body_line  = ? any line that does not match a header production above
                   and does not begin with "> **Result:** " ? ;

non_structural_line = ? any line that does not match a header
                        production above ? ;

blank_line      = NEWLINE ;

NUMBER          = "0" | NONZERO_DIGIT, { DIGIT } ;

DIGIT           = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

NONZERO_DIGIT   = "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;

IDENTIFIER      = LETTER, { LETTER | DIGIT | "-" | "_" } ;

LETTER          = "a" | "b" | ... | "z" | "A" | "B" | ... | "Z" ;

ANY_CHAR        = ? any Unicode character ? ;

ESCAPED_BACKSLASH = "\\\\" ;

ESCAPED_BACKTICK = "\\`" ;

NEWLINE         = ? line terminator (LF or CRLF) ? ;
```

## 3. Semantic Constraints

Beyond the syntactic rules, the following semantic constraints must be validated:

The grammar tolerates optional blank lines immediately after `## Tasks` in a
single-file plan and at the start of a workspace task file. Implementations
should accept those blank lines in both formats.

Throughout this specification, a *task node* means any authored node entry at
heading level `###`, `####`, `#####`, or `######` whose keyword is declared in
`structure.nodeKinds` (or the default `[task]` when omitted). Root task nodes
live directly under `## Tasks`; deeper task nodes are children of the nearest
shallower task node above them.

Throughout this specification, a *terminal state* means any state marked
`final: true` in the active state machine. In the built-in `rhei` machine, the
terminal state is `completed`.

Throughout this specification, a *leaf task node* means a task node with no
child task nodes. A **non-leaf task node is a task in its own right**: it
carries its own `**State:**`, its own journey through the machine, and its own
result. Nothing derives a parent's state from its children — no command stamps
a parent terminal because its descendants are ([§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup)) — because
once the children are finished the parent may still owe the verification,
review, or integration its machine declares.

Exactly one invariant ties a parent to its children, and it is a property of
the graph rather than of any one command: **a task node may enter a terminal
state only after every one of its descendants is terminal.** Because it is
structural, it is enforced on the shared transition path, identically for every
verb that can move a task ([§FS-rhei-transition-cmd.3.1](rhei-transition-cmd.spec.md#31-descendants-first-on-terminal-entry)); a resting violation is
a `rhei validate` **error** ([§FS-rhei-validate.4](rhei-validate.spec.md#4-behavior)).

Work eligibility follows from that invariant: **a non-leaf task node is
workable only once all of its descendants are terminal.** The same rule governs
manual claim selection ([§FS-rhei-next.3](rhei-next.spec.md#3-default-behavior-claim-mode)) and autonomous scheduling
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)), so a parent and one of its own descendants are never worked
at the same time, and a parent is never scheduled ahead of the subtree it
integrates. A leaf task node satisfies the rule trivially.

One declared exception refines the eligibility rule without touching the
invariant: a task whose current state is a *supervising* state
([§FS-rhei-supervision](rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification)) is worked *between* its descendants rather than only
after them. The engine wakes it at checkpoints of its subtree and holds the
subtree while it is owed a visit or running, so a supervisor and one of its
descendants are still never worked at the same time, and the supervisor may
still enter a terminal state only after every descendant is terminal. The
hold/release rule is specified in [§FS-rhei-supervision.3](rhei-supervision.spec.md#3-hold-and-release).

Dependency readiness requires successful terminal dependencies: a task is ready
with respect to `**Prior:**` only when every referenced dependency is in a
terminal state whose normalized state name is not `cancelled`. State-machine
`instructions` text is descriptive guidance for agents and must not narrow or
override this readiness rule unless the machine introduces a separate normative
field for that purpose.

When frontmatter defines a `structure` map, the following keys are meaningful
to this specification:

- `structure.maxLevels` — maximum allowed **ticket depth**, counted from
  depth 1 at `### <kind> ...` (`#### ` is 2, `##### ` is 3, `###### ` is 4).
  Valid values are `1` through `4`. If omitted, the default is `2`, so a plan
  may nest one level of subtasks without declaring anything. This is the same
  number `rhei list --json` reports as a ticket's `depth`.

  Ticket depth is **not** the project-tree level used by node policy and the
  Plan Root Model below, which counts the virtual Panta root as level 0 and
  rheis as level 1 — so ticket depth 1 is project level 2. `maxLevels` and
  `depth` always speak in ticket depth; `node_policy` selectors and the reserved
  `panta`/`rhei` kinds always speak in project level.
- `structure.nodeKinds` — allowed node-kind keywords for task nodes. Values
  must be unique `IDENTIFIER`s. Parsing and validation normalize them
  case-insensitively. If omitted, the default is `[task]`.

`panta` and `rhei` are reserved node-kind names. `panta` denotes the project
itself and is always the kind of the single virtual root at level 0. `rhei`
denotes a plan (flow) and is the kind of every level-1 node directly under Panta.
Neither `panta` nor `rhei` may appear in `structure.nodeKinds`. `panta` must
never be authored; a level-2-or-deeper node must never use the `panta` or `rhei`
kind.

### Plan Root Model

The level-0 root node is **Panta**, the project root ([§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)). Panta is
virtual: it is not authored in markdown, has no `**State:**`, `**Prior:**`,
`**Assignee:**`, or `> **Result:**` line, and is not persisted as a node in any
plan file or workspace task file. Runtime commands must not claim, transition,
complete, cancel, or reset Panta. Its rheis are the level-1 nodes beneath it; a
ticket (a `task`-kind node) is level 2 or deeper. The exact id-extension and
load rules for this hierarchy are specified in [§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture).

`node_policy.root` validates and names the state-machine profile for tools that
model the Panta root internally, but it does not create a persisted or executable
node. UI-level project status, including any `plan_state` shown by visualization
tools, is derived from authored task nodes rather than from a root state field.

The heading keyword is the node kind. By convention, authored headings render
that keyword in Title Case (`task` -> `Task`, `bug` -> `Bug`), but semantic
matching is case-insensitive.

Each node's state policy — which state it starts in and which states it may
ever hold — is resolved from the active state machine's `profiles` and
`node_policy` blocks. The Panta root resolves through `node_policy.root`; all
other nodes resolve through `node_policy.overrides`, then
`node_policy.by_type[<kind>]`, then `node_policy.default`. See the
[States Specification](rhei-states.spec.md#9-node-policy) for the full
resolution order and validation rules.

### 3.1. Dependency Integrity

All task references in `**Prior:**` fields must resolve to existing task nodes
in the same logical plan: in a Single-File Plan that means the same document,
and in a Directory Workspace that means the merged workspace graph across all
task files under `tasks/`. A `**Prior:**` list must not contain duplicate
references, must not reference its own task (self-reference is a 1-cycle), and
must not reference any ancestor of the task. A child task cannot list its
parent as `**Prior:**`; if generated follow-up work must wait for a completed
parent task, author that follow-up as a top-level sibling with `**Prior:**`
pointing at the completed task.

```markdown
### Task 2: Implementation
**State:** pending
**Prior:** Task 1, Task 3    ← Task 1 and Task 3 must exist
```

Directory Workspace example:

```markdown
# tasks/backend.md
### Task api: Build API
**State:** pending

# tasks/frontend.md
### Task ui: Build UI
**State:** pending
**Prior:** Task api    ← Valid: resolves across the merged workspace graph
```

When a dependency reference includes a node kind, that kind must match the
declared kind of the referenced node. A kind keyword that is not a declared
node kind of the plan is reported as such; when the reference also resolves to
no task, the error additionally suggests that the author may have pasted a task
*title* — `**Prior:** Design schema` parses as kind `Design`, id `schema` — and
points at referencing the task by id instead. Both checks run in the validator
rather than the parser: a reference may point across rheis in a Panta project
([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)), so the set of declared kinds and the referenced node's kind
are only known once the whole project is loaded.

Invalid child dependency example:

```markdown
### Task fetch-prs: Fetch pull requests
**State:** completed

#### Task fetch-prs.ci-failure-5227: Triage CI failure
**State:** pending
**Prior:** Task fetch-prs    ← ERROR: child cannot depend on its parent
```

### 3.2. State Validity

All state values must be defined in the associated states configuration resolved
by the state-machine resolution rules in
[State Machine Resolution](#13-state-machine-resolution).

In addition, each authored `**State:**` must be a member of the node's
resolved profile's `allowed` set, as determined by the active state
machine's `node_policy`. A state that is defined in the global `states`
block but excluded from the node's resolved profile is a validation error
on that node.

State-name rendering is normative across the plan and state-machine specs:

- A canonical state name that matches `IDENTIFIER` exactly may be written bare:
  `**State:** pending`.
- Any canonical state name containing whitespace or any character outside
  `IDENTIFIER` must be written in backticks, using the backticked forms shown
  in the examples below.
- Inside the backticked form, `\\` encodes a literal backslash and `\`` encodes
  a literal backtick.
- When related specs describe artifact or transition state names as arbitrary
  YAML strings, this markdown encoding is the corresponding representation in a
  plan file.

When a state machine state declares `visits: <n>`, the authored markdown may
encode later counted visits directly in `**State:**` using a `-<visit>` suffix:

```markdown
**State:** review      ← first visit (implicit)
**State:** review-2    ← second visit
**State:** `human review-3`  ← third visit for a spaced state name
```

The canonical state machine state is the unsuffixed base name (`review`,
`human review`). The suffix is only valid for states that declare `visits`, must
be greater than `1`, and must not exceed the declared visit budget.

Parsing rule for ambiguous names:

1. Remove optional surrounding backticks and first try to match the rendered
   value exactly against a loaded state name.
2. Only if no exact match exists may an implementation interpret a trailing
   `-<digits>` suffix as a counted visit.
3. In that case, the unsuffixed base name must itself exactly match a loaded
   state name, and the parsed visit count becomes the suffix value.

This means a machine that defines a literal state named `review-2` treats
`**State:** review-2` as that canonical state on visit 1. It is only parsed as
"state `review`, visit 2" when no literal `review-2` state exists.

Additional examples:

```markdown
**State:** pending                 ← canonical state `pending`
**State:** `human review`          ← canonical state `human review`
**State:** `qa/review`             ← canonical state `qa/review`
**State:** `security.review-2`     ← canonical state `security.review`, visit 2
**State:** `review-2`              ← either literal state `review-2` or, if absent, state `review` visit 2
```

### 3.3. Acyclic Dependencies

The task dependency graph must be a Directed Acyclic Graph (DAG). Circular dependencies are forbidden:

```markdown
### Task 1: First
**State:** pending
**Prior:** Task 2    ← ERROR: creates cycle

### Task 2: Second
**State:** pending
**Prior:** Task 1    ← ERROR: creates cycle
```

### 3.4. Hierarchical Task Consistency

Task nodes form a tree. Authored heading depth and task-id path depth must
match:

- a `### <kind> ...` node must use a one-segment id such as `1` or `api`
- a `#### <kind> ...` node must use a two-segment id such as `1.1` or `api.cache`
- a `##### <kind> ...` node must use a three-segment id
- a `###### <kind> ...` node must use a four-segment id

Child task ids must extend their parent id by exactly one segment, and sibling
task ids must be unique within the same parent.

```markdown
### Task 2: Parent Task
**State:** pending

#### Task 2.1: Valid child
**State:** pending

#### Task 3.1: Invalid child
**State:** pending
                         ← ERROR: child id does not extend parent id `2`

#### Task 2.1: Duplicate sibling
**State:** pending
                         ← ERROR: duplicate child id under Task 2
```

Mixed numeric and named path segments are valid so long as the full child id
extends the parent id by one segment:

```markdown
### Task api: Build API
**State:** pending

#### Task api.cache: Add cache layer
**State:** pending

##### Task api.cache.1: Verify cache key behavior
**State:** pending
```

Task id segments are canonical. A numeric segment must be `0` or a decimal
integer with no leading zeroes, and it must fit in the unsigned 32-bit range
`0..=4294967295`. `Task 1` and `Task 01` are not two distinct ids:
`Task 01` is invalid syntax because of the leading zero. `Task 4294967296` is
semantically invalid because the numeric segment is out of range. Result paths,
dependency references, rendered ids, and uniqueness checks all use the canonical
task id written in the plan.

Task depth must not exceed `structure.maxLevels`.

### 3.5. Identifier Uniqueness

Task ids must be unique across the entire plan. In a Single-File Plan, two
task nodes with the same id are an error. In a Directory Workspace, two task
nodes with the same id across *any* files within the `tasks/` directory are an
error.

In a Panta Project, each ticket's project-wide id is its rhei id joined with its
rhei-local id (rhei-local `1` under rhei `auth` is `auth.1`; rhei-local `3`
under the synthetic basin rhei is `basin.3`). Uniqueness is checked on these
fully-qualified ids across the whole project, and rhei ids must themselves be
unique across the project directory plus the optional synthetic `basin` rhei.
Because the rhei
id prefixes every ticket beneath it, authors only need rhei-local uniqueness
within each rhei plus unique rhei ids. Authored rhei-local ids and heading depth
are validated per rhei exactly as in a standalone plan
([§FS-rhei-plan-language.3.4](rhei-plan-language.spec.md#34-hierarchical-task-consistency)); the Panta prefix is applied at merge and is not
authored into task headings.

### 3.6. Link Integrity

All relative markdown links (`[text](target)`) in content sections and task
node content must resolve to existing files. In a Single-File Plan, links
resolve relative to the directory containing the plan file. In a Directory
Workspace, they resolve relative to the workspace root, meaning the directory
containing `index.rhei.md`, even when the link appears in a nested file under
`tasks/`.

External URLs (`http://`, `https://`, `mailto:`), and fragment-only anchors (`#section`) are not checked. When a link contains a fragment (`file.md#section`), only the file portion is verified.

A link inside a fenced code block — a run of three or more backticks or
tildes, closed by a run of the same character at least as long — or inside an
inline code span is an illustration of the format, not a reference to a file,
and is not checked. A plan that documents how to write a task quotes links that
were never meant to resolve, in a content section as in a task body. A fence
that is never closed runs to the end of the text that opened it.

For Directory Workspaces, implementations must not resolve `./` or `../`
against the physical path of the task file that contains the link. This keeps
links stable when tasks move between files or when task files are nested under
`tasks/`.

For workspace-relative validation, implementations must join the relative
target to the workspace root and then normalize `.` and `..` path segments. If
the normalized path escapes the workspace root, the link is invalid.

```markdown
## Overview
See [the spec](specs/language.md) for details.    ← specs/language.md must exist

### Task 1: Setup
**State:** pending

Read [guide](https://example.com/guide)           ← OK: external URL, not checked
See [section](#overview)                           ← OK: fragment-only, not checked
See [missing](docs/nonexistent.md)                 ← ERROR: file does not exist
```

Directory Workspace example with a nested task file:

```markdown
# <workspace>/tasks/backend/api.md
### Task api: Build API
**State:** pending

See [guide](./docs/guide.md)        ← resolves to <workspace>/docs/guide.md
See [shared](shared.md)             ← resolves to <workspace>/shared.md
See [protocol](specs/http.md#post)  ← checks <workspace>/specs/http.md only
See [notes](#api-notes)             ← fragment-only anchor, not checked
```

### 3.7. Node Kind Validity

The heading keyword for each task node must be listed in `structure.nodeKinds`.
When `structure.nodeKinds` is omitted, only `Task` is valid.

Example:

```markdown
---
structure:
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Stabilize release
**State:** pending

#### Bug 1.1: Fix crash in cache warmer
**State:** pending        ← Valid

#### Spike 1.2: Unknown category
**State:** pending        ← ERROR: `spike` is not a declared node kind
```

### 3.8. Result Block Consistency

When a task contains a `> **Result:**` block, that block must describe the
enclosing task itself. Text and target are validated **as a pair**: both name
the same ticket, in the same form.

- The link text must equal the enclosing task's `task_id`, and the target path
  must be `runtime/results/<task-id>.md` using that same id.
- Because every ticket gained a rhei prefix when bare rheis became implicit
  Pantas ([§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup)), the id may be written in either form: the
  project-qualified id (`[auth.1](runtime/results/auth.1.md)`) that commands
  write from here on, or the **legacy rhei-local** id
  (`[1](runtime/results/1.md)`) left in plans completed before that change.
  Both validate; there is no migration pass, and no command rewrites the
  result link of a ticket it is not completing. `rhei complete` refreshes the
  completed ticket's link to the qualified artifact it writes
  ([§FS-rhei-panta.6.3](rhei-panta.spec.md#63-completion-and-rollup)).
- A link that **mixes** the two forms (`[auth.1](runtime/results/1.md)`), or
  names any other id, is an error — it would point at an artifact that does not
  hold this ticket's result.

A result block is optional syntax, but it has a lifecycle invariant:

- A non-terminal task must not contain a result block.
- A terminal task may contain one valid result block. Validation does not
  require every terminal task to have one, because terminal states can be
  reached by commands other than `rhei complete` and by imported plans.
- `rhei complete` must create or preserve exactly one valid result block for a
  successful non-cancelled terminal completion.
- `rhei transition` may append audit entries to the result file, but it never
  adds a result block to the task body.
- `rhei reset` removes result blocks along with other runtime completion
  artifacts.

Example:

In a rhei whose id is `web` (from `web.rhei.md` or the directory `web/`):

```markdown
### Task api: Build API
**State:** completed
> **Result:** [web.api](runtime/results/web.api.md)  ← Valid (qualified)

### Task ui: Build UI
**State:** completed
> **Result:** [ui](runtime/results/ui.md)            ← Valid (legacy rhei-local)

### Task docs: Write docs
**State:** completed
> **Result:** [web.docs](runtime/results/docs.md)    ← ERROR: mixes the two forms

### Task cli: Build CLI
**State:** completed
> **Result:** [web.api](runtime/results/web.api.md)  ← ERROR: references a different task id
```

`result_block` is validated by this task-local rule rather than by the general
link-integrity check above. The file may be created later by runtime commands
such as `rhei complete`.

### 3.9. Terminal Tree Coherence

A task node in a terminal state must not contain any non-terminal descendants.

```markdown
### Task 2: Parent
**State:** completed

#### Task 2.1: Still open
**State:** pending        ← ERROR: terminal parent with non-terminal child
```

### 3.10. State Artifact Contracts

The active state machine may declare required file `inputs` and `outputs` for a
state. These contracts are part of execution semantics, not markdown syntax:

- Entering a state may require one or more input files to already exist.
- Leaving a state may require one or more output files to have been written.
- Artifact paths are resolved relative to the plan root (single-file plan) or
  workspace root (directory workspace).

This section is normative for artifact-path resolution across the Rhei spec
set. The execution root is defined as:

- The directory containing the `.rhei.md` plan file for a Single-File Plan.
- The directory containing `index.rhei.md` for a Directory Workspace.

When related specs describe artifact `path` values as "workspace-relative"
templates, they mean relative to this execution root. In other words, the same
artifact template is interpreted relative to the single-file plan directory in
single-file mode and relative to the workspace root in directory-workspace
mode.

This execution root is the Rhei artifact root, not necessarily the subprocess
checkout root. Agent subprocess cwd resolution is defined by the agents spec so
repository-root instruction files and task worktrees can be used without moving
Rhei runtime artifacts ([§FS-rhei-agents.4](rhei-agents.spec.md#4-environment-variables)).

Because artifact existence depends on runtime workspace state, this constraint
is enforced by execution commands such as `rhei transition`, `rhei complete`,
`rhei run`, and `rhei next`, rather than by pure syntax validation of markdown
alone. When a transition also declares `on_leave` or `on_enter` callbacks,
those callbacks are optional per edge: an omitted callback is treated as
implicit success.

This section is canonical for artifact enforcement order across commands:

| Command | Enforced artifacts | Ordering |
|---------|--------------------|----------|
| `rhei next` | Current-state `inputs` only | Before a task is claimable, resolve the current state's inputs. Required inputs must exist; optional inputs are resolved and exposed but missing optional inputs do not block. Claim mode re-checks under the file lock immediately before writing `**Assignee:**`. `next --peek` does not check outputs, run callbacks, write state, or write result files; claim mode may auto-advance non-runnable initial states as defined in the next-command spec. |
| `rhei transition` | Source-state `outputs`; target-state `inputs`; the target's terminal result | After the compare-and-swap guard and edge validation, execute `on_leave` unless skipped. Then check required source outputs — skipped when the effective target is `cancelled`, which abandons the work rather than finishing it — resolve target inputs for the final target state, and reject before the state write if any required artifact is missing. Optional target inputs are skipped for blocking but still resolved. When the final target is `final: true`, its implicit terminal result is checked at the same point ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)): a non-empty `runtime/results/<task-id>.md` or a `--result` message, or the transition is rejected before the state write. Write the target state, execute `on_enter` unless skipped, atomically persist the task file, then append the transition audit entry to `runtime/state-transitions.log` and, for a terminal target, the terminal finalization. |
| `rhei complete` | Source-state `outputs`; completion-target `inputs`; the completion target's terminal result | Select the non-cancelled terminal completion target first, then use the same transition artifact order as `rhei transition`, carrying `--result` as the terminal result. `rhei complete` adds no artifact rule of its own; its target is never `cancelled`, so the cancellation waiver never applies to it. |
| `rhei run` | Current-state `inputs`; source-state `outputs` for successful work; target-state `inputs`; the selected target's terminal result | Before spawning work, the ready-set scan checks current-state required inputs and skips missing optional inputs for blocking. After a subprocess exits `0`, required source outputs are part of the completion condition; if any are missing, no transition fires, the task stays in its current state, it is not spawned again within the pass, and the run continues with the other claimable tickets in both execution modes ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) step 5). When the transition the run would select lands on a `final: true` state, the ticket's non-empty `runtime/results/<task-id>.md` — or, per invocation of a fanned-out state, its own fragment under `runtime/results/<task-id>/<state>/<visit_count>/` — is part of that same completion condition and a missing one is reported exactly like a missing required output ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop), [§FS-rhei-agents.3.2](rhei-agents.spec.md#32-completion-condition), [§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). Non-zero, timeout, and tooling-failure routes select error transitions as specified by `rhei run`, do not require normal source outputs, and carry an engine-written result message when they land on a terminal state. For any selected target transition, required target inputs are checked before the state write and optional target inputs do not block. A successful-work transition also re-checks source outputs after `on_leave` and before the state write. Result-file writes, if any, happen only after the transition succeeds. |

One artifact in the table is never declared: the terminal result. Every `final:
true` state requires a non-empty `runtime/results/<task-id>.md` on the edge into
it, checked where that state's `inputs:` are checked, on every path. It is
specified in [§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result) and enforced in [§FS-rhei-transition-cmd.3.2](rhei-transition-cmd.spec.md#32-terminal-result-on-entry).

For every declared input, required or optional, implementations resolve the path
and expose the path and existence flag to templates, agents, and programs.
`optional: true` is valid only on inputs. Outputs are always required when
declared, and a missing output blocks the transition out of the source state
except for the `rhei run` failure, timeout, and tooling-unavailable routes
described above, and except on a transition into the `cancelled` state.
Cancellation abandons the work, so the source state's artifact contract is
moot: there is no output to require of a step that is not being finished. The
terminal-result obligation is *not* waived with it — a cancel into `cancelled`
still needs a `--result` message or a result already on disk, because a
cancelled ticket still has to say why.

Examples:

```markdown
# Single-file plan at /repo/plans/release.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/plans/runtime/reviews/release.md

# Directory workspace at /repo/release/index.rhei.md
Artifact path: runtime/reviews/release.md
Resolves to: /repo/release/runtime/reviews/release.md
```

### 3.11. Task Execution Overrides

A task may pin the execution identity used when an agent works that task,
independent of the per-state `target` declared by the active state machine
([§FS-rhei-states.1.2](rhei-states.spec.md#12-per-state-fields)). Two optional, mutually exclusive metadata fields express
this at two altitudes:

- `**Model:**` overrides only the *model* dimension. Each state the task enters
  keeps its own agent, mode, and provider; only the resolved model is replaced.
  This is the common case: capability and cost are properties of the work item,
  while agent identity is a property of the phase.
- `**Target:**` overrides the *full* execution identity using an inline target
  selector — one of the four `<agent>[<mode>]:<provider>:<model>` forms defined
  in [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification). It replaces the agent, mode, provider, and model the state
  would otherwise resolve.

Both fields, when present, apply in **every** state in which the task is run by
an autonomous agent — the override follows the work item across its lifecycle
rather than belonging to a single phase. They have no effect in states that
perform no autonomous agent work (human-gated states, program states).

**Mutual exclusivity.** A single task must declare at most one of `**Model:**`
or `**Target:**`. Declaring both is a validation error, because a `**Target:**`
selector already carries a model and the combined intent would be ambiguous.

**Validation.**

- `**Target:**` must be a non-empty string matching one of the selector forms
  in [§FS-rhei-states.1.3](rhei-states.spec.md#13-validation-rules), and its `<agent>`, `<mode>`, `<provider>`, and
  `<model>` segments must resolve under the same rules a state `target` is held
  to.
- `**Model:**` must name a model profile available to the active state machine —
  the same membership check applied to a state's `model` field
  ([§FS-rhei-states.1.3](rhei-states.spec.md#13-validation-rules)).
- A task override on a *fanout* state — one declaring `all_targets` or
  `all_models` — is a validation error. Fanout means "run once per declared
  target," which a single per-task identity contradicts. A future revision may
  relax this.
- A task override targeting a state that declares `target_locked: true`
  ([§FS-rhei-states.1.2](rhei-states.spec.md#12-per-state-fields)) is rejected: such states pin an execution identity that
  is essential to the phase and must not be reassigned per task.

**Resolution precedence.** A task override sits between the command-line
override and the state in the agent/model resolution chain
([§FS-rhei-agents.1.4](rhei-agents.spec.md#14-resolution-order)):

```
CLI override  >  task **Target:** / **Model:**  >  state target /
all_targets / legacy model+agent  >  settings defaults
```

`**Target:**` is resolved exactly as a state `target` selector. `**Model:**`
resolves the state's identity normally and then substitutes the model segment,
whether the state used a modern `target` selector or the legacy `agent`+`model`
split.

Example — two sibling tasks in the same state, executed with different models:

```markdown
### Task 7: Tricky migration
**State:** pending
**Model:** claude-opus-4-7

### Task 8: Routine follow-up
**State:** pending
**Prior:** Task 7
```

Both tasks pass through the same states, but Task 7 runs on `claude-opus-4-7`
in every agent state while Task 8 uses each state's default model.

### 3.12. Task Exports

A task hands work product to later tasks by publishing a named **export**. The
producer declares what it publishes and the consumer declares what it reads,
both in task metadata:

```markdown
### Task 1: Design the session API
**State:** completed
**Provides:** api-contract

### Task 2: Implement the client
**State:** pending
**Prior:** Task 1
**Consumes:** 1:api-contract
```

An export is a file at a path derived from the producing task's id and the
export name, under the execution root of the rhei that owns that task:

```text
runtime/exports/<task-id>/<name>.md
```

The path is a convention, not a template: it is keyed by the *task*, never by
the state that happened to write it. A consumer resolves it from the plan graph
alone, so producer and consumer need not share a state machine, a workflow
phase, or even a rhei — the export of a cross-rhei prior resolves under that
prior's own root ([§FS-rhei-panta.6.1](rhei-panta.spec.md#61-readiness-and-rhei-next)).

This is a plan-level handoff, deliberately outside the state machine. State
handoffs ([§FS-rhei-states.3.2](rhei-states.spec.md#32-state-handoffs)) carry notes between the states of *one* task and
are declared in `states.yaml`; exports carry work product between *different*
tasks and are declared in the plan, because the dependency graph — not the
workflow — is what orders them.

**Fields.** `**Provides:**` is a comma-separated list of export names.
`**Consumes:**` is a comma-separated list of `<task-id>:<name>` references. An
export name starts with a letter or digit and continues with letters, digits,
`.`, `_`, or `-`, so that it is safe as a path segment. Both fields are
optional; both follow `**State:**` as every metadata field does. Repeating a
name within one task's `**Provides:**`, or repeating a reference within one
task's `**Consumes:**`, is a parse error: the first leaves a consumer no way to
say which export it meant, the second is a leftover from an edit.

**Runtime semantics.** Rhei injects each consumed export that exists into the
consuming agent's prompt, and tells a producing agent where to write each
export it declares ([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)). A consumed export that was never
written, or that is empty, is skipped: the section is simply absent from the
prompt.

Validation resolves every consumed export to a task and a name in that task's
`**Provides:**`, even when the export file has not been written yet. A producer
need not be a prior: `**Consumes:**` declares data flow, while `**Prior:**`
declares scheduling. A declared export that was not written remains an optional
prompt input and is silently skipped unless a state declares the same artifact
as a required input. An exclusion may never overlap a consumed export; §3.13
defines the overlap rule and makes that contradiction a validation error.

### 3.13. Task Read Exclusions

A task may declare source artifacts that its agent must not read. The field is
task-level, optional, and occurs after `**Consumes:**` and before assignee or
execution overrides:

```markdown
**Excludes:** checkout=notes/private.md, artifact=runtime/reviews/, review.2:statement
```

`checkout=<path>` is relative to the task's effective checkout, including a
task-specific worktree. `artifact=<path>` is relative to the execution root of
the rhei that owns the task. An export reference uses the grammar of
`**Consumes:**`: a local task wins; otherwise a name-led dotted id remains
project-qualified and resolves through the merged graph. The referenced task
and export name must be declared, but the export file need not exist.

Authored paths use `/` on every platform and may contain spaces. They may not
be empty, absolute or platform-prefixed, contain `\` or `,`, or contain empty,
`.` or `..` components. A trailing `/` names a directory recursively; without
it, the entry names exactly one file even before that file exists. Resolution
uses path components, never string prefixes: `private/` covers
`private/note.md`, not `private-copy/note.md`.

Rhei resolves every entry to a logical absolute target beneath its declared
root. It canonicalizes the longest existing ancestor and appends the remaining
components, so aliases through an existing symlink cannot evade the boundary
when the leaf does not yet exist. A symlink and its in-root canonical target
are both targets. A symlink that escapes the declared root is invalid.
Resolution and canonicalization run during validation and again immediately
before each agent spawn. This second pass is authoritative when the filesystem
changed after validation.

Two authored entries are duplicates when their logical or canonical targets
are identical, or when one directory entry contains the other target; the
validator reports the redundant spellings rather than silently selecting one.
Containment and equality use normalized path components and resolved identity.
Rhei does not track hard links or content copied to a different artifact: this
is a source-artifact boundary, not content-provenance control.

An exclusion is invalid when its exact or canonical target, or an excluded
ancestor directory, overlaps any of these invocation requirements:

- an export named by the same task's `**Consumes:**`;
- the current task's authored source file or the active state-machine source;
- a required state `inputs:` artifact or required handoff.

An optional prior, child, history, result, brief, checkpoint, context, or
handoff payload may be excluded. Its bytes are then omitted, while the graph
identity and navigation metadata that name the task and artifact remain. A
required-input conflict is invalid rather than being converted to optional
omission. A task that can enter a state with named `snapshot.inherit` may not
declare `**Excludes:**` in v1 ([§FS-rhei-snapshots.11](rhei-snapshots.spec.md#11-validation-rules)).

The resolved entries form one policy reused by prompt composition and process
launch ([§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition)). Plans with no `**Excludes:**` preserve their prior
parse, validation, composition, and execution behavior. Program states,
callbacks, `--no-agent`, and `rhei snapshot continue` do not launch a new agent
under this contract and are unchanged.

## 4. Token Types

This section is illustrative and non-normative. A complete implementation must
support every normative grammar production above, including YAML frontmatter,
`**Assignee:**`, and `> **Result:**` blocks.

For lexer implementation, the following token types are a reasonable minimum:

| Token | Pattern | Example |
|-------|---------|---------|
| `RheiHeader` | `# Rhei: .*` | `# Rhei: My Project` |
| `MetadataStates` | `\*\*States:\*\* .*` | `**States:** rhei` |
| `FrontmatterFence` | `^---\s*$` | `---` |
| `FrontmatterYamlLine` | Any line inside frontmatter that is not `---` | `metadata:` |
| `TasksSection` | `^## Tasks\s*$` | `## Tasks` |
| `SectionHeader` | `^## .+$` (matched only if `TasksSection` did not match) | `## Overview` |
| `NodeHeader` | `^(###|####|#####|######) <kind> <id>: .*` | `#### Bug 1.2: Config` |
| `MetadataState` | `\*\*State:\*\* .*` | `**State:** pending` |
| `MetadataPrior` | `\*\*Prior:\*\* .*` | `**Prior:** Bug 1.2` |
| `MetadataProvides` | `\*\*Provides:\*\* .*` | `**Provides:** api-contract` |
| `MetadataConsumes` | `\*\*Consumes:\*\* .*` | `**Consumes:** 1:api-contract` |
| `MetadataExcludes` | `\*\*Excludes:\*\* .*` | `**Excludes:** checkout=private.md` |
| `MetadataAssignee` | `\*\*Assignee:\*\* .*` | `**Assignee:** alice` |
| `MetadataModel` | `\*\*Model:\*\* .*` | `**Model:** claude-opus-4-7` |
| `MetadataTarget` | `\*\*Target:\*\* .*` | `**Target:** codex[safe]:openai:gpt-5-codex` |
| `ResultBlock` | `^> \*\*Result:\*\* \[[^]]+\]\([^)]+\)\s*$` | `> **Result:** [task-1](runtime/results/task-1.md)` |
| `Text` | Any other line | Description text |

## 5. AST Node Types

This section is also illustrative and non-normative. It shows one viable shape
for a parser AST, but it is not a complete or exclusive contract.

For parser implementation, the following AST structure is recommended:

```rust
struct Rhei {
    title: String,
    states: String, // state machine name; defaults to "rhei" when omitted
    frontmatter: Option<YamlValue>,
    content_sections: Vec<ContentSection>,
    tasks: Vec<TaskNode>,
}

struct ContentSection {
    title: String,
    content: String,
}

struct TaskNode {
    id: TaskId,
    title: String,
    state: String,
    kind: String,
    prior: Vec<TaskId>,
    provides: Vec<String>,          // export names published (**Provides:**)
    consumes: Vec<ConsumedExport>,  // exports read from prior tasks (**Consumes:**)
    excludes: Vec<Exclusion>,       // authored read exclusions (**Excludes:**)
    assignee: Option<String>,
    model: Option<String>,    // per-task model override (**Model:**)
    target: Option<String>,   // per-task full execution identity (**Target:**)
    content: String,
    result: Option<ResultLink>,
    children: Vec<TaskNode>,
}

struct TaskId {
    segments: Vec<TaskIdSegment>,
}

enum TaskIdSegment {
    // Numeric segments are canonical: no leading zeros and within u32 range.
    Number(u32),
    Named(String),
}

struct ResultLink {
    task_id: TaskId,
    path: String,
}

struct ConsumedExport {
    task: TaskId,
    name: String,
}
```

## 6. Language Classification

The Rhei Plan language is **context-sensitive** because:

1. Child task ids must align with parent task ids and heading depth
2. `Prior` references must resolve to existing task definitions
3. Node-kind keywords depend on plan-level `structure` configuration
4. State values depend on external states configuration
5. State artifact contracts depend on external workspace files at execution time

The language cannot be fully described by a context-free grammar alone; semantic analysis is required for complete validation.

## 7. File Extension

A single-file Rhei Plan document must use the `.rhei.md` extension: the file
stem is the rhei id that prefixes every ticket ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)), so a plan
without the suffix has no id and the loader rejects it. The bare `.md`
extension remains only for task files under a Directory Workspace's `tasks/`
directory (and basin task files), whose owning rhei supplies the id.

## 8. CLI Command Groups

The `rhei` CLI help currently organizes its subcommands into five groups:

| Group | Commands | Purpose |
| --- | --- | --- |
| **Inspection** | `validate`, `render`, `states`, `list`, `viz` | Read-only commands that examine or render a plan without modifying it |
| **Templates** | `templates`, `instantiate` | Discover and instantiate reusable plan and workspace templates |
| **Execution** | `transition`, `run`, `snapshot`, `next`, `complete`, `reset` | Commands that mutate the plan file or workspace state, or operate on execution runtime artifacts |
| **Setup** | `install-skills`, `completions` | Install packaged Rhei skills and generate shell completion scripts |
| **Info** | `version`, `help` | Meta commands about the tool itself |

`rhei viz` is the read-only **Inspection** command that renders the visualization
behavior surface ([§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization)) as a self-contained HTML page from a plan or
workspace; it appears in the generated help under that group.

## Related Specifications

- [Rhei Language Reference](rhei-language-reference.spec.md) - Entry point for the complete user-authored language surface
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
- [Plan Language Usage Guide](rhei-authoring.spec.md) - Practical authoring patterns and walkthroughs
- [States Specification](rhei-states.spec.md) - Defines the states configuration format
- [Transitions Specification](rhei-transitions.spec.md) - Formal state transition system, callbacks, and YAML schema
- [Validate Command](rhei-validate.spec.md) - `rhei validate` semantic checks
- [Render Command](rhei-render.spec.md) - `rhei render` output formats
- [States Command](rhei-states-cmd.spec.md) - `rhei states` state-machine inspection
- [List Command](rhei-list.spec.md) - `rhei list` filter set and output format
- [Next Command](rhei-next.spec.md) - `rhei next` behavioral contract, including `--peek` mode
- [Transition Command](rhei-transition-cmd.spec.md) - `rhei transition` compare-and-swap contract
- [Complete Command](rhei-complete.spec.md) - `rhei complete` behavioral contract
- [Run Command](rhei-run.spec.md) - `rhei run` execution loop under orchestrator authority
- [Reset Command](rhei-reset.spec.md) - `rhei reset` behavior for restoring initial state
- [Completions Command](rhei-completions.spec.md) - `rhei completions` shell completion generation
- [Version Command](rhei-version.spec.md) - `rhei version` component version output
- [State Machine Writer](rhei-state-machine-writer.spec.md) - Designing custom state machines from project specs and teams
