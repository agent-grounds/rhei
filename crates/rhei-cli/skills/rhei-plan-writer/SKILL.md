---
name: rhei-plan-writer
description: Create, refactor, and validate Rhei Plan markdown documents for project execution. Use when users ask for implementation plans, task breakdowns, roadmap-to-task conversion, dependency cleanup, or status updates in a strict, structured plan format with task states and prerequisites.
---

# Rhei Plan Writer

Produce a Rhei Plan markdown document that is deterministic, dependency-safe, and ready for execution.

## Output Contract

A Rhei Plan can be authored in two formats:

- **Single-File Plan** — one `.rhei.md` file containing the full task tree. Default for low-concurrency work.
- **Directory Workspace** — an `index.rhei.md` plus a `tasks/` directory of per-file task definitions. Use when multiple agents or humans will work the plan in parallel.

Default to Single-File unless the user asks for high concurrency or merge-conflict safety.

### Single-File Plan

- Emit exactly one H1: `# Rhei: <title>`.
- Optionally emit `**States:** <state-machine-name>` as the first non-empty line after the H1 to declare which state machine the plan follows. Omit to use the built-in `rhei` state machine.
- Optionally emit a YAML frontmatter block (see *Frontmatter*) after the `**States:**` field, before any H2 section.
- Emit zero or more contextual H2 sections before tasks, then `## Tasks` as the final H2 section with at least one task.

### Directory Workspace

- Emit `index.rhei.md` at the workspace root: H1, optional `**States:**`, optional frontmatter, optional H2 context sections — **no** `## Tasks` section.
- Emit one or more task files under `tasks/`. Each file begins directly with `### <kind> <id>:` headers and contains no `# Rhei:` header and no independent frontmatter.
- Frontmatter (including `structure`, `metadata.tasks.*`) lives only in `index.rhei.md` — the workspace has exactly one authoritative metadata map.
- Numeric IDs are the default here too, as long as tickets are added with `rhei new` (see *Adding to a Live Project*): it allocates the next free number under a lock, which is exactly the collision the format used to fear. Prefer letter-prefixed or name-style IDs (e.g., `task-avatar`, `bug-null-cache`) only when task files are hand-authored in parallel branches, where nothing is holding that lock.

### Task Block

Use this exact block shape for every task node (root and child):

```markdown
### Task <id>: <title>
**State:** <state>
**Prior:** Task <id>, Task <id>

<description>
```

Apply these rules:
- **Every task MUST have a `**State:**` field**, placed as the first metadata line directly under the heading (no blank line between). A task without `**State:**` is invalid and will fail validation — this is the single most common authoring mistake, so check for it before finishing (see *Planning Workflow* step 10).
- Place `**Prior:**` second when present; omit it when no prerequisites exist.
- **Do not author `**Assignee:**` or `> **Result:**` blocks** — both are runtime-owned: `rhei next` writes `**Assignee:**` when a task is claimed; `rhei complete` removes it and writes `> **Result:** [<id>](runtime/results/<id>.md)`, where `<id>` is the project-qualified ticket id (e.g. `plan.1` for `plan.rhei.md`).
- Separate metadata from description with a blank line. Emit no other metadata fields.
- Keep descriptions actionable and implementation-oriented.

### Allowed States

Run `rhei states` in the project to discover the allowed state values, their agent instructions, and the declared transitions for the state machine the plan will follow. Use `rhei states --state-machine <path>` to target a specific YAML file (e.g. the one a plan's `**States:**` line references), and `--json` when machine-readable output is preferred. Use only state values reported by that command, follow the printed instructions when describing task work, and respect the declared transitions when choosing initial states.

#### No-CLI state-machine resolution

If the `rhei` CLI is unavailable, resolve the effective declaration using this
table and then read the selected YAML directly. An explicit matching
`--state-machine <path>` remains a whole-scope override and is read first.

| Invocation scope | Effective `**States:**` | Matching file available | Resolution |
|---|---|---|---|
| Panta project default (inherited or restated) | `rhei` | project-root `states.yaml` | project-root file |
| Panta project default (inherited or restated) | `rhei` | member-root `states.yaml` only | built-in `rhei` |
| Standalone single-file plan | `rhei` | sibling `states.yaml` | sibling file |
| Standalone Directory Workspace | `rhei` | workspace-root `states.yaml` | workspace-root file |
| Panta custom project default | non-`rhei` | one matching member-root `states.yaml` | unique member-root file |
| Panta custom project default | `rhei` (member's own, differing) | none found anywhere | built-in `rhei` |

A plan with no effective `**States:**` declaration uses the built-in machine in
[default-states.md](references/default-states.md) and ignores automatically
discovered files. For a declared `rhei`, use a matching file only at the local
lookup location shown above; without one, use that same built-in machine and its
`pending`/`completed` states. In particular, a member that inherits or restates
a Panta project default named `rhei` cannot replace it with a member-root file.

For a non-`rhei` declaration outside a Panta project, use a matching sibling
`states.yaml` for a single-file plan or a matching `states.yaml` at a Directory
Workspace root. In a Panta project, a member's own declaration that differs
from the project default checks its execution root first. Otherwise check the
Panta project root; if it is absent or names a different machine, use a matching
file from the rhei roots only when exactly one exists. A missing match is an
error — except that a member's own differing declaration of `rhei` itself falls
back to the built-in machine instead of erroring. Multiple matches or an
unreadable candidate remain errors in every case, and no other automatic
location is searched
([§FS-rhei-plan-language.1.3](../../../../docs/functional-spec/rhei-plan-language.spec.md#13-state-machine-resolution)).

Each node's initial state comes from the machine's `profiles.<name>.initial` via `node_policy` — **not** from a state-level `initial: true` flag. In the built-in `rhei` machine the initial state is `pending`, so every task in a new plan under that machine starts in `pending`.

For markdown safety, format state names containing hyphens, spaces, or punctuation with backticks (`` **State:** `agent-review` ``, `` **State:** `human review` ``). Backticks are acceptable for all state values; canonical names matching `IDENTIFIER` exactly (e.g. `draft`, `pending`) may be written bare. For machines that declare a `visits` budget, a counted-visit suffix (`-<n>`, with `n >= 2`) may appear in the rendered state value for later visits — the plan writer normally does not author these; they accumulate at runtime.

### ID Policy

- Choose exactly one ID style per document and do not mix styles.
- Numeric style (`1`, `2`, `3`, ...) or named style (`setup`, `review`, `api`, ...).
- Prefer numeric IDs unless the plan is small and conceptual. Numeric is also right for a Directory Workspace whose tickets are added with `rhei new` — it allocates under a lock; reach for named IDs there only when task files are hand-authored in parallel branches.
- A ticket created with `rhei new` gets the next free number automatically, so do not pre-assign one; pass `--id <name>` only when the ticket deserves a name.
- Headings stay rhei-local (`### Task 1:`); command output shows the project-qualified form (`plan.1`) — never author the qualified prefix into a heading.

### Child Task Format

Decompose a task with nested `Task` nodes at a deeper heading level. Child nodes use the same block shape as roots — including the mandatory `**State:**` first line — but with a dotted id that extends the parent:

```markdown
#### Task <parent>.<child>: <title>
**State:** <state>

<description>
```

Apply these rules:
- The child id extends the parent id by exactly one new `.`-separated segment (`1.1`, `1.2.3`, `api.cache`). Numeric children increment from `1` within their parent; named children use short identifiers; mixed numeric/named segments are allowed as long as depth matches. Sibling ids must be unique under the same parent.
- Default to adding child tasks whenever a task benefits from progressive disclosure and per-step logging. Skip them only when the work is clearly atomic — and then make the task description explicit enough to act as a single implementation log entry. A ticket captured with `rhei new` starts as a leaf and stays valid as one; give it children when you come back to plan it, not to satisfy this rule at capture time.
- Heading depth is bounded by the plan's `structure.maxLevels` (default `2`, maximum `4`). H3 is depth 1, H4 is depth 2, H5 is depth 3, H6 is depth 4. A plan that needs more than two levels must declare `structure.maxLevels` in frontmatter.
- A parent is a ticket, not a folder. Once its children are all terminal it is handed to a worker like any other task — or, when the active machine puts it in a state that declares `execute_on:`, at every checkpoint *while* its children run — and that worker must have something to finish: integration, verification, the summary that ties the children together, or the judgement a supervisor makes between steps. Write the parent's description as that work. Nothing stamps a parent from its children, so a parent authored as an empty container becomes a ticket whose worker has no instructions.

### Frontmatter

Emit a YAML frontmatter block only when the plan needs non-default `structure` settings. The `structure` block is the only thing a plan writer should author in frontmatter; runtime-managed keys under `metadata.tasks.*` (such as `stateVisits` counters) are written by the CLI and must not be hand-authored.

```markdown
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---
```

- `structure.maxLevels` — maximum task depth, from `1` (`###` only) through `4` (`######` allowed). Default `2`. Required when any child task would exceed depth 2.
- `structure.nodeKinds` — allowed heading keywords. Default `[task]`. Add other kinds (`bug`, `spike`, `epic`, ...) only when the plan actually uses them; once declared, the title-cased form may appear as the heading keyword (e.g. `#### Bug 1.2: Fix null-cache panic`). The keyword `rhei` is reserved and must never appear in `nodeKinds`.

## Planning Workflow

1. Extract deliverables, constraints, and sequencing needs from the request.
2. Decompose the work into independently completable tasks.
3. Assign only real prerequisites to maximize parallel execution.
4. Build a dependency DAG and remove cycles before drafting. Ensure the graph is topologically sortable for execution order.
5. Draft concise context sections only when they improve implementation clarity.
6. Write each task and child task as concrete implementation instructions.
7. Set initial states correctly:
   - New plan: set every task to the active machine's profile `initial` (`pending` for the built-in machine).
   - Existing plan update: preserve truthful terminal states (`completed`, `cancelled`) unless explicitly changed, and preserve any `**Assignee:**` / `> **Result:**` blocks the runtime has written.
8. Save the plan where *File Location and Name* says — in the host project's `panta/`, never at the repository root.
9. Run the validation checklist before returning output.
10. **Final scan:** re-read every `### Task` / `#### Task` / deeper heading (or other declared kinds) and confirm each is immediately followed by a `**State:**` line. This is the most common defect — always perform this check last.

## Validation Checklist

Validate every response against all checks:

- One H1 matching `# Rhei: <title>` (Single-File Plan) or an `index.rhei.md` (Directory Workspace).
- If present, `**States:**` is the first non-empty line after the H1, and any YAML frontmatter sits between it and the first H2.
- `## Tasks` is present and last in Single-File Plans; omitted entirely from `index.rhei.md`.
- Every root task is `### <Kind> <id>: <title>` and every child is `#### <Kind> <parent>.<child>: <title>` (deeper at H5/H6 when `structure.maxLevels` permits).
- Every task (root or child) has `**State:**` with an allowed value from the resolved profile; `**Prior:**` appears only after `**State:**`; no `**Assignee:**` or `> **Result:**` is authored; no other metadata fields appear.
- Each `**Prior:**` references only existing tasks (resolved across the merged workspace graph in a Directory Workspace). Dependencies are acyclic: a task never self-references nor lists its parent or any ancestor — follow-up work that must wait for a parent is a top-level sibling, not a child.
- ID style is consistent; each child id extends its parent by exactly one segment; sibling ids under one parent are unique.
- Each task has child tasks unless it is clearly simple and non-decomposable. This applies to a plan drafted as a document; a ticket captured one at a time with `rhei new` is a leaf by construction, and decomposing it later is `rhei new --under <ticket-id>` or a plan edit.
- Heading depth ≤ `structure.maxLevels`; if mixed kinds are used, every heading keyword appears in `structure.nodeKinds` and `rhei` never does.

When the CLI is available, run `rhei validate <plan>` after writing — it performs the full grammar, state, dependency, link, and terminal-coherence checks the checklist only approximates.

## File Location and Name

Plans live in the host repository's Panta project: `panta/<id>.rhei.md` for a Single-File Plan, `panta/<name>/index.rhei.md` for a Directory Workspace. Before writing anything, look for `index.panta.md` — in the working directory or in its `panta/` child — or just run `rhei list`. When the project is there, `rhei new "<title>"` writes the file into it from any directory beneath the host and you never choose a path (see *Adding to a Live Project*); hand-write into that same directory only when the CLI is unavailable. Discovery reads the project directory's immediate children and nothing else, so a plan left at the repository root or tucked into a subfolder of `panta/` is not a rhei — and nothing tells you: `rhei list` never shows its tickets and `rhei validate` still succeeds.

When the host has no project yet, creating one stays the human's call, because `rhei init` writes `.gitignore` and `AGENTS.md` in their repository. Ask for it, and write the plan to `panta/<id>.rhei.md` meanwhile — `rhei init` adopts the plans it finds there, while a `.rhei.md` left at the repository root makes it refuse until the file is moved. Init also gitignores `panta/`, so say that the plan will not appear in `git status`; leave that entry alone unless the user wants the plan versioned, which is `rhei init --here` or dropping the host's `panta/` line, and is theirs to decide.

Name a Single-File Plan `<id>.rhei.md` and a Directory Workspace directory `<id>/` — the file stem, or the directory name, becomes the rhei id that prefixes every ticket id in command output (`plan.rhei.md` → tickets `plan.1`, `plan.2`, ...), so choose it like an identifier: it must start with a letter and contain only letters, digits, `_`, or `-`, and `basin` is reserved for the project's unfiled inbox. A bare `.md` extension is not a valid single-file rhei. The Directory Workspace root file is always `index.rhei.md`.

## Adding to a Live Project

Authoring a plan from scratch is one job; adding to a project that already exists is another, and `rhei new` is the command for the second. Do not hand-write a new `.rhei.md` file or splice a task block into an existing one when the CLI is available — `rhei new` writes the file format for you, allocates the id under a lock, checks every flag against the rhei it is writing into, validates the result, and rolls the write back if the create broke anything.

A Rhei project (Panta) is a directory holding `index.panta.md` with its rheis beside it; `rhei init` creates one. Ticket ids are project-qualified: rhei `auth` holds `auth.1`, `auth.2`, and `auth.1.1` under `auth.1`.

```bash
rhei new "Authentication"                          # a new rhei: authentication.rhei.md
rhei new "Authentication" --id auth --dir          # a Directory Workspace rhei instead
rhei new "Rotate signing keys" --under auth        # a ticket in rhei auth -> auth.1
rhei new "Handle expiry" --under auth.1            # a subtask -> auth.1.1
rhei new "Fix the footer typo" --under basin       # unfiled capture, no rhei chosen yet
```

Rules worth knowing before reaching for it:

- **`--under` selects the mode.** With it you create a ticket; without it you create a rhei. Flags belonging to the other mode are a hard error, not a silent no-op.
- **Every metadata field has a flag**, so a ticket never has to be finished by hand: `--state`, `--prior`, `--provides`, `--consumes`, `--kind`, `--model`, `--target`. `--prior` takes either form the plan language allows (`--prior "Task 3"`, `--prior auth.2`), and repeats or takes a comma-separated list.
- **`--consumes` is not a dependency.** A consumer is ready before its producer has run; ordering comes from `--prior`.
- **`--description` / `--description-file`** write the body. `--description-file -` reads standard input, which is how an issue body gets in. Prose only: a heading, a `**Field:**` line, a bare `---`, or an unbalanced ``` fence would author plan structure instead of describing the ticket, and each is refused with the offending line named.
- **`--assignee` is a claim, not a label.** An assigned ticket is skipped by `rhei next` and `rhei run` until `rhei release <id>`, so do not assign work up front unless you mean it.
- **`--dry-run` tells the truth.** It performs the write and the validation and then always rolls back, so a create that would fail fails in the preview too. Use it before a bulk create; add `--json` for a machine-readable object.
- **`--states <name>` requires the machine to exist.** Author it with `rhei-state-machine-writer` first; the rhei points at a machine, never the other way round.
- **Do not create a project or run an orchestrator.** `rhei init` is the human's call, and `rhei run` is always human-initiated.

`rhei new` only ever creates. Re-titling, re-parenting, moving, and deleting remain plan edits, and state changes remain `rhei transition` / `rhei complete`.

## Editing Existing Rhei Plans

When modifying an existing Rhei Plan:

1. Preserve unchanged sections, task IDs, frontmatter, `**Assignee:**` lines, and `> **Result:**` blocks.
2. Append new tasks with `rhei new --under <parent>` where the CLI is available (see *Adding to a Live Project*); hand-append only when it is not, using the existing ID style. Update dependencies transitively when inserting or deleting tasks either way.
3. Do not reset `completed` or `cancelled` tasks unless explicitly requested — the worker treats them as immutable.
4. Keep `## Tasks` as the final section after edits (Single-File Plans), and run `rhei validate` afterward.

## Missing Information Handling

If required input is missing, ask the user to provide it. If the missing information is project-related, the user can instruct you to summon a researcher.

## Important: Task Granularity

Right-sizing tasks balances competing constraints:

- **Too large:** the implementing agent exhausts its context window before finishing.
- **Too small:** task-management overhead (transitions, re-reads, cold context) dominates useful work.
- **Right-sized:** a task fits comfortably in one agent session and produces a meaningful, reviewable unit of change. Child tasks should decompose work the agent can reuse context for — shared files, related functions, sequential build steps.

The state machine defines what happens at each stage of a task's lifecycle — read it before deciding granularity. Heavyweight review gates (multi-agent review, human sign-off, multi-team handoffs) justify larger tasks to amortize that overhead; a lightweight machine (implement → done) allows smaller, more focused tasks. When a task is simple enough that child tasks would just be a checklist, omit them and use inline TODO lists in the description instead.
