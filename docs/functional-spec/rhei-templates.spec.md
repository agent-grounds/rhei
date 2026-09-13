# FS-rhei-templates: Rhei Templates Specification

This document specifies **Rhei Templates** — parameterized, reusable plan+state-machine bundles that can be instantiated with concrete inputs to produce ready-to-execute workspaces.

## Motivation

Rhei plans and state machines today are authored from scratch or copied manually. Common patterns — code review loops, release checklists, onboarding workflows — are recreated each time. Templates let users capture a proven workflow once and instantiate it with project-specific inputs, removing boilerplate and enforcing consistency.

## Concepts

A **template** is a directory containing:

1. A **manifest** (`template.yaml`) declaring the template's name, description, and typed input parameters.
2. A **state machine** (`states.yaml`) — optional; when absent, instantiation and validation fall back to the built-in `rhei` default.
3. **Prompt templates** (`prompt_templates/*.md`) — optional; when present next
   to `states.yaml`, the directory declares reusable state prompt fragments
   referenced by state-level `prompt_template` fields.
4. A **plan skeleton** — either a single-file plan (`plan.rhei.md`) or a directory-workspace layout (`index.rhei.md` + `tasks/`).
5. **Additional files** — any non-manifest files bundled with the template. Text files are rendered with a restricted MiniJinja template environment; binary files are copied verbatim into the output.

Materialized template files may contain **instantiation templates** (`{{ ... }}`, `{% ... %}`) that are resolved at instantiation time from user-supplied inputs. These are distinct from the single-brace **runtime variables** (`{name}`) defined by the state-machine and plan specifications. Rendering is ordered: first resolve MiniJinja templates across template files during `rhei instantiate`, then let later `rhei` commands resolve runtime `{...}` variables against the instantiated workspace. The manifest (`template.yaml`) is parsed before rendering and is never itself templated.

State-machine prompt templates are runtime prompt fragments, not instantiation
templates. When a template bundles `prompt_templates/*.md` next to
`states.yaml`, their single-brace placeholders are preserved through
`rhei instantiate`, substituted from each state's `prompt_template.values`, and
any runtime variables inside those values are then resolved during `rhei next`
or `rhei run`. Inline state `personality` and `instructions` remain valid.

## 1. Template Discovery

Templates are resolved per name in order, first match wins:

| Priority | Location | Scope |
|----------|----------|-------|
| 1 | each `<ancestor>/.agent-grounds/rhei/templates/<name>/`, then `<ancestor>/.agents/rhei/templates/<name>/` | Project-local, nearest ancestor outward; deprecated second at each level |
| 2 | `~/.agent-grounds/rhei/templates/<name>/` | User-global |
| 3 | `~/.agents/rhei/templates/<name>/` | User-global, deprecated |
| 4 | compiled into the `rhei` binary | Built-in |

`<ancestor>` ranges from the working directory through every filesystem parent
to the filesystem root. Section 1.2 defines the project-local walk, including
how it keeps the configured user-global roots in their own tier. Every genuine
project ancestor is searched before the user-global and built-in tiers.

The `<name>` is the directory name and serves as the template identifier used in CLI commands.

**Built-in templates** ship inside the binary. They are the tier every install
has: a fresh `cargo install` — or a downloaded release binary — must be able to
run `rhei templates` and `rhei instantiate` without first authoring a template
or cloning a repository. Shipping them as files beside the binary would make the
feature depend on an installation layout that no distribution channel
guarantees, so the library is embedded at compile time instead.

Because built-ins sit last in the search order, a project or user template of
the same name shadows one, which is how a built-in is customized: copy it into
`.agent-grounds/rhei/templates/<name>/` and edit.

A built-in is extracted to a temporary directory when it is instantiated, so
every later step — manifest validation, layout detection, rendering — sees an
ordinary template directory and behaves identically for all three tiers.

Discovery errors are handled per command:

- `rhei templates` skips unreadable or invalid template directories and prints a warning naming the skipped path. `--source` selects a tier: `project`, `user`, `builtin`, or `all`.
- `rhei instantiate <name>` fails if the matched template directory is unreadable or invalid.
- `rhei instantiate <path>` fails if the explicit template path is unreadable or invalid.

### 1.1. Rhei's project-local home, and its deprecated predecessor

Rhei's own project-local material — templates and project settings — lives
under `.agent-grounds/rhei/`. The former home, `.agents/rhei/`, is still read
and is deprecated.

The two names are separate because the directories hold different kinds of
thing. `.agents/` is where agent *instructions* live, and an agent runtime may
mount it read-only inside a checkout — Codex's managed permission profile does.
Rhei's templates and settings are product files an agent working in the
repository has to be able to edit, so they cannot live behind that protection.
`.agents/skills/` is agent instruction, is discovered there by other runtimes,
and does not move.

Both names exist at every tier that has a project-local or user-global root.
The new name is searched first at a user-global tier and first at each
project-local ancestor level. A template present under both names at the same
level resolves to the `.agent-grounds` copy and is listed once. A nearer
project-local copy under the deprecated name still precedes a farther copy
under the current name.

Rhei never *writes* to `.agents/rhei/`. Every path that creates project-local
rhei material — a template's root `settings.json` on instantiation (§4), the
settings hoist into an enclosing project (§6.2) — writes under
`.agent-grounds/rhei/`, so rhei's own output is never a path rhei then warns
about.

### 1.2. The ancestor walk checks both names at each level

The project-local roots are an ordered ancestor sequence, not one selected
level. Discovery walks upward from the working directory through the filesystem
root. At each level it checks `.agent-grounds/rhei/templates` and then
`.agents/rhei/templates` **before ascending to the parent**. No Git repository,
Panta project, rhei home, or other project marker ends this walk.

When the configured user home is itself on that ancestor chain, its exact
`.agent-grounds/rhei/templates` and `.agents/rhei/templates` roots are reserved
for the user-global tier and are not also project-local candidates. This
exclusion removes only those two roots: discovery still examines every parent
above the user home through the filesystem root. It therefore neither turns the
user home into a boundary nor hides a genuine project ancestor above it.

Resolution is per template name. The first directory named `<name>` in that
sequence wins. A settings-only rhei home, an empty templates directory, or a
templates directory containing only other names therefore cannot hide an
ancestor copy of `<name>`. The same order aggregates listings and completion
candidates, with the first copy of each name included once. Named detail,
no-name instantiation, and named instantiation inherit that discovery order.
For a valid winning copy, its source, path, manifest content, and rendered
content must agree across those commands.

Checking both names per level preserves nearest-copy-wins without giving the
current name global precedence. A child copy under `.agents/rhei/templates`
therefore beats a parent copy of the same name under
`.agent-grounds/rhei/templates`; at one level, the `.agent-grounds` copy still
beats the `.agents` copy. Every project-local ancestor remains ahead of both
user-global homes and the built-in fallback.

Except for the two user-global roots reserved above, each level holding either
templates directory contributes both candidate paths in current-then-deprecated
order, whether or not both directories exist, so a searched-path display never
advertises the deprecated home alone. When no genuine project ancestor holds
either templates directory, the project root supplies the same
current/deprecated fallback pair for the `Searched:` diagnostic. The fallback
cannot reintroduce an excluded user-global root as project-local, and it does
not cut off or otherwise redefine the ancestor walk.

### 1.3. The deprecation warning

When rhei reads project-local material from a `.agents/rhei/` path, it prints a
warning naming **both** the path it read and the `.agent-grounds` path to move
that material to. A warning that only says "deprecated" sends the reader
hunting for what to do about it.

The warning's contract:

- It is written to **stderr**, never stdout, so `--json` output stays parseable.
- It is a `warning:` line that says the path it read is deprecated, and names
  the `.agent-grounds` path in the same message.
- It fires **once per distinct deprecated path per process**, not once per
  lookup. Templates and settings are resolved repeatedly inside one `rhei run`,
  and a per-lookup warning would bury the run's own output.
- It fires only when a deprecated path is the one actually read, so rhei is
  silent when only `.agent-grounds/rhei/` is present. Where both names exist at
  a tier, what is shadowed decides: a project settings file resolves to exactly
  one file, so the deprecated one is never read and never warned about, while
  discovery reads a *directory* and reports per template — a template found
  only under the deprecated name warns even though the `.agent-grounds`
  directory beside it exists, and a deprecated directory whose every template
  is shadowed says nothing.
- It is suppressed while the process is serving a shell completion request.
  Every completion is a fresh process, so the once-per-process guard cannot
  hold there: without this, a project on the deprecated home prints the whole
  paragraph over the candidate list on every Tab press. A run is serving one
  only when `COMPLETE` names a shell: the values that turn dynamic completion
  *off* — unset, empty, and `0` — are ordinary runs and warn like any other.

## 2. Directory Layout

```
<name>/
├── template.yaml          # Manifest (required)
├── states.yaml            # State machine (optional)
├── prompt_templates/      # Reusable state prompt fragments (optional; requires states.yaml)
│   └── review.md
├── settings.json          # Template-shipped project settings (optional)
├── plan.rhei.md           # Single-file plan skeleton
│   ── OR ──
├── index.rhei.md          # Directory-workspace index skeleton
├── tasks/                 # Directory-workspace task skeletons
│   ├── 01-step.md
│   └── ...
└── ...                    # Additional files (text rendered; binary copied)
```

A template must contain exactly one plan entry point: either `plan.rhei.md` (single-file) or `index.rhei.md` (directory workspace). Containing both is an error. For single-file templates the entry-point filename also determines the ticket-id prefix of every instantiated workspace: the file stem is the rhei id, so `plan.rhei.md` yields tickets `plan.1`, `plan.2`, ..., regardless of the `--output` directory name.

**A template's task ids must not repeat the template name.** Every ticket id is
already qualified by its rhei ([§FS-rhei-panta.5](rhei-panta.spec.md#5-identity)), and for an instantiated
template the rhei id is the output directory — the template's own name by
default. A task authored as `### Task spec-review:` inside the `spec-review`
template therefore reads as `spec-review.spec-review` in every listing, error,
and artifact path (`runtime/reviews/task-spec-review.spec-review-review-1.md`).
Name the task for the step it performs — `review`, `loop`, `coordinate` — and
let the prefix say which rhei it belongs to.

## 3. Manifest Schema (`template.yaml`)

```yaml
name: <identifier>             # Template identifier (must match directory name)
version: <yaml-scalar>         # Template version (informational; displayed by `rhei templates`;
                               #   no semantic-version validation in v1)
description: <string>          # One-line human-readable summary

inputs:
  - name: <identifier>        # Variable name, referenced as {{name}} in skeletons
    description: <string>      # What this input controls
    type: <string|number|boolean|path|array|object>  # Value type (default: string)
    required: <boolean>        # Whether the input must be supplied (default: true)
    default: <value>           # Default value (makes the input optional; mutually
                               #   exclusive with required: true)
    positional: <integer>       # Optional 1-based positional CLI slot for short
                               #   `rhei instantiate <template> <value>` input
    validate: <regex>          # Optional regex the value must match
    format: <execution-target>  # Optional named format checked at instantiation time
    items:                     # Required for `type: array`
      type: <...>
      ...
    properties:                # Optional for `type: object`
      <property-name>:
        type: <...>
        required: <boolean>
        default: <value>
        ...
```

### 3.1. Validation Rules

- `name` must match the enclosing directory name.
- `inputs[].name` values must be unique within the manifest.
- `version` is informational metadata. In v1 it may be any YAML scalar and is not semantically validated.
- An input with `required: true` (the default) must not declare a `default`.
- An input with a `default` is implicitly `required: false`.
- An input with `required: false` and no `default` resolves to the empty string when not supplied by the user. Templates are pure text substitution in v1, so authors should tolerate that empty string directly or prefer declaring a `default`.
- `positional`, when present, must be a positive integer. Positional indexes must be unique within a manifest and contiguous starting at `1`.
- `type: array` inputs must declare `items`.
- `type: object` inputs may declare `properties`. Properties are required by default unless they declare either `required: false` or a `default`.
- `validate` is only valid on scalar input types (`string`, `number`, `boolean`, `path`).
- Optional inputs with no `default` resolve to type-shaped empty values:
  - `string`, `path` → `""`
  - `number`, `boolean` → `null`
  - `array` → `[]`
  - `object` → `{}`
- For `type: array` and `type: object`, positional values, `KEY=VALUE`, `--set KEY=...`, and `--set-file KEY=...` values are parsed as YAML/JSON snippets before validation.
- `type: path` values are rendered exactly as supplied by the user or manifest `default`; instantiation does not rewrite them to absolute paths. Relative `path` values are interpreted relative to the instantiating process `cwd` only when the CLI itself must resolve that path for its own file operations. The exception is an omitted optional `path` input with no `default`, which resolves to the empty string.
- `validate`, when present, is a Rust `regex`-crate pattern applied to the string representation of the resolved scalar value and anchored to the entire rendered value. It is enforced on every scalar it is declared on, including scalars nested inside `object` `properties` and `array` `items`; a failing match aborts instantiation with a path-qualified error (for example, `input 'agents[0].id' does not match validation pattern '…'`).
- `format`, when present, names a built-in value check applied at instantiation time, before any file is rendered. The only format in v1 is `execution-target`, which parses the value as an execution target selector ([§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification)) and reports a malformed value against the input the user supplied rather than against the rendered state machine ([§FS-rhei-errors.3.1](rhei-errors.spec.md#31-execution-target-inputs)). Like `validate`, it is only valid on scalar input types and is enforced on nested `properties` and `items` scalars. `format` and `validate` may be combined; both must pass.

## 4. Template-Shipped Settings

A template may bundle a `settings.json` file alongside `template.yaml` to
declare project-scoped settings that the instantiated workspace should start
with — most commonly, the MCP server and skill profiles the state machine
references.

The file uses the standard project settings schema documented in
[Agents Specification — Global and Project Settings](rhei-agents.spec.md#11-global-and-project-settings).
It is treated as a text template file: instantiation variables (`{{name}}`)
are resolved at instantiation time so a template can parameterize
workspace-specific values (workspace ids, paths, hostnames) without exposing
host secrets.

On instantiation the rendered file is written to `.agent-grounds/rhei/settings.json`
in the output tree, where `rhei run` and `rhei validate` automatically pick it
up and compose it over the user's global `~/.config/rhei/settings.json`.

When the output joins a Panta project the file is **hoisted to the project**
instead (§6.2): settings resolve once, at the root the plan loads from, and for
a member rhei that root is the project, not the workspace. Left in the
workspace, a template's own agent registry was silently unread, and the
template's own default targets then failed validation as unknown agents.

Example:

```json
// settings.json inside the template
{
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "{{linear_workspace_id}}" }
    }
  },
  "skills": {
    "review-checklist": { "path": ".rhei/skills/review-checklist" }
  },
  "defaults": {
    "mcp_servers": ["linear"]
  }
}
```

With the matching manifest input:

```yaml
# template.yaml
inputs:
  - name: linear_workspace_id
    description: Linear workspace id used by the Linear MCP server
    type: string
    required: true
```

Rules:

- `settings.json` must be a UTF-8 JSON file. Instantiation fails if parsing
  the rendered file fails.
- References to host secrets use the settings file's standard `${VAR}`
  expansion — **not** template variables — so secrets are resolved at
  `rhei run` time on the user's machine, not baked into the output.
- A template that references MCP or skill ids in its `states.yaml` must
  declare matching registry entries either in its bundled `settings.json`
  or expect the user to provide them in their global settings. `rhei
  validate` (step 7 below) surfaces any remaining dangling references as
  errors.
- Users may edit `.agent-grounds/rhei/settings.json` after instantiation to replace
  template-declared entries, add project-specific overrides, or clear the
  `defaults` lists.

## 5. Instantiation Template Syntax

Instantiation rendering uses a restricted [MiniJinja](https://github.com/mitsuhiko/minijinja)-style environment. This is intentionally distinct from the single-brace runtime variables (`{task_id}`, `{visit_count}`, etc.) that later `rhei` commands resolve at execution time.

Supported constructs in v1:

- `{{ expr }}` for interpolation
- `{% for item in items %}` ... `{% endfor %}` for loops
- `{% if cond %}` ... `{% else %}` ... `{% endif %}` for conditionals
- `{% raw %}` ... `{% endraw %}` for literal template syntax
- `|slug` filter for filesystem-safe slugs
- `range(stop)` and `range(start, stop)` for a numeric sequence, half-open at
  the top like Python's
- arithmetic (`+`, `-`, `*`, `/`, `//`, `%`) and comparison (`<`, `<=`, `==`,
  `!=`, `>=`, `>`) inside any expression
- `~` for string concatenation

That list is **closed**. Text that is not one of those constructs is not
instantiation syntax, and is emitted verbatim however a general-purpose
template engine would read it. In particular, **instantiation templates have no
comment syntax**: `{# ... #}` is ordinary text. A bundled shell script's
`${#ARR[@]}`, a Makefile, or any other file holding `{#` passes through
unchanged, never aborts instantiation, and never loses the text that follows
it. Because earlier versions read `{#` as a comment opener and cut everything up
to the next `#}`, rendering a file that contains `{#` also warns (§5.3), so a
template authored against that behavior is told its output has changed rather
than changing silently.

`range()` with arithmetic is what unrolls a *counted* structure at
instantiation time — one task per round of a loop the plan has to name
individually, because `**Prior:**`, `**Provides:**`, and `**Consumes:**` are
per-task metadata and cannot be written once for a loop the engine repeats:

```jinja
{% for k in range(1, review_rounds + 1) %}
#### Task deliver.review-{{k}}: Code review round {{k}}
**State:** review
**Prior:** Task deliver.{% if k == 1 %}implement{% else %}fix-{{ k - 1 }}{% endif %}
{% endfor %}
```

The same arithmetic sizes a state's own budget from the inputs that shaped the
plan — `visits: {{ 3 * review_rounds + docs_rounds + 3 }}`. Contrast this with
a counted loop (`visits:` plus a `visitCount`-conditioned transition), which
repeats *one* task's state and is the right shape when the rounds need no
per-round identity. Unrolling is for when they do.

The renderer is **strict**:

- Referencing an undefined variable or missing object property is an error.
- `template.yaml` is parsed before rendering and is never templated.
- The environment does not load external templates; includes/imports are unavailable.

### 5.1. Resolution Rules

- Instantiation templates are resolved **at instantiation time**, producing a concrete plan with no remaining `{{...}}` / `{% ... %}` markers.
- Runtime variables (`{...}`) pass through instantiation untouched — they remain in the output for `rhei next` to resolve later.
- An unresolved `{{name}}` or missing `foo.bar` property is an **error** (fail-closed), unlike runtime variables which fail-open. This catches typos early.
- Instantiation always happens before any runtime `{...}` substitution from bundled or default state machines.
- Instantiation variables can appear in any **text** file that is materialized into the output tree. `template.yaml` is parsed before rendering, is excluded from output, and must not rely on `{{...}}` substitution.
- A file is considered text if it contains no null bytes in its first 8 KiB; all other files are binary.
- Text template files must decode as UTF-8. If a file is classified as text but cannot be decoded as UTF-8, instantiation fails with a file-read error.
- Binary files (images, compiled artifacts) are copied without instantiation-variable resolution or UTF-8 decoding.

### 5.2. Escaping

To emit a literal `{{` or `{%`, prefer MiniJinja raw blocks:

```jinja
{% raw %}{{target.slug}}{% endraw %}
```

For backward compatibility, `\{{` also emits a literal `{{` and the backslash is consumed during instantiation.

`{#` needs no escape, because it is not syntax (§5): it is emitted as written
whether or not a `#}` follows it anywhere in the file. Wrapping it in a raw
block is harmless and remains supported, so a template that already fences a
shell body in `{% raw %}` ... `{% endraw %}` keeps rendering exactly as it does
today.

### 5.3. Render Diagnostics

Every diagnostic instantiation prints about a template's text names what is in
that text. Help is derived from the failure that actually occurred, never
assigned by the part of the renderer the failure passed through — the general
rule is [§FS-rhei-errors.6](rhei-errors.spec.md#6-coverage), and this section is
what it requires of instantiation:

- A **parse failure** names the file, the opener that was never closed — `{{` or
  `{%` — and the line that opener sits on, written together as `<path>:<line>`.
  The line named is the opener's own, not the later position at which the parser
  ran out of input. The remedy names the closing delimiter that opener wants and
  offers `{% raw %}` ... `{% endraw %}` for braces that are meant to appear in
  the output.
- A **render failure** names the file, the expression that failed, and its line
  in the same `<path>:<line>` form. When the expression names an input the
  manifest does not declare, the remedy names that input, gives `{% raw %}` ...
  `{% endraw %}` as the way to emit the expression literally instead, and says
  how to list the inputs the template does declare.
- Neither names a construct the file does not contain. A failure caused by an
  unclosed `{%` is never reported as an invalid `{{ }}` expression, and no
  instantiation diagnostic mentions comments, which are not syntax here (§5).
- A `{% raw %}` ... `{% endraw %}` region is literal text and is never blamed:
  an unclosed opener inside one is not a failure and is not reported as one.
- Where no unclosed opener can be identified, the message falls back to the
  position the underlying parser reported, and quotes that parser's own words
  rather than contradicting them.

The `<path>` of every message and warning in this section is written in the
platform's own spelling, so the file a Unix reader is sent to as
`scripts/emit.sh:5` is named `scripts\emit.sh:5` on Windows — the one declared
difference here, and the spelling that platform's own shell and editor take
([§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity)).

Rendering a file whose text contains `{#` also emits one **warning** per file on
stderr, naming the file and the line of the first occurrence and saying that the
text is now emitted verbatim (§5). The warning is not a failure: instantiation
proceeds and the exit code is unchanged, so `--dry-run` is the way to ask which
files a template has that earlier versions silently cut.

### 5.4. Reserved Code Points

Emitting `{#` and `\{{` as written (§5, §5.2) costs a handful of code points:
rendering stands them in for that text while the parser is looking at it and
puts the text back afterwards. They are a C1 control and two private-use
scalars, which no plan, script, or configuration file is authored with. They are
reserved against a template's own **text** only: an input value that carries one
is resolved into the output as the value it is. A template file whose text
carries one is refused before it is rendered, by an error naming that file and
the code point, rather than having the code point quietly rewritten into
something the author did not write.

## 6. CLI Commands

### 6.1. `rhei instantiate`

Create a concrete plan workspace from a template.

```
rhei instantiate [template] [input ...] [options]

Arguments:
  [template]                   Template name or path to a template directory
  [input ...]                  Positional input values or key=value assignments

Options:
  --set <key>=<value>          Set an input value (repeatable)
  --set-file <key>=<path>      Set an input value from file contents (repeatable)
  --values <file>              Load input values from a YAML or JSON file (repeatable;
                                 later files, then input args / --set, then --set-file
                                 override earlier values)
  --output <path>              Output directory (default: <project>/<template-name>/
                                 inside a Panta project, else ./<template-name>/, where
                                 <template-name> is the directory basename of the
                                 resolved template — see §6.2)
  --execute                    Instantiate and immediately begin execution
  --dry-run                    Show what would be generated without writing files
  --keep-on-error              Keep output directory on validation failure
  --list-inputs                Print the template's input schema and exit
```

#### 6.1.1. Input UX

`rhei instantiate` supports three equivalent ways to provide simple template
inputs:

```bash
rhei instantiate spec-review docs/functional-spec/rhei-run.spec.md
rhei instantiate code-review target=src/auth review_passes=3
rhei instantiate code-review --set target=src/auth --set review_passes=3
```

`--set` remains the explicit, script-safe form. Bare `KEY=VALUE` arguments are
short syntax for `--set KEY=VALUE`. Bare values without `=` are positional input
values.

A template may opt into positional input values by declaring `positional` on
one or more inputs:

```yaml
inputs:
  - name: spec
    description: Path to the specification file to review
    type: path
    positional: 1

  - name: criteria
    description: Additional things to look for during review
    type: string
    required: false
    default: ""
```

With that manifest, these commands are equivalent:

```bash
rhei instantiate spec-review docs/functional-spec/rhei-run.spec.md
rhei instantiate spec-review spec=docs/functional-spec/rhei-run.spec.md
rhei instantiate spec-review --set spec=docs/functional-spec/rhei-run.spec.md
```

For backward-compatible convenience, if a template declares exactly one
required input and no `positional` fields, one bare input value maps to that
required input. Templates with zero required inputs, multiple required inputs,
or multiple bare input values must declare `positional` fields or use explicit
`KEY=VALUE` / `--set` input.

Input arguments are parsed as follows:

- `KEY=VALUE` is an assignment when `KEY` is a valid input identifier. The key
  must name a declared template input.
- A value containing `=` is treated as a positional value when the text before
  the first `=` is not a valid input identifier. This keeps path-like values
  containing `=` usable.
- A positional value is assigned to the input with the matching `positional`
  index. When using the single-required-input fallback, it is assigned to that
  required input.
- Supplying the same input more than once is allowed. Later values from the
  same precedence group override earlier values.
- If both a positional value and an explicit assignment set the same input, the
  explicit assignment wins.
- `--set-file KEY=PATH` remains the file-content form and has higher precedence
  than positional values, `KEY=VALUE`, and `--set`.

#### 6.1.2. Behavior

1. **Show choices when omitted.** If no template is provided, print the same human-readable discovered-template list as `rhei templates` and exit successfully.
2. **Locate template.** Resolve `<template>` through the per-name discovery
   chain in §1.2 unless it is already a filesystem path. The first matching
   directory wins; named instantiation loads that copy and reports its load or
   validation error rather than falling through to a farther copy. Direct paths
   include absolute paths, relative paths containing `/`, and dot-prefixed
   relative paths such as `./my-template` or `../templates/review`. A direct
   path bypasses discovery. When a named template is not found and a discovered
   template name is sufficiently similar, include that closest name as a
   suggestion in the lookup error.
3. **Load manifest.** Parse `template.yaml`, validate schema.
4. **Collect inputs.** Resolve inputs using this precedence order: manifest defaults < `--values` files from left to right < positional input values < `KEY=VALUE` input arguments and `--set` flags from left to right < `--set-file` flags from left to right. Error on missing required inputs, unknown input names, ambiguous positional values, or duplicate `positional` declarations. Validate types and `validate` patterns. For `array` / `object` inputs, positional values, `KEY=VALUE`, `--set`, and `--set-file` values are parsed as YAML/JSON snippets before validation.
5. **Render templates.** Walk all materialized text files in the template directory and render them through the restricted MiniJinja environment. `template.yaml` is parsed before this step and is never rendered into the output. Error on any unresolved instantiation template reference.
6. **Write output.** In normal mode, copy the resolved tree to `--output`. `--output` must not already exist; instantiation fails rather than merging into or overwriting an existing path. In `--dry-run` mode, the CLI skips the output-path existence check, materializes into a temporary scratch directory instead of `--output`, validates that scratch output, and reports what would have been written. Preserve directory structure and file permissions. Hidden files and directories (names starting with `.`) and `template.yaml` itself are excluded from the output. A root-level `settings.json` in the template is moved to `.agent-grounds/rhei/settings.json` under the output root; all other files preserve their template-relative paths.
7. **Validate.** Run `rhei validate` on the instantiated plan. If the output root contains `states.yaml`, treat that file as the state machine for validation; otherwise fall back to the built-in default. Validation composes the merged settings (global, then the output root's project settings file, §FS-rhei-agents.1.1) and resolves every `agent`, `model`, `mcp_servers`, and `skills` reference declared in the state machine. Warnings are printed; errors abort (output directory is removed on error unless `--keep-on-error` is passed).
8. **Print summary.** After successful validation, print a human-readable instantiation summary with the output path, task/state counts, instantiated output tree, rendered task tree, the last few rendered task definitions in source order, and a stop-point explanation. For normal instantiation without `--execute`, the stop point is the next ready task and the reason is that execution has not started.
9. **Print invocation.** Print a shell-safe `rhei instantiate ... --output <path>` command that shows how to instantiate the same template and input values again. The printed command uses the resolved output path, so shell expressions such as `$(date ...)` appear as the concrete path value seen by the CLI.
10. **Execute (optional).** When `--execute` is passed, invoke `rhei run <output>` after successful validation. `rhei run` uses the instantiated output's root `states.yaml` by default when present; otherwise it falls back to the built-in default.

#### 6.1.3. Instantiation Summary Output

After successful validation, `rhei instantiate` prints a compact report before
the reproducible invocation. The report is intended to be useful for both
humans and agent-orchestrator logs: it shows what was created, which tasks are
present, the most recent task definitions, and why instantiation stopped.

The summary uses these headings in order:

```text
=== Instantiation Summary ===
Output: <output-path>
Tasks: <count>
States: <state=count[, ...]>

Files:
  <output-path>/
  ...

Task tree:
  - Task <id>: <title> [<state>]
  ...

Recent task definitions:
--- Task <id>: <title> [<state>] ---
### Task <id>: <title>
**State:** <state>
...

Stopped:
  <stop reason>
```

Paths in the report — the `Output:` line, the `Files` root, the `Stopped`
follow-up commands, the placement report, and the reproducible invocation's
`--output` — render relative to the working directory whenever they sit inside
it. The report is read, and its commands pasted, from that directory:
`rhei next panta/product-management` is what a reader actually types, where the
absolute form merely restates where they already are. Paths outside the working
directory stay absolute.

`Files` prints the full instantiated output tree, including hidden files that
were generated by instantiation such as `.agent-grounds/rhei/settings.json`. The root line
uses the user-requested `--output` path. In `--dry-run`, this is still the
requested path even though the rendered files live in a temporary scratch
directory and are discarded after validation.

`Task tree` prints every rendered task in source order, preserving hierarchy by
indentation. Each line includes the canonical task kind, task id, title, and
current state.

`Recent task definitions` prints up to the last five rendered task definitions
in source order. Each definition reconstructs the task heading, `**State:**`,
`**Prior:**` when present, `**Assignee:**` when present, and the task body. It
does not inline child task definitions under a parent; child tasks appear as
their own recent definitions when they are among the last five tasks.

`Stopped` explains the current hand-off point:

- In `--dry-run`, the reason says rendering and validation succeeded but no
  files were written to the requested output path.
- If all tasks are terminal, the reason says the plan is already complete.
- If one or more tasks are in a gating state, the reason lists the first few
  human-gated tasks.
- If at least one task is ready, the reason names the next ready task and shows
  `rhei run <output>` and `rhei next <output>` follow-up commands.
- If no task is ready because prerequisites are incomplete, the reason lists
  the first few blocked tasks.
- If none of the above applies, the reason says validation succeeded but no
  claimable task was found.

#### 6.1.4. Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Any instantiation error, including template lookup failure, missing or invalid inputs, unresolved instantiation variables, output-path conflicts, validation failure, or `--execute` runtime failure surfaced by the CLI |

### 6.2. Instantiating inside a Panta project

A template is not a free-floating directory. Dropped next to `index.panta.md`
it becomes a member rhei: it keeps the state machine it declares
([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)), but the project — not the workspace — owns the one
settings root ([§FS-rhei-agents.1.1](rhei-agents.spec.md#11-global-and-project-settings)) that governs it. `rhei instantiate`
therefore resolves the enclosing project before writing, and reconciles with
it.

**Default output.** Run inside a project, a template's default output is
`<project>/<template-name>/` — the project is the default home for a new rhei
([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)), the same place the empty-project message points at.
Defaulting to the working directory instead dropped the workspace where
discovery never looks, so the command reported success and `rhei list` still
said the project had no tickets. Outside a project the default stays
`./<template-name>/`.

**A taken output directory is a naming problem, and the error says so.** The
default output is the template's own name, and a rhei's id *is* its directory
name ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)), so instantiating the same template a second time in
one project always collides. That second instantiation is not an edge case —
it is reviewing a second spec, auditing a second subject, running a second
release checklist, the most likely next thing anyone does with a template. The
error therefore names the cause, explains that the directory name becomes the
rhei id prefixing every ticket id, and prints a ready command with a free name
filled in:

```text
'panta/spec-review' already exists, so template 'spec-review' has already been
instantiated here under that name. A second copy needs its own directory,
because the directory name becomes the rhei id every one of its ticket ids is
prefixed with. Name it for what it is about:
  rhei instantiate spec-review spec=docs/payments.spec.md --output panta/spec-review-2
```

A bare `output path '…' already exists` left the user to rediscover `--output`
and to guess whether a second copy was supported at all. An explicit
`--output` that is taken keeps the short form — the caller chose that path, so
the only news is that it exists.

**State machines compose; nothing to reconcile.** The state machine is a
property of the rhei, defaulted by the project ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding),
[§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)): a template that declares its own machine lands as a member
running under it, its `states.yaml` staying in the workspace root where
per-rhei resolution finds it, and the project manifest is never edited. A
template that declares nothing inherits the project default. Ten templates
with ten machines coexist in one project — each is a distinct process, and
that is the product's normal shape, not an edge case.

Two earlier mechanisms are deliberately gone. *Refusal* — rejecting a template
whose machine differed from the project's — walled off every template but the
first, contradicting the multi-routine pitch. *Adoption* — writing the first
template's machine into `index.panta.md` as the project default — silently
re-governed every later plain rhei: a hand-written `launch.rhei.md` on
`pending`/`completed` then validated against the template's machine and
failed. Both existed only to police a uniformity the model no longer requires.

**Settings hoist.** A template's bundled `settings.json` (§4) is written to the
*project's* `.agent-grounds/rhei/settings.json` rather than the workspace's, merged
key-by-key into whatever is already there. Values the project already defines
always win — a template must not quietly redefine an agent the operator
configured — and both the added keys and the kept ones are reported. No copy is
left in the workspace, where nothing would read it.

**A project settings file the hoist read from the deprecated home is reported
as superseded, not as material to move.** The merge is written to
`.agent-grounds/rhei/settings.json` and the deprecated file is left on disk
holding its pre-merge content, so the generic move-warning (§1.3) would be
actively wrong here: it names a path rhei has just written itself, and obeying
it drops the template's entries. The hoist therefore says its own piece
instead, naming the deprecated file, stating that the merged file supersedes it
and that it can be deleted. Rhei does not delete it — rhei wrote the workspace
copy it removes above, and it did not write this one.

**A discarded instantiation leaves no hoist behind.** Validation runs after the
hoist, so output that fails it has already written the project's settings file.
Discarding the output undoes the hoist: a project with no file at the current
home has the written one and its new directories removed, and one that had a
file gets its pre-merge content back. A command that failed must not change
which file the project reads. `--keep-on-error` keeps the hoist with the output.

**Standalone output inside a git repository.** A standalone workspace — what
`--output` outside any project produces — lands outside any project, so
no init-managed ignore rule covers it: `git status` reports it as untracked
even when the repository gitignores `panta/` on the position that planning
state is working material ([§FS-rhei-init.3](rhei-init.spec.md#3-ignore-rules)). Instantiation does not edit
`.gitignore` itself — a standalone workspace is deliberately also how
committed, versioned workspaces (checked-in examples) are produced, so the
versioning call belongs to the user. Instead, when the standalone output sits
inside a git repository and is not already ignored, the summary states that
the workspace is untracked and names the `.gitignore` entry that would ignore
it. No note is printed for output that is already ignored, outside any
repository, or a member of a project.

**Validation scope.** A member rhei is validated *through its project*, because
that is the only context in which its machine and settings resolve. Validating
the workspace in isolation is what let `rhei instantiate` print
"Validation succeeded" for output that made every project-scoped command fail.

**Output that the project cannot see.** An `--output` path under the project
but not directly next to `index.panta.md` is not a member: discovery never
recurses ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)). That is allowed, but warned about, naming the
member path that would have worked.

### 6.3. `rhei templates`

List available templates, or show one template in detail.

```
rhei templates [template] [options]

Options:
  --json                       Output as JSON
  --source <project|user|all>  Filter by source (default: all)
```

Without a positional, prints a table of discovered templates with name,
version, description, source path, and required input count. Project-local
names are aggregated across every ancestor in §1.2 and appear once each; user
and built-in copies appear only when no project ancestor already contributed
that name. `--source` filters this listing by tier without changing named
lookup precedence.

With a template name (or a path to a template directory), prints that
template's detail: source, path, and the same input schema
`rhei instantiate <template> --list-inputs` prints, followed by an
instantiation hint. Naming a template is the natural gesture after reading
the list — it must answer with the template, not an argument error. `--json`
emits the single template as one object in the list's entry shape. For valid
copies, named detail selects the same source, path, and manifest that an
unfiltered listing and named instantiation select.

When discovery encounters an unreadable or invalid template directory,
`rhei templates` skips it and prints the existing warning instead of failing
the entire listing or detail lookup. This skip may expose the next valid copy.
It does not change named instantiation's rule in §6.1.2: instantiation stops at
the first matching directory and reports that copy's error. Deprecated-home
warnings remain governed by §1.3 and fire only for copies actually read.

#### 6.3.1. The JSON entry carries the whole input schema

`--json` is the surface another program reads a template through, so every
input in the entry carries every key the manifest declared for it — not the
subset the human table prints:

| Key | Value |
| --- | --- |
| `name`, `type`, `required`, `description` | always present |
| `default` | the declared default, or `null` |
| `validate` | the declared pattern, or `null` |
| `format` | the declared format (§3.1), or `null` |
| `positional` | the declared 1-based slot, or `null` |
| `items` | for `type: array`, the element schema; `null` otherwise |
| `properties` | for `type: object`, a name → schema map; `null` otherwise |

`items` and `properties` nest the same value schema — `type`, `required`,
`default`, `validate`, `format`, and their own `items` and `properties` — so a
caller can walk an array of execution targets or a record of typed fields
without opening `template.yaml`, which a built-in template has nowhere on disk
to open.

This is what lets a caller build an input form rather than a text box:
`format: execution-target` says a value is an agent selector and not free
text, `items.format` says the same of every element of an array, and
`positional` says which input the template treats as its principal one.

## Example

### Template: `code-review`

```
~/.agent-grounds/rhei/templates/code-review/
├── template.yaml
├── states.yaml
├── prompt_templates/
│   └── review.md
├── index.rhei.md
├── tasks/
│   └── 01-review.md
└── workflow.sh
```

#### `template.yaml`

```yaml
name: code-review
version: 1.0.0
description: Multi-pass code review with fix loop

inputs:
  - name: target
    description: File or directory to review
    type: path
    positional: 1

  - name: review_passes
    description: Number of review iterations
    type: number
    default: 2

  - name: model
    description: Model to use for review
    default: claude
```

#### `prompt_templates/review.md`

```markdown
Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
Focus on `{{target}}`.
Write findings to `{review_notes_path}`.
```

#### `index.rhei.md`

```markdown
# Rhei: Code Review — {{target}}
**States:** code-review

## Overview
Automated {{review_passes}}-pass review of `{{target}}` using {{model}}.
```

#### `tasks/01-review.md`

```markdown
### Task review: Review {{target}}
**State:** review

Review `{{target}}` for correctness, style, and security issues.
Write findings to `{output.review-notes.path}`.
```

#### `states.yaml`

```yaml
name: code-review
version: 1.0.0

states:
  review:
    description: Review pass
    prompt_template:
      name: review
      values:
        visit_count: "{visit_count}"
        visits: "{visits}"
        task_id: "{task_id}"
        task_title: "{task_title}"
        review_notes_path: "{output.review-notes.path}"
    visits: {{review_passes}}
    outputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md

  fix:
    description: Fix findings from review
    instructions: |
      Fix pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Read `{input.review-notes.path}` and apply fixes to `{{target}}`.
      Write a summary of changes to `{output.fix-summary.path}`.
    visits: {{review_passes}}
    inputs:
      - name: review-notes
        path: runtime/reviews/task-{task_id}-review-{visit_count}.md
    outputs:
      - name: fix-summary
        path: runtime/fixes/task-{task_id}-fix-{visit_count}.md

  completed:
    description: Review and fixes complete.
    final: true

  cancelled:
    description: Review cancelled.
    final: true

transitions:
  - from: review
    to: fix
    description: Review pass done, apply fixes.
  - from: fix
    to: review
    condition: visitCount < visits
    description: Return for another review pass.
  - from: fix
    to: completed
    condition: visitCount >= visits
    description: All passes complete.
  - from: "*"
    to: cancelled
    description: Cancel from any state.

profiles:
  default:
    initial: review
    allowed: [review, fix, completed, cancelled]

node_policy:
  root: default
  default: default
```

### Instantiation

```bash
rhei instantiate code-review src/auth/ \
  review_passes=3 \
  --output ./reviews/auth-review/

# Or instantiate and run immediately:
rhei instantiate code-review src/auth/ --execute
```

### Resulting Output (`./reviews/auth-review/`)

```markdown
# index.rhei.md
# Rhei: Code Review — src/auth/
**States:** code-review

## Overview
Automated 3-pass review of `src/auth/` using claude.
```

```yaml
# states.yaml (instantiation variables resolved; path text preserved as authored input)
name: code-review
states:
  review:
    instructions: |
      Review pass {visit_count} of {visits} for Task {task_id}: {task_title}.
      Focus on `src/auth/`.
    visits: 3
    ...
```

All `{{...}}` are resolved during instantiation. All `{...}` remain for runtime.

## 7. Interaction with Existing Features

| Feature | Interaction |
|---------|-------------|
| **Runtime variables** (`{task_id}`, etc.) | Pass through instantiation untouched. Resolved later by `rhei next`. |
| **State machines** | A template may bundle its own `states.yaml` at the output root. If present, `rhei instantiate`, `rhei validate`, `rhei run`, `rhei next`, and related commands that operate on the instantiated workspace use that sibling/root `states.yaml` by default when `--state-machine` is not supplied; otherwise they fall back to the built-in default. When the rendered plan declares `**States:** <name>`, that declaration participates in lookup: the auto-discovered `states.yaml` is the active configuration and its `name` must match `<name>`. `--state-machine <path>` overrides the auto-discovered file. If a sibling `prompt_templates/` directory exists next to the active `states.yaml`, its direct `.md` files are loaded with that state machine. Templates that rely on non-default state names should therefore bundle `states.yaml`. |
| **Directory workspaces** | Templates can produce directory workspaces. The `tasks/` directory and `index.rhei.md` are resolved like any other template file. |
| **`rhei validate`** | Runs automatically post-instantiation. Template authors can validate their templates with `rhei instantiate --dry-run`. |
| **Program states** | Program states (`program` field) work in templates. Instantiation variables resolve in `program` strings, `program.command` arrays, `program.env` values, and `program.working_directory`. Runtime variables in those fields pass through to `rhei run`. |
| **Bundled shell scripts and workflow fragments** | A script bundled in a template is rendered like any other text file and arrives with its permissions, executable bit included. Shell parameter expansion is not instantiation syntax and passes through verbatim, `${#ARR[@]}` included (§5). A GitHub Actions `${{ ... }}` *is* a `{{ ... }}` interpolation and is still read as one, so a workflow fragment that must keep its own expressions wraps them in `{% raw %}` ... `{% endraw %}`; where it does not, the failure names the line and the expression rather than the manifest (§5.3). |
| **Skills** | The `rhei-plan-worker` skill works on instantiated plans identically to hand-authored plans. No skill changes required. |
| **`install-skills`** | Unchanged. Templates are orthogonal to skill installation. |

## 8. Grammar Extension

The `rhei_document` and `workspace_index` productions are unchanged. Templates are a pre-processing layer that produces valid Rhei documents — the parser never sees `{{...}}` syntax.

No changes to the Rhei plan grammar are required.

## 9. Manifest Fields

`template.yaml` is a YAML mapping parsed by field name, not by key order. The following schema is normative:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | Yes | Must match the enclosing template directory name and the identifier rules described above. |
| `version` | YAML scalar | Yes | Informational metadata only in v1. |
| `description` | string | Yes | Must be non-empty after trimming. |
| `inputs` | sequence of mappings | No | Defaults to an empty list when omitted. |

Each `inputs[]` entry is a YAML mapping with these fields:

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `name` | string | Yes | Unique within the manifest; referenced as `{{name}}` in instantiation variables. |
| `description` | string | Yes | Must be non-empty after trimming. |
| `type` | `string` \| `number` \| `boolean` \| `path` \| `array` \| `object` | No | Defaults to `string`. |
| `required` | boolean | No | Defaults to `true` unless a `default` is present. |
| `default` | YAML value | No | Must be compatible with `type`; mutually exclusive with `required: true`. |
| `positional` | positive integer | No | Optional 1-based CLI positional input slot. Values must be unique and contiguous starting at `1`. |
| `validate` | string | No | Rust `regex`-crate pattern, matched against the fully rendered value. |
| `format` | `execution-target` | No | Built-in value check applied at instantiation time. |

## 10. File Extension

Template manifest files use the `.yaml` extension. Template plan entry points are exactly `plan.rhei.md` or `index.rhei.md`. In directory-workspace templates, files under `tasks/` may use `.md` consistent with standard Rhei workspaces.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — Grammar and semantics of the output format
- [States Specification](rhei-states.spec.md) — State machine format (bundled in templates)
- [Program States Specification](rhei-programs.spec.md) — Deterministic program execution (program states work in templates)
- [How Rhei Is Used](rhei-usage.spec.md) — Roles and coordination patterns
