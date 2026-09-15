---
name: rhei-template-writer
description: Create or edit Rhei Templates — parameterized, reusable bundles of a plan skeleton, optional state machine, optional settings, and a typed input manifest that can be instantiated with `rhei instantiate` to produce ready-to-execute workspaces. Use when users want to capture, fix, extend, or validate a recurring workflow (review loops, release checklists, onboarding, audits) that is instantiated later with concrete inputs.
---

# Rhei Template Writer

Create or edit a Rhei Template directory — a manifest plus a plan skeleton (and, when needed, a state machine and settings) that `rhei instantiate` renders into a concrete, executable workspace. The template writer runs before `rhei instantiate`: it packages a proven workflow; `rhei instantiate` materializes it with user-supplied inputs. It does not replace the plan writer or state machine writer — it composes their outputs into something reusable.

For anything beyond a linear checklist — counted loops, multi-agent fan-out and aggregation, parallel tasks, git-worktree isolation, or a coordinator that creates follow-up tasks at run time — do not design from scratch. Start from the [Pattern Library](#pattern-library--canonical-examples): it maps each pattern to a checked-in, `rhei validate`-passing reference template to read and adapt.

## When To Use This Skill

Use this skill when a workflow is instantiated more than once with the same shape but different inputs, including creation, maintenance, and repair of an existing template:

- A code-review loop parameterized by target directory and pass count.
- A release checklist parameterized by version, release type, and environments.
- An onboarding workflow parameterized by new-hire name and team.
- A compliance audit parameterized by scope and reviewers.
- Any "scaffold a workspace" request where plans and state machines would otherwise be copy-pasted.
- Fixing or extending an existing `.agent-grounds/rhei/templates/<name>/` or `~/.agent-grounds/rhei/templates/<name>/` template, especially when an instantiated workspace fails `rhei validate` or `rhei run --dry-run`.

Do **not** use it when: the user needs a single plan (use `rhei-plan-writer`); the user needs only a state machine (use `rhei-state-machine-writer`); the user just wants an empty rhei or one more ticket in a project that already exists (that is `rhei new "<title>"` and `rhei new "<title>" --under <parent>` — a blank rhei should not require a template system to exist); or the workflow varies so much that parameterization costs more than it saves.

## Required Inputs

Gather all three before designing a new template. For edits, derive them first from the existing `template.yaml`, README, skeleton files, examples, and validation failure; ask only when the requested behavior cannot be inferred safely.

1. **Workflow intent** — What workflow is captured (one sentence)? Which parts change per instantiation, which stay constant? Single-file (one plan) or multi-file (directory workspace)?
2. **Parameter surface** — Which values must the user supply? Which have sensible defaults? What are the types (`string`, `number`, `boolean`, `path`, `array`, `object`)? Any validation regexes (semver strings, ticket-id patterns)?
3. **State machine scope** — Does the workflow fit the built-in `rhei` machine (bundle no `states.yaml`), need a custom machine (author via `rhei-state-machine-writer` and bundle it), or need MCP servers / skills / agents declared in `settings.json` (bundle one)?

## Output Contract

A template is a directory. Required layout:

```
<name>/
├── template.yaml          # Manifest (required)
├── README.md              # Template documentation (required — see Required Accompaniments)
├── states.yaml            # State machine (optional — omit to use the built-in `rhei` default)
├── settings.json          # Template-shipped project settings (optional)
├── plan.rhei.md           # Single-file plan skeleton  — OR —
├── index.rhei.md          # Directory-workspace index skeleton
├── tasks/                 # Directory-workspace task skeletons (only with index.rhei.md)
│   ├── 01-step.md
│   └── ...
└── ...                    # Additional files: text rendered, binary copied
```

Rules:

- The directory name is the template identifier and must match `manifest.name` exactly.
- Exactly one plan entry point: `plan.rhei.md` **or** `index.rhei.md`. Having both is an error.
- `template.yaml` is parsed before rendering, is never templated itself, and is excluded from the output. Hidden files/directories (names beginning with `.`) are also excluded.
- A root-level `settings.json` is moved to `.agent-grounds/rhei/settings.json` in the output — do not write the `.agent-grounds/rhei/` path in the template source.
- Binary files (null bytes in the first 8 KiB) are copied verbatim. Text files are rendered through the instantiation environment and must decode as UTF-8.

When a custom state machine is needed, `states.yaml` is a required generated artifact (not design notes): produce the complete YAML in the template directory, and keep the rendered plan's `**States:** <name>` aligned with `states.yaml:name`. See *Required Accompaniments* for its diagram and *Response Discipline* for how to present it.

## Required Accompaniments

Every template ships three pieces of context alongside the skeleton. Omit any and the template is incomplete.

### 1. `README.md` at the template root

One file at `<template>/README.md` describing: what the template does (one paragraph); a table of inputs (name, type, default, description); a short per-task-kind summary of how each task walks the state machine (a table is usually enough); the narrative flow in numbered steps (what the coordinator does, what the fan-out looks like, where human gates live); the canonical `rhei instantiate` invocation with representative `--set` arguments; and a link to the checked-in example.

Do not inline the state-machine diagram here — link to `states.yaml`, where the diagram lives (below). The README is rendered through the instantiation environment like any text file; keep `{{...}}` out of it unless you want per-instantiation copies to diverge — generally it documents the template, not the rendered output.

### 2. State-machine diagram as a comment block in `states.yaml`

When the template bundles a `states.yaml`, add an ASCII diagram as a YAML comment block at the very top of the file, before `name:`. It must cover: every non-terminal state and the transitions between them; which state is `initial`; which states are `final`; gating states; any `all_models` / `all_targets` fan-out points, named explicitly; and a short list of per-task paths through the machine (`coordinator: split → completed`, etc.). This lives in the YAML, not just the README, so anyone reading the state machine sees the picture without a context switch. If the template uses the built-in `rhei` machine (no `states.yaml`), skip this.

### 3. A pre-rendered example that passes `rhei validate`

Check in one pre-rendered example so reviewers and users see a working instantiation without running `rhei instantiate` themselves — it is the template's smoke test.

- Place it under `examples/<template-name>-example/` at the repo root (match neighbouring templates).
- Generate it with `rhei instantiate <template> --set ... --output examples/<template-name>-example`.
- For any non-scalar (`array` / `object`) input, check a values file into the example directory (`instantiation-values.yaml`) and regenerate with `--values` so the input shape is reproducible — the established convention (`spec-implementation-example`, `product-management-example`, `parallel-worktrees-example` all do it).
- Overwrite the rendered `README.md` with an example-specific one recording the values used, the validate command, and the regenerate command. The template's README stays documentation; the example's README is an instantiation log.
- Most examples are validated directly with `rhei validate <path>`; only register one in `xtask`'s `EXAMPLES` list if you also want it in `cargo xtask examples validate --all` / the viz dashboard (most `*-example` directories are not registered).
- The example must pass `rhei validate examples/<template-name>-example` as shipped, and must be regenerated whenever the template changes state shape, inputs, or default seed-file rendering.

Pick inputs that exercise every non-trivial code path — e.g. if there's a `focus_areas` input with an empty-list branch, set it to a non-empty list. If one example can't cover every branch, pick the most interesting combination and note the trade-off in the example's README.

## Manifest Contract (`template.yaml`)

```yaml
name: <identifier>             # Must match the enclosing directory name
version: <yaml-scalar>         # Informational; displayed by `rhei templates`
description: <string>          # One-line human-readable summary

inputs:
  - name: <identifier>         # Variable name, referenced as {{name}} in skeletons
    description: <string>      # What this input controls
    type: <string|number|boolean|path|array|object>  # Default: string
    required: <boolean>        # Default: true (unless `default` is present)
    default: <value>           # Mutually exclusive with required: true
    validate: <regex>          # Rust regex applied to the rendered scalar value
    positional: <integer>      # Optional 1-based CLI positional slot
    items: { type: <...> }     # Required when type: array
    properties:                # Optional when type: object
      <property-name>: { type: <...>, required: <boolean>, default: <value> }
```

Rules the writer enforces at author time:

- `name` matches the directory name; `description` is non-empty after trimming; input names are unique.
- `required: true` and `default:` are mutually exclusive — a `default` implicitly makes the input optional.
- `validate` is only valid on scalar types (`string`, `number`, `boolean`, `path`). The pattern is anchored to the **whole** rendered value (`\A(?:…)\z`) and enforced on every matching scalar, including those nested in object `properties` and array `items`; a violation fails instantiation with a path-qualified error (e.g. `input 'targets[0].id' does not match validation pattern '…'`).
- `positional`, when present, is a unique positive integer contiguous from `1`. It lets the user pass that input as a bare CLI argument (`rhei instantiate <t> <value>`) instead of `name=value`.
- `type: array` entries must declare `items`. `type: object` properties are required by default unless they declare `required: false` or a `default`.
- Optional inputs with no `default` resolve to type-shaped empty values (`""` for `string`/`path`, `null` for `number`/`boolean`, `[]` for `array`, `{}` for `object`). Author the template to tolerate the empty value, or declare a `default`.
- `type: path` values are resolved to an **absolute** path (relative inputs join the instantiating cwd), and a non-default `path` is **checked for existence** — a missing path fails instantiation. A `default` path is used as-is and not existence-checked. Consequence: an absolute path is baked into the rendered output, so a `path` input makes the checked-in example machine-specific — regenerate it locally (as `examples/spec-review-example` and `examples/hourly-human-intervention-example` do). To skip absolutization and the existence check, use `type: string`.

## Instantiation Template Syntax

The instantiator uses a restricted MiniJinja environment. **Instantiation templates are distinct from Rhei runtime variables** — both can coexist in one file:

| Form | Resolved by | Example | When |
|---|---|---|---|
| `{{ name }}` | `rhei instantiate` | `{{target}}` | At instantiation time |
| `{% ... %}` | `rhei instantiate` | `{% for t in targets %}...{% endfor %}` | At instantiation time |
| `{name}` | `rhei next` / `rhei run` | `{task_id}`, `{visit_count}`, `{output.review-notes.path}` | At runtime |

Supported MiniJinja constructs in v1: `{{ expr }}` interpolation; `{% for item in items %}`; `{% if cond %}` / `{% else %}` / `{% endif %}`; `{% raw %}` / `{% endraw %}` (to emit literal `{{` / `{%`); the `|slug` filter for filesystem-safe slugs; `range(stop)` / `range(start, stop)`; arithmetic and comparison inside expressions; and `~` for string concatenation. That list is **closed** — text that is not one of those constructs is emitted verbatim, so in particular there is **no comment syntax**: `{# ... #}` is ordinary text, needs no escaping, and a bundled shell script's `${#ARR[@]}` passes through as written. Rendering a file that contains `{#` prints one warning naming the file and line, because earlier versions of rhei cut everything up to the next `#}`.

`{% for k in range(1, rounds + 1) %}` is how a *counted* structure is unrolled into one task per round. Reach for it when each round needs its own identity — its own `**Prior:**`, `**Provides:**`, `**Consumes:**`, or title — because those are per-task metadata and a `visits:` loop repeats one task. When the rounds need no identity, the counted loop is smaller and better. The same arithmetic sizes a budget from the inputs that shaped the plan (`visits: {{ 3 * review_rounds + 3 }}`). See the `supervised-delivery` template.

The renderer is strict: referencing an undefined variable or missing object property is an error (typos fail immediately); external includes/imports are unavailable; unresolved `{{...}}` in the output is an error. Runtime `{name}` variables **pass through** untouched — they are not errors at instantiation time. To emit a literal `{{...}}`, prefer `{% raw %}{{task_id}}{% endraw %}` (the legacy `\{{` escape also works, consuming the backslash). **Bundled scripts and workflow fragments** are rendered like any other text file and arrive with their permissions, executable bit included; a GitHub Actions `${{ ... }}` *is* a `{{ ... }}` interpolation and is still read as one, so a fragment that keeps its own expressions wraps them in `{% raw %}` / `{% endraw %}`.

## Design Rules

### Manifest

1. **Name inputs for the thing, not the form.** Prefer `target`, `review_passes`, `release_version` over `input_path`, `count`, `string1`.
2. **Prefer `default:` over required** when a sensible default exists — the user can always override with `--set`.
3. **Use `validate` for format-shaped values** (semver strings, slug identifiers, date fragments), not open-ended free text.
4. **Keep the input surface small.** Every input is cost; if two knobs are always flipped together, fold them.
5. **Document the effect in `description`,** not the type.

### Plan skeleton

1. **Treat the skeleton like a plan authored by `rhei-plan-writer`** — it must pass `rhei validate` after rendering, following the plan writer's contract (one H1 `# Rhei: <title>`, optional `**States:**`, `## Tasks` last for single-file, tasks with `**State:**` first and `**Prior:**` second). `**Prior:**` is not free text: each comma-separated element must render as `<Kind> <id>` (normally `Task <id>`), and every referenced id must exist in the same rendered workspace. Do not template partial prior fragments that can leave empty items, missing `Task` prefixes, unresolved placeholders, dangling ids, or dangling commas. A generated child task must never list its parent or any ancestor as `**Prior:**`; generate follow-up work as top-level sibling tasks when it must wait for the parent.
2. **Never author `**Assignee:**` or `> **Result:**`** — these are runtime-owned.
3. **Use `{{...}}` only where the plan actually depends on input.** Interpolating `{{target}}` into every task title is noisier than interpolating only where the value matters.
4. **Use `{% for %}` to fan out tasks only when user input controls multiplicity** (one task per reviewer or environment). Keep generated IDs stable and unique.
5. **Keep runtime variables runtime** — `{task_id}` must remain literal in the output for `rhei next` to resolve per task.
6. **Make blind rounds blind in metadata, not prose or timing.** Add `**Excludes:**` after `**Consumes:**` for every participant that must not see another participant's file, directory, or export. Use `checkout=<path>` for checkout sources, `artifact=<path>/` for an execution-root directory, and `<task-id>:<export>` for a declared export. Never exclude a consumed export, the task or machine source, or a required input/handoff. Current built-in profiles filter composed context only; bundle a custom `settings.json` profile with `deny_read.path_flag` only when its wrapper really denies every repeated absolute path for the full process tree. [§FS-rhei-templates.5](../../../../docs/functional-spec/rhei-templates.spec.md#5-authoring-guidance)

### State machine (optional)

1. **Omit `states.yaml` when the built-in `rhei` machine fits** — the default, and it keeps the template small and auto-pickable.
2. **Bundle a custom `states.yaml`** for non-default states, artifact contracts, visit loops, program states, model fan-out, or team gates. Follow `rhei-state-machine-writer` and produce the complete machine (see *Output Contract* and *Required Accompaniments*); do not stop at a prose summary.
3. **If the rendered plan declares `**States:** <name>`, the bundled `states.yaml`'s `name` must match** — auto-discovery keys off it.
4. **Use `{{...}}` inside `states.yaml` only where the workflow needs parameterized control** (`visits: {{review_passes}}`, `model: {{model}}`), keeping `{task_id}` literal and `{{model}}` instantiation-time.
5. **Do not validate or reason from raw templated `states.yaml` as if it were final YAML** while it still contains placeholders — instantiate first, then inspect or validate the rendered `states.yaml`.
6. **For task-level parallelism, set `concurrent: true` on the working states and ship a directory workspace.** `rhei run --parallel N` runs up to N ready tasks at once, but only schedules multiple tasks in the *same* state together when that state declares `concurrent: true` (default `false`); otherwise it defers them one-per-pass and the fan-out serializes. `--parallel` is also ignored on single-file `plan.rhei.md` plans. Real parallelism needs **both** independent ready tasks (sibling tasks with no shared `**Prior:**`, e.g. one per array entry via `{% for %}` in the `tasks/` file) **and** `concurrent: true` states. This is orthogonal to `all_targets` / `all_models`, which fan a *single* task's state across multiple targets inside one task. See the `parallel-worktrees` template.

### Authoring verification

1. **Instantiate before validating** — the authoritative artifact is the rendered workspace, not the placeholder-bearing source.
2. **Run `rhei validate` on the instantiated workspace, not only `--dry-run`.** Dry-run catches rendering errors; validation catches plan, state-machine, settings, artifact, and link errors in the concrete output.
3. **Run `rhei run <workspace> --dry-run` before returning a runnable template** — it checks the orchestrator-facing shape without spawning agents, callbacks, or programs.
4. **Exercise non-scalar inputs through `--values`** (a YAML/JSON file). `--set` and positional inputs are scalar strings and do not test typed structures.
5. **Keep the values file used for the example, or document it in the example README,** so reviewers can reproduce the exact non-scalar input shape.

### Settings (optional)

1. **Only bundle `settings.json` when the template references MCP servers, skills, or agent profiles** not guaranteed in the user's global config.
2. **Use `{{...}}` for workspace-specific values** (workspace ids, hostnames, paths), and the settings file's standard `${VAR}` expansion for secrets — `${VAR}` resolves at `rhei run` time on the user's machine, not at instantiation.
3. **Every MCP / skill / agent id referenced by the bundled `states.yaml` must be declared here or in the user's global settings** — `rhei validate` (post-instantiation) surfaces dangling references.
4. **Write `settings.json` at the template root**; `rhei instantiate` moves it to `.agent-grounds/rhei/settings.json`.

### Additional files

1. **Bundle scripts and runbooks the state machine references from callbacks** (`on_leave`, `on_enter`, or `program` states).
2. **Binary assets are copied verbatim** (images, fonts, compiled artifacts pass through without rendering).
3. **Avoid bundling anything the user can reasonably supply** — smaller templates are easier to audit.

## Workflow

1. Determine whether this is a new template or an edit to an existing template.
2. For edits, read the existing `template.yaml`, README, plan skeleton, optional `states.yaml` / `settings.json`, checked-in example, and any reported validation error before changing files. Preserve the template's public input contract unless the user requested a breaking change.
3. Confirm workflow, parameters, and state-machine scope with the user when they are not clear from the existing template or request.
4. Pick single-file (`plan.rhei.md`) or directory workspace (`index.rhei.md` + `tasks/`). Prefer single-file unless per-file concurrency matters; do not change an existing template's shape without a concrete reason.
5. Draft or update `template.yaml` with the minimum required inputs.
6. Draft or update the plan skeleton — interpolate `{{...}}` only where input shapes the output; keep runtime `{...}` variables where they belong.
7. Decide whether to bundle or update `states.yaml`. If yes, apply `rhei-state-machine-writer` for the full machine, wire in `{{...}}` where needed, and add or maintain the diagram comment block.
8. Decide whether to bundle or update `settings.json`. If yes, declare MCP servers, skills, and `defaults` that match the state machine.
9. Place the template in a discoverable directory (see *File Placement*).
10. Smoke-render with `rhei instantiate <template> --dry-run --set ...`, using `--values <file>` for array/object inputs; fix rendering errors.
11. Write or update `README.md` at the template root.
12. Generate or regenerate the pre-rendered example into `examples/<template-name>-example/` and overwrite its README with the current values and commands.
13. Inspect the rendered example's task metadata, especially every `**Prior:**` line, and fix the template if any rendered prior item is not an existing `<Kind> <id>` reference.
14. Validate the example: `rhei validate examples/<template-name>-example/`. A validation failure means the template is not done; do not return it with a known failing example.
15. Run `rhei run examples/<template-name>-example/ --dry-run` and fix execution-shape errors.
16. Repeat instantiate + validate + run-dry-run for at least two other input combinations to catch branches the example doesn't cover. For small edits with no branch/input-surface change, at minimum re-run the existing canonical example plus the user-reported failing instantiation.

## Response Discipline

When returning a template in chat instead of editing files directly, print a file-by-file artifact list — every required file as a fenced block with its path. If a custom state machine is needed, one block must be `<template>/states.yaml` containing the full YAML, including the top diagram comment; do not describe the machine only in prose or defer it to a later step (unless the user explicitly asks for an outline). If no custom machine is needed, say explicitly that the template uses the built-in `rhei` machine and therefore omits `states.yaml`.

## Validation Checklist

Before returning the template, verify:

- Directory name matches `manifest.name`; `template.yaml` declares `name`, `version`, `description`, and optionally `inputs`.
- Every input has a unique `name` and non-empty `description`; none mixes `required: true` with a `default`.
- `type: array` inputs declare `items`; `validate` appears only on scalar types.
- Exactly one plan entry point exists and uses the Rhei Plan grammar (`# Rhei: <title>`, `## Tasks` last for single-file, task headings with `**State:**` first); no `**Assignee:**` or `> **Result:**` is authored.
- Every rendered `**Prior:**` line contains only comma-separated `<Kind> <id>` elements (normally `Task <id>`), with no empty elements, unresolved template placeholders, duplicate/missing prefixes, or trailing commas; each referenced id exists in the rendered single-file plan or directory workspace.
- Every blind participant carries the necessary `**Excludes:**` entries after `**Consumes:**`; each path stays within its declared root, each export resolves to a declared `**Provides:**`, and no exclusion overlaps consumed or required input.
- Every `{{...}}` variable is declared in `manifest.inputs` (or is a nested property on an object input); runtime `{name}` variables passing through are valid against the active machine's namespace (`{task_id}`, `{task_title}`, `{visit_count}`, `{visits}`, `{model}`, `{input.<name>.path}`, `{output.<name>.path}`, `{meta.<key>}`).
- If `states.yaml` is bundled: the rendered plan's `**States:** <name>` matches its `name`; it begins with the ASCII diagram comment block; it passes the state-machine-writer checklist; placeholder-bearing source is inspected/validated only via the rendered workspace; and every MCP/skill/agent id it references is declared in a bundled or global `settings.json`. If the workflow needs a custom machine, `states.yaml` exists as a concrete artifact, not prose.
- If the template fans out tasks for parallel execution, `rhei run <workspace> --parallel N --dry-run` schedules multiple tasks in one pass (not "Deferred … to a later pass"). Deferral means missing `concurrent: true` or a shared `**Prior:**`.
- If `settings.json` is bundled, it is valid JSON after rendering.
- `rhei instantiate <template> --dry-run ...` succeeds; every array/object input has been exercised through `--values`; the rendered workspace has been inspected for malformed `**Prior:**` metadata and passes both `rhei validate` and `rhei run --dry-run`.
- `<template>/README.md` exists and covers: summary, inputs table, per-task paths through the state machine, numbered flow, canonical `rhei instantiate` command, and a link to the example.
- A pre-rendered example under `examples/<template-name>-example/` exists, was generated by `rhei instantiate`, has an example-specific `README.md` (inputs used, validate command, regenerate command), and passes `rhei validate <example-path>` as shipped.

## File Placement

Templates are resolved by `rhei instantiate <name>` in this order (first match wins):

| Priority | Location | Scope |
|---|---|---|
| 1 | `<project>/.agent-grounds/rhei/templates/<name>/` | Project-local |
| 2 | `<project>/.agents/rhei/templates/<name>/` | Project-local, deprecated |
| 3 | `~/.agent-grounds/rhei/templates/<name>/` | User-global |
| 4 | `~/.agents/rhei/templates/<name>/` | User-global, deprecated |

`<project>` is the **nearest** level at or above the working directory holding either name, not the repository root, so the table is the order *within* one level once the walk has fixed which level that is; `.agents/rhei/` is the former home, still read, and reading it prints one `warning:` line on stderr naming the `.agent-grounds` path to move the material to. Place project-scoped templates under `.agent-grounds/rhei/templates/` so the team picks them up from the checkout; personal templates under `~/.agent-grounds/rhei/templates/` so they're available across projects. A template can also be instantiated from an arbitrary path (`rhei instantiate ./path/to/template/`) — useful for authoring and review.

## Instantiation CLI Surface

`rhei instantiate <template> [inputs...] [options]`. Inputs are supplied four ways (precedence low → high): manifest `default` < `--values <file>` (YAML/JSON; repeatable) < bare positional values (for inputs declaring `positional`, or the single-required-input fallback) < `KEY=VALUE` and `--set KEY=VALUE` < `--set-file KEY=<path>` (sets the input to a file's contents — for long prose like a brief).

| Flag | Use |
|---|---|
| `--values <file>` | The only sane way to pass `array` / `object` inputs (parsed as YAML/JSON). Always smoke-test structured inputs through this. |
| `--set-file KEY=<path>` | Inject long text (briefs, descriptions) without shell-quoting hell. |
| `--dry-run` | Render + validate into a scratch dir, write nothing. Catches rendering and validation errors. |
| `--output <path>` | Defaults to `<project>/<template-name>/` inside a Panta project, else `./<template-name>/` ([§FS-rhei-templates.6.2](../../../../docs/functional-spec/rhei-templates.spec.md#62-instantiating-inside-a-panta-project)). Must **not** already exist (except under `--dry-run`); instantiation refuses to merge/overwrite. |
| `--keep-on-error` | Keep the output dir when post-instantiation validation fails, so you can inspect the broken render. First move when an example won't validate. |
| `--list-inputs` | Print the resolved input schema and exit — quick way to confirm the manifest parses. |
| `--execute` | Instantiate then immediately `rhei run` (mutually exclusive with `--dry-run`). |

## Pattern Library — Canonical Examples

When a workflow is more than a linear checklist, start from a proven template. Each entry is a checked-in, `rhei validate`-passing reference — read its `states.yaml` (diagram in the top comment), its `tasks/`, and its `README.md`, then adapt. Paths are repo-relative.

**Counted loops** (`review → fix → review …`)
- Template `crates/rhei-cli/templates/spec-review/`; examples `examples/spec-review-example/` and the callback-driven `examples/review-fix-visits/`.
- Technique: a state pair both declaring `visits: N`, with a transition gated `condition: visitCount < visits` (loop back) vs `visitCount >= visits` (exit). The smallest loop in the repo.

**Multi-agent review + aggregation** (fan out across reviewers, then merge)
- Template `crates/rhei-cli/templates/changeset-review/` (the richest: `split → fan-out review → aggregate-reviews → propose-fixes → aggregate-proposals → decide → human gate → fix`); also `crates/rhei-cli/templates/spec-implementation/`.
- Technique: `all_targets: <array_input>` fans one state across many agents inside a single task; a following aggregator state run by a single `smart_target` merges the per-agent artifacts. Reviewer multiplicity is an input, never hardcoded.

**Multi-target / multi-model fan-out**
- Template `crates/rhei-cli/templates/multi-model-analysis/`; example `examples/multi-model-analysis-example/`.
- Technique: an `analyze` state with `all_targets` over an `agents` object-array, then one synthesis state. Per-target artifact paths slugify a structured field: `path: …/{{ t.selector|slug }}.md` (the `|slug` filter — the only filter available).

**Multi-round discussion / deliberation** (participants take each other's points into account across rounds, then converge or escalate)
- Example `examples/agent-discussion/` (callback-driven; no template yet — a candidate to parameterize: participant list, their stances/goals, and the round budget are the natural inputs).
- Technique: a `collect ↔ judge` loop. `collect` declares `all_models` so the position callback fans out once per participant; every round after the first reads the previous round's judge digest, so positions move instead of repeating. The `judge` callback writes a per-round digest and returns a `nextState` redirect — `converged` (consensus, records `decision.md`), `escalated` (a **gating** human handoff once the round budget is spent), or no redirect to loop back. Participants argue from assigned project goals (distinct stances), so it is a multi-perspective deliberation rather than a one-shot poll, and the converged `decision.md` gates a downstream task via `**Prior:**`. Contrast with *Multi-agent review + aggregation*, which fans out once and merges; this loops and lets participants respond to each other.
- **Gotcha:** the loop is driven by the judge's `nextState` redirect, **not** by `visits`. Do not put `visits` on the `all_models` `collect` state — the engine runs an `all_models`+`visits` state per-target *per-visit* and spins on a `state → state-2` self-loop. Bound the loop in the judge callback (a `CAP`) instead.

**Dynamic / agent-driven task creation** (a coordinator analyzes, then spawns a run-time-decided number of tasks)
- Template `crates/rhei-cli/templates/analyze-and-dispatch/`; example `examples/analyze-and-dispatch-example/`. The same coordinator mechanism also lives inside `spec-implementation/` and `changeset-review/`.
- Technique: the rhei "API" for adding tasks is **writing a conforming `tasks/NN-<slug>.md` file** into a directory workspace — `rhei run` re-parses `tasks/` every pass, so there is no `rhei add` command. A coordinator state's agent decides how many tasks to create (the count is *not* fixed at instantiation), writes one file per work item with `**Prior:** Task {task_id}` — its own runtime-substituted id, copied verbatim, so spawned tasks wait for the coordinator — and optionally a final aggregate task whose `**Prior:**` lists every spawned id. Use when the number of follow-ups depends on what the agent finds; contrast with *Parallel execution* below, where the count is fixed at instantiation by an array input. (`##` headings are reserved in task files, and a multi-line free-text input must not be interpolated into a `states.yaml` block scalar — keep it in the markdown task body.)

**Parallel execution** (many *independent* tasks advancing at once)
- Template `crates/rhei-cli/templates/parallel-worktrees/`; example `examples/parallel-worktrees-example/`.
- Technique: a directory workspace whose `tasks/` file `{% for %}`-fans out sibling tasks with **no `**Prior:**`** between them, and working states marked `concurrent: true`. Both are required (see State machine rule 6). The example README shows the `--parallel N` dry-run beside the sequential one.

**Per-task git worktrees** (isolate concurrent edits)
- Template `crates/rhei-cli/templates/parallel-worktrees/` (clean), and the `prepare-workspace` state of `changeset-review/` (worktree as one branch of a `none|branch|worktree|fork` choice).
- Technique: worktree creation is an **instruction the agent runs** (`git worktree add <root>/{task_id} -b <prefix>/{task_id}`), not a Rhei primitive. Key each worktree + branch on `{task_id}` so parallel agents never collide; write runtime artifacts back to the scratchpad, not inside the worktree. Mix instantiation-time `{{...}}` and runtime `{task_id}` in one path to get a per-task, per-instantiation location.

**Supervised delivery pipeline** (a parent that decides what its subtree does next, one step at a time)
- Template `crates/rhei-cli/templates/supervised-delivery/`; example `examples/supervised-delivery-example/`.
- Technique: the root task's state declares `execute_on: child-terminal`, so the orchestrator wakes it after every finished child and holds the rest of the subtree in between ([§FS-rhei-supervision](../../../../docs/functional-spec/rhei-supervision.spec.md#fs-rhei-supervision-subtree-supervision-specification)). Three things turn that into a working template. (1) **The brief is the release gate**: every child state declares a *required* `inputs:` entry at `runtime/supervise/{task_id}.md`, so a step whose brief the supervisor has not written is never dispatched — without it a released subtree runs whatever the prerequisite graph allows and the supervisor only watches. (2) **The channel is plan exports**: `**Provides:**` / `**Consumes:**` ([§FS-rhei-plan-language.3.12](../../../../docs/functional-spec/rhei-plan-language.spec.md#312-task-exports)) declare which handoffs the engine injects into prompts; they are data-flow, not filesystem visibility, so workers may still read undeclared sibling exports under `runtime/exports/`. Concurrent scheduling and explicit briefs can reduce accidental cross-reading but cannot enforce blindness once an export exists. Declare the same path as the producing state's `outputs:` so the completion condition refuses a step that did not publish it, and put the schema in a `prompt_templates/` fragment several states share. (3) **Rounds are unrolled** with `{% raw %}{% for k in range(1, review_rounds + 1) %}{% endraw %}` because each round needs its own `**Prior:**` and exports.
- **Gotchas, both learned the hard way:** do *not* declare `outputs:` on the supervising state — a satisfied completion condition makes `rhei run` advance a ticket without spawning it, so a file written on visit 1 silently skips every later visit; use an `optional: true` **input** instead and branch on `{if input.<name>.exists}` to tell the first visit from the rest. And chain each later phase to the *first* task of the previous phase, never the last: `cancelled` does not satisfy a prerequisite ([§FS-rhei-states.1.4](../../../../docs/functional-spec/rhei-states.spec.md#14-reserved-state-names)), so the first round the supervisor cancels would strand everything after it.

**Human gates / branch / fork isolation / counted decision loops**
- Templates `crates/rhei-cli/templates/changeset-review/` and `crates/rhei-cli/templates/hourly-human-intervention/`.

## Example Skeleton

A minimal one-input template using the built-in `rhei` machine and a single-file plan:

```yaml
# release-notes/template.yaml
name: release-notes
version: 1.0
description: Draft, review, and publish release notes for a version

inputs:
  - name: version
    description: Semantic version being released (e.g., 1.4.0)
    type: string
    validate: "^\\d+\\.\\d+\\.\\d+$"
  - name: channel
    description: Publication channel
    default: beta
```

```markdown
# release-notes/plan.rhei.md
# Rhei: Release {{version}} notes

## Tasks

### Task draft: Draft notes for {{version}}
**State:** pending

Draft the release notes for `{{version}}` targeting the `{{channel}}` channel.
Include highlights, breaking changes, and migration notes.

### Task review: Review draft
**State:** pending
**Prior:** Task draft

Review the draft for accuracy, tone, and completeness.

### Task publish: Publish to {{channel}}
**State:** pending
**Prior:** Task review

Publish the reviewed notes to the `{{channel}}` channel.
```

```bash
rhei instantiate release-notes --set version=1.4.0 --set channel=stable --output ./releases/1.4.0/
```

## Missing Information Handling

If required input is missing, ask the user for workflow intent, parameter surface, and state-machine scope; do not invent parameters for uncertain parts. If the template would be a thin wrapper over a single plan with no real parameterization, push back — `rhei-plan-writer` is the better tool. If the workflow needs more than roughly ten inputs or more than one state machine, push back: consider splitting into separate templates.
