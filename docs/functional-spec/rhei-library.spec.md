# FS-rhei-library: Composable Blocks

A **block** is a template whose plan, state-machine fragment, settings, files,
and typed inputs can be mounted with other blocks. Composition is an
instantiation-time compile step: it produces one ordinary plan workspace and
one flat `states.yaml`, validates that output, and adds no composition syntax
to runtime parsing or execution. This makes reusable flows first-class while
preserving the predictable output required by
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals).

## 1. Block manifests and encapsulation

A block uses the existing template directory and `template.yaml`; there is no
second manifest or discovery catalog. The existing `name`, `version`,
`description`, and `inputs` fields retain their meanings. A manifest may also
declare `ports`, `data`, `use`, `bind`, `seams`, and `compatibility`.

`ports` declares the control surface:

```yaml
ports:
  entry: review
  exits:
    done: completed
    cancelled: cancelled
```

- `entry` names exactly one local state, or `<child-alias>.<port>` in a block
  with `use`.
- `exits` is a non-empty mapping from public port names to local states or
  child control endpoints.
- The conventional exit name used by default sequencing is `done`.
- Port names use the template identifier grammar: they start with an ASCII
  letter and continue with ASCII letters, digits, `_`, or `-`.
- Every local state must exist in the rendered local state fragment and every
  child endpoint must resolve after expansion. An exit state is terminal before
  composition.

`data` declares the only runtime values another block may consume:

```yaml
data:
  inputs:
    brief: { kind: state-file, state: prepare, name: brief }
  outputs:
    findings: { kind: task-export, task: review, name: findings }
```

The `inputs` and `outputs` mappings are keyed by public endpoint name. An
endpoint is one of:

- `{kind: state-file, state: <local-state>, name: <artifact-name>}`. An input
  must resolve to that state's declared input artifact; an output must resolve
  to that state's declared output artifact.
- `{kind: task-export, task: <local-task-id>, name: <export-name>}`. An output
  must resolve to the task's `Provides` declaration. An input identifies the
  existing consumer task and gives the endpoint its local semantic name; its
  cross-task `Consumes` reference is created only when a pass is lowered.

Endpoint, port, and input names are separately unique within their mappings.
An endpoint declaration exposes the named contract, not the state, task, path,
or export behind it. A parent may cross a mount boundary only through a
declared input, control port, or data endpoint. All other block-owned names and
files remain private even though the compiled workspace necessarily contains
their qualified forms.

A legacy template with none of the new fields remains valid and behaves as
before. A block used by `use` or `--mount` must declare `ports`, including a
`done` exit when default sequencing is requested.

A manifest with `use` may omit a local plan entry point and local
`states.yaml`; its children then supply the complete fragment. Otherwise the
existing exactly-one-plan-entry rule remains. This is what permits a curated
flow to be only composition rather than a placeholder task or machine.

## 2. Mounts, bindings, and seams

`use` is an ordered sequence of mounts:

```yaml
use:
  - { block: code-review, as: review }
  - { block: fix, as: fix }
```

`block` is either a discovered template name or a path. A relative path is
resolved from the directory containing the declaring manifest. A name uses the
same project, user, then built-in discovery precedence as a top-level template.
`as` is a mount alias using the template identifier grammar. Aliases are unique
among siblings; the same block may be mounted repeatedly under different
aliases.

Expansion is recursive. Resolution retains a stack of resolved manifest paths
and aliases; encountering a manifest already on that stack is a cycle error.
The diagnostic prints the complete root-to-repeat chain. Reusing the same
block in separate branches of the mount tree is not a cycle.

`bind` performs compile-time partial application:

```yaml
bind:
  - { input: change_ref, to: review.target }
```

`input` names an input of the declaring block and `to` is
`<child-alias>.<child-input>`. The source and target definitions must have the
same type, including nested item/property types and format constraints. The
root input is resolved using normal template precedence before the child is
rendered; its typed value then overrides the child's default. Every required
child input must be satisfied by a binding or, in direct composition, by a
qualified user value. A target may be bound once. `bind` is static and may
supply MiniJinja; it never reads runtime output.

`seams` is an ordered sequence of completion-to-entry links:

```yaml
seams:
  - from: review.done
    to: fix.entry
    pass:
      review.findings: fix.brief
```

Control endpoints use `<alias>.<port>`. `entry` is the fixed public name for a
mount's declared `ports.entry`; exit names come from `ports.exits`. Data
endpoints use `<alias>.<endpoint>`. A pass always maps an output to an input.

When `seams` is omitted, mount order supplies a seam from each mount's `done`
exit to the next mount's `entry`. When `seams` is present it is the whole outer
chain: it must form one acyclic linear ordering that covers every sibling mount
exactly once. The head has no incoming seam, the tail has no outgoing seam,
and every other mount has one of each. No implicit edge is added beside an
explicit seam.

A seam fires only when its source exit completes. Seams do not accept a
condition, gate, callback, or arbitrary expression. Conditions, gates, and
callbacks authored inside a block remain intact, and a seam cannot replace an
internal edge. Conditional or gated seams are a deferred feature.

## 3. Command-line composition and inputs

Direct composition uses repeatable, explicit mounts:

```text
rhei instantiate \
  --mount review=code-review --mount fix=fix \
  --set review.target=HEAD~3 \
  --seam review.done=fix.entry \
  --pass review.findings=fix.brief
```

`--mount <alias>=<block>` preserves command order. Its alias follows §2 and
its block reference follows the same named/path rules as `use`. The direct
mount form has no positional template or positional input values; supplying
either is an actionable ambiguity error. Conversely, when no `--mount` is
present the first positional remains the one legacy template and every later
positional retains the parsing in
[§FS-rhei-templates.6.1.1](rhei-templates.spec.md#611-input-ux). A positional
value that happens to name a block is never reinterpreted as a mount.

Mounted inputs use `<alias>.<input>` with `--set` and `--set-file`. A
`--values` document uses nested alias mappings:

```yaml
review:
  target: HEAD~3
fix:
  mode: worktree
```

After flattening those mappings, precedence applies per qualified key:
manifest default, then `--values` files from left to right, then `--set` from
left to right, then `--set-file` from left to right. The same rule applies at
each recursive expansion; a parent's `bind` is the user-supplied value at the
child boundary. Unknown aliases and unknown inputs are rejected before any
file is rendered.

`--seam <source>=<target>` uses the same endpoint grammar as manifest seams.
`--pass <source>=<target>` identifies the unique declared seam whose source and
target mounts own those data endpoints, so option order is immaterial. No
matching seam or more than one matching seam is an error; use a curated block
when two seams between the same mounts need distinct pass sets.

`--list-inputs` prints the qualified union in mount order. `--dry-run` resolves,
renders, compiles, and validates the complete graph in scratch without writing
the requested output. `--execute` starts `rhei run` only after successful
materialization and validation. Direct composition defaults its output name to
the ordered aliases joined by `-`; `--output` wins. A curated block defaults to
its own name. Existing refusal to merge into an existing output remains.

## 4. Qualification and generated workspace

Each mounted item has an alias chain. One alias segment is encoded as `m`, the
decimal UTF-8 byte length of the alias, `_`, the alias, then `__`; nested
segments concatenate. Thus local state `done` under alias `review` becomes
`m6_review__done`, while the encoding of nested and repeated mounts remains
injective and reversible. The local identifier is appended unchanged.

Qualification is semantic. The compiler rewrites definitions and references
for:

- task ids, hierarchy, `Prior`, `Provides`, and `Consumes`;
- states, transitions, conditions' state references, and control ports;
- profile names, `initial`/`allowed` states, node-policy routing, and task
  kinds owned by a routing rule;
- prompt-template file references and their mounted file locations;
- block-shipped agent, model, MCP-server, and skill setting ids and every local
  reference to them; and
- state artifact paths, task-export paths, program working directories, and
  other declared paths owned by the mounted block.

Text search and replacement is forbidden. A reference to a setting not shipped
by the block remains external and unqualified; a reference to a block-shipped
setting is owned and qualified. Dangling references after this classification
are errors. Ownership is resolved within the reference's registry kind: an
owned agent does not capture an external model, MCP server, or skill with the
same spelling.

Task-state qualification uses the shared counted-state parser: exact state
names take precedence, otherwise the owned base state is qualified and its
explicit visit count is preserved (`work-2` becomes `m1_a__work-2`).

Mounted relative runtime paths are placed below
`runtime/blocks/<encoded-alias-chain>/`, followed by the complete normalized
local relative path. Retaining the complete path makes `runtime/x` and `x`
distinct. Absolute artifact paths and paths containing `..` are invalid in a
mounted block. Private mounted files live below
`.agent-grounds/rhei/blocks/<encoded-alias-chain>/`; typed references to them
are rewritten. Runtime prompt templates are the exception required by the
unchanged loader: they are emitted as
`prompt_templates/<encoded-alias-chain><local-name>.md`, and the state's
`prompt_template` reference receives that same qualified name. This prevents
same-named scripts, prompt templates, and support files from colliding.

The compiled output is one ordinary workspace with one ordinary flat
`states.yaml`. Existing workspace/state validation runs after lowering.
`validate`, `next`, `transition`, `complete`, `run`, `states`, and `viz` receive
no block syntax. Generated plan and state files begin with `# Generated by rhei
block compiler v1; root: <root-name-or-ordered-aliases>`, followed by one
`# block:` line per resolved block naming its name, version, source path/tier,
and alias chain. Markdown uses the equivalent HTML comment. Per-node origin
records and richer lock metadata are deferred.

## 5. Control flow and routing

An authored terminal exit remains terminal unless a seam consumes that exact
exit. A consumed exit loses `final: true` and receives the one seam transition.
The tail's unconsumed `done` exit and every unconsumed alternate exit, including
cancellation exits, retain their authored terminality. Internal transitions
are preserved.

The compiler derives an outer profile whose initial state is the head mount's
entry and whose allowed states contain each mounted primary lane in seam order.
Tasks owned by those lanes route through the outer profile. A block's additional
profiles and node-policy overrides remain separate, qualified internal lanes;
`all_targets`/`all_models` fan-out remains on the state that declared it.
Authored `by_type` and level rules are restricted to task kinds owned by that
mount and cannot capture another mount's tasks. The lowered profiles and policy
must satisfy [§FS-rhei-states.8](rhei-states.spec.md#8-profiles) and
[§FS-rhei-states.9](rhei-states.spec.md#9-node-policy).

## 6. Runtime data passes

`pass` is runtime wiring, distinct from instantiate-time `bind`. It never
substitutes a rendered string. Both endpoints must exist, have opposite
directions, and have the same `kind`.

For `state-file`, the producer output and consumer input lower to the same
qualified producer path within the composed task's execution root. The source
output remains required, so its absence blocks successful producer completion.
The target retains its authored required/optional flag: a missing required
input blocks entry and a missing optional input does not. Resolution and check
ordering remain [§FS-rhei-plan-language.3.10](rhei-plan-language.spec.md#310-state-artifact-contracts).
State-file passes between different task owners are rejected; task exports are
the cross-task mechanism.

For `task-export`, lowering adds or rewrites the producer's qualified
`Provides` and the consumer's qualified `Consumes` reference. The path is read
from the qualified producer task's execution root. Present non-empty content is
injected into the consumer prompt; an absent or empty export is skipped under
[§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports).
Composition does not strengthen that optional runtime behavior.

A state-file/task-export conversion is invalid. A data mapping does not create
a control dependency: the matching seam and the plan's task dependency remain
the ordering authorities.

## 7. Identity compatibility

A curated wrapper may retain an established public identity while compiling
mounted blocks. `compatibility` maps each stable local output identity to one
qualified child identity:

```yaml
compatibility:
  states: { split: review.split, final-fix: fix.final-fix }
  tasks: { coordinate: review.coordinate }
  profiles: { root: review.root }
  settings: { smart: fix.smart }
  artifacts: { runtime/final.md: fix.final-result }
```

The supported maps are `states`, `tasks`, `profiles`, `settings`, and
`artifacts`. State/task/profile/settings values are `<alias>.<local-name>`;
artifact values are declared data endpoints. Every source and target must
resolve and kinds must agree. Stable keys are unique across the corresponding
compiled namespace, and two keys may not claim the same target. A collision or
unresolved mapping is an error rather than a silent rename.

Compatibility lowering is applied only at the identity root. When that wrapper
is subsequently mounted, its stable local names are qualified by the outer
alias like any other block. This is how the `changeset-review` wrapper preserves
its current public input names, controls, state/task/profile names, shipped
settings references, artifact paths, output placement, and behavior while its
review and fix portions become reusable blocks connected by a real artifact
pass.

The proving extraction ships the review portion as the discoverable block
`code-review` (`split` through `human-review`) and the fix portion as `fix`
(`prepare-workspace`, `final-fix`, and `commit-fix`). `code-review` exposes
`entry`, `done`, and state-file output `decision`; `fix` exposes `entry`,
`done`, and state-file input `decision`. The `changeset-review` wrapper mounts
them as `review` and `fix`, passes `review.decision` to `fix.decision`, binds its
existing public inputs, and applies the compatibility map. Directly mounting
the two blocks is supported independently of the wrapper.

## 8. Diagnostics and deferred surface

Composition failures follow [§FS-rhei-errors.1](rhei-errors.spec.md#1-anatomy-of-an-error).
They name the failing alias or endpoint, both declaring manifest paths where
two declarations are involved, the full mount chain for nested failures, and a
next action. Small valid sets are listed and near misses are suggested.

This applies to duplicate or invalid aliases, unknown blocks, cycles, missing
ports, malformed or incomplete seam chains, unknown inputs/endpoints, duplicate
bindings, kind mismatches, cross-task file passes, unresolved owned references,
and incompatible identity maps. No partial output is accepted after one of
these failures.

Catalog/discovery UX beyond existing template discovery, a replacement textual
authoring language, richer provenance, additional exposure modes,
conditional/gated seams, and arbitrary pass expressions are deliberately
outside this contract. They are linked non-blocking follow-up work, and this
feature remains deferred past release 0.5.0.

## 9. Related specifications

- [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
  owns template discovery, input types, rendering, placement, and the CLI shell
  around compilation.
- [§AR-rhei-library](../architecture/rhei-library.spec.md#ar-rhei-library-block-compiler-architecture)
  owns the typed compiler boundary.
- [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
  and [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
  own the generated runtime contracts.
