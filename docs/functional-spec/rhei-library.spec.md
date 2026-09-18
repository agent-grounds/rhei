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
declare `ports`, `data`, `expose`, `use`, `bind`, `seams`, `compatibility`, and
the opt-in `select` declaration template below.

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
declared input, control port, data endpoint, or typed identity exposure under
§1.2. All other block-owned names and files remain private even though the
compiled workspace necessarily contains their qualified forms.

A legacy template with none of the new fields remains valid and behaves as
before. A block used by `use` or `--mount` must declare `ports`, including a
`done` exit when default sequencing is requested.

A manifest with `use` may omit a local plan entry point and local
`states.yaml`; its children then supply the complete fragment. Otherwise the
existing exactly-one-plan-entry rule remains. This is what permits a curated
flow to be only composition rather than a placeholder task or machine.

### 1.1. Input-selected declarations

`select` is an optional YAML block scalar containing a restricted MiniJinja
template whose result is a mapping with only `ports`, `data`, `expose`, and
`compatibility`. It uses the same restricted environment as materialized files
([§FS-rhei-templates.5](rhei-templates.spec.md#5-instantiation-template-syntax)).

```yaml
select: |
  ports:
    entry: {% if fix_prepare == 'none' %}final-fix{% else %}prepare-workspace{% endif %}
    exits: { done: completed, cancelled: cancelled }
  data:
    outputs:
      final-fix: {kind: state-file, state: final-fix, name: final-fix-note}
      {% if fix_commit != 'none' %}
      commit-ref: {kind: state-file, state: commit-fix, name: commit-ref}
      {% endif %}
```

Parse and validate the static manifest, including the unchanged input schema,
first. Resolve typed input values using the established precedence and binding
rules. Render `select` once from those values, parse its result as the closed
declaration schema, and render the local files with the same values. Check all
selected declarations against the typed rendered fragments and expanded child
interfaces before qualification, seams, or compatibility lowering. A selected
`expose` group may name only identities present in that selected mode. No
runtime value participates in selection. The core receives ordinary typed
declarations, never MiniJinja source.

A group may be authored statically or selected, never both in one manifest;
duplicate groups are errors, even if the static group is empty. An omitted
selected group retains its static value or ordinary absent default. An empty
mapping selects nothing. Null/non-mapping output, unknown fields at any selected
declaration level, invalid types, unresolved references, and rendering errors
are refused. Diagnostics name the authored `template.yaml`, `select`, the
offending group/endpoint, the mount chain, and how to correct it. `inputs`,
`use`, `bind`, `seams`, and every other manifest field remain static. The
manifest itself is never rendered and legacy parsing is unchanged without
`select`.

### 1.2. Typed identity exposure

`expose` is the closed, opt-in public identity surface of a block:

```yaml
expose:
  states:
    ready: { local: internal-ready }
  tasks:
    audit: { local: phase.audit }
  settings:
    agents: { reviewer: { local: review-agent } }
    models: { careful: { local: reasoning-model } }
    mcp_servers: { tracker: { local: tracker-server } }
    skills: { checklist: { local: review-checklist } }
```

The only top-level kinds are `states`, `tasks`, and `settings`; the only
settings registries are `agents`, `models`, `mcp_servers`, and `skills`. Each
mapping key is a public name using the template identifier grammar. Each target
has exactly one of these forms:

- `{ local: <identity> }` names an identity owned by the declaring block's
  rendered local fragment or owned settings registry.
- `{ mount: <child-alias>, name: <public-name> }` names an exposure of one
  immediate child in the same kind and, for settings, the same registry.

The two forms cannot be combined and no additional target fields are valid.
Within one kind or settings registry, public names and resolved targets are
one-to-one. Duplicate public names, two names claiming one target, and a public
name whose generated identity would collide with another owned identity are
errors. Explicit kind and target forms prevent dotted task ids and equal names
in separate settings registries from creating ownership ambiguity.

A parent refers to an immediate mount's exposed identity as
`<mount>.<public-name>`, for example state `review.ready`, task `review.audit`,
agent `review.reviewer`, or model `review.careful`. State exposures are valid in
every existing typed state-reference position: task state, transitions,
profiles, snapshot selectors, and counted-state identities. The visit suffix
remains attached to the resolved exposed state. Task exposures are valid in
typed task-reference positions, including `Prior`. Settings exposures are
valid only in reference positions belonging to their declared registry,
including execution targets and per-state MCP-server and skill lists.

Exposure grants identity access only. It does not expose defaults, secrets,
arbitrary settings values, setting-owned files, paths, task exports, or a
task's `Provides`/`Consumes` surface. Inputs, control ports, data endpoints,
bindings, seams, and passes retain their existing meanings. A task exposure in
`Prior` therefore creates a dependency but never authorizes reading one of the
task's exports; that still requires a declared data endpoint and pass.

The declaration is the complete author-owned public identity surface. Curated
and direct mounts consume it without a caller-side selection step. Exposure is
not transitive: a wrapper must explicitly re-expose an immediate child's
public name with the `mount` form at every boundary. A reference may contain
only its immediate mount and public name, so `outer.review.ready` cannot bypass
an `outer` wrapper that omitted the re-exposure. Undeclared local identities,
child private names, and generated qualified spellings remain inaccessible.
Settings references, including execution-target components, are checked against
the immediate children's actual compiled ownership in the relevant registry
before merging. An undotted spelling that names such a child-owned identity
does not become external; genuinely external names and valid locally owned
references remain valid, even when their spelling resembles a generated name.

Public names are stable interface identities. Mounted as `review`, exposed
state `ready` lowers to the ordinary generated identity `m6_review__ready`.
Changing the local or child target while retaining a compatible public key
preserves both `review.ready` and the generated identity. Renaming or removing
the key is an interface change. Blocks without `expose` retain exactly their
existing privacy, generated names, output, and runtime behavior.

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

Direct mounts consume each block's `expose` declaration automatically. They
have no flag, values key, or other caller-side grammar for selecting exposed
members. A direct and curated mount with the same alias produce the same public
qualified identities and apply the same typed validation.

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

Before ordinary qualification, the compiler resolves each typed exposure
against the declaring block's local ownership tables or an immediate child's
public table. It then substitutes the public key for the target's local suffix
in both the exposed definition and every typed reference to it. Qualification
uses that public suffix, so internal target names never leak into the exposed
generated identity. Private definitions continue through the existing
qualification path, and exposure neither merges nor coalesces identities.

Text search and replacement is forbidden. A reference to a setting not shipped
by the block remains external and unqualified; a reference to a block-shipped
setting is owned and qualified. Dangling references after this classification
are errors. Ownership is resolved within the reference's registry kind: an
owned agent does not capture an external model, MCP server, or skill with the
same spelling.

Task-state qualification uses the shared counted-state parser: exact state
names take precedence, otherwise the owned base state is qualified and its
explicit visit count is preserved (`work-2` becomes `m1_a__work-2`).

Cancellation classification is captured before renaming as the ordinary flat
state property `role: cancellation` (§FS-rhei-states.1.4). Wildcard transitions
retain `from: "*"` and receive an explicit `sources` set containing the owner's
states, or their already authored subset (§FS-rhei-transitions.4.6). Both these
references and pre-existing source sets are qualified and compatibility-rewritten.
The source set includes owned terminal states; the runtime excludes final
sources when matching, so a consumed exit becomes eligible only once a seam
makes it non-final. No wildcard is expanded into ordinary exact edges, and no
runtime consumer infers ownership or cancellation from generated prefixes.

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

A cancellation-role exit cannot be consumed by a completion seam. A consumed
human gate keeps its declared cancellation escape as a human transition;
automatic progress follows the exact completion seam, with existing gate rules
unchanged. Scoped wildcard terminal edges remain escapes, never automatic
fallback when a conditional exact edge is inapplicable.

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
injected into the consumer prompt; a missing or blank export fails the
consumed-export preflight, so the consumer is not spawned and its state stays
unchanged ([§FS-rhei-plan-language.3.12.2](rhei-plan-language.spec.md#3122-consumer-availability)).
Composition neither weakens nor strengthens that runtime contract.

A state-file/task-export conversion is invalid. A state-file pass does not
create a control dependency: the matching seam and the plan's task dependency
remain the ordering authorities. A task-export pass adds the qualified producer
to the consumer's `Prior` when it is not already there, under the producer's
own kind keyword, because a consumed export's producer must stand directly in
the consumer's `Prior` ([§FS-rhei-plan-language.3.12.1](rhei-plan-language.spec.md#3121-declaration-integrity));
the seam still carries the control flow between the blocks.

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

The one-target maps are `states`, `tasks`, `profiles`, `settings`, and
`artifacts`; `terminals` is the checked exception in §7.1. State/task/profile/settings values are `<alias>.<local-name>`;
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

Exposure resolution runs before this final root compatibility rename. A
compatibility map may therefore target an immediate child's exposed public
identity and map it to the wrapper's legacy identity. The compatibility name
wins for final spelling. Existing compatibility manifests need no `expose`
declaration, and exposure does not add a many-to-one case: §7.1's checked
terminal equivalence remains the only coalescing operation and is validated
before definitions are combined.

### 7.1. Equivalent terminal identities

`compatibility.terminals` explicitly maps a stable state name to a list of at
least two distinct child state identities. Existing one-target `states` maps
retain their meaning:

```yaml
compatibility:
  states: { final-fix: fix.final-fix }
  terminals:
    completed: [review.completed, fix.completed]
    cancelled: [review.cancelled, fix.cancelled]
```

Each target must resolve to an unconsumed terminal. A target can be claimed
once across both state maps, and stable names cannot collide with each other or
an existing definition. Before coalescing, compare the definitions after typed
compatibility/path/reference rewrites and data passes. All members must have
the same effective cancellation role and operative contract: execution selectors
and settings references, effective bound instructions/personality, input/output
artifacts and requiredness, gates, polling/visits, snapshots/handoffs, tooling,
and transition callbacks/conditions. Descriptive prose, declaration order, and
prompt filenames alone are not operative differences; prompt content after
binding is. Missing/unbound prompt templates are errors, never empty contracts.
Terminals with outgoing exact transitions are refused for coalescing.

Only after equality is established may one definition represent the group.
Rewrite every typed reference, including task states, profiles, ports, scoped
wildcard sources, and snapshot references, and deduplicate state sets. Divergent
roles or contracts fail with the stable identity, both child identities and
manifest paths, and the differing contract field; no winner silently overwrites
another definition. This is the sole many-to-one ownership exception. At an
outer mount the stable terminal receives exactly one further qualification.

### 7.2. Extracted workflow modes

For both the reusable `fix` and `changeset-review`, `fix_prepare=none` omits
`prepare-workspace` and every `workspace-ref` input/output; entry is `final-fix`.
Other preparation modes enter `prepare-workspace` and require its workspace
output before fixing. `fix_commit=none` omits `commit-fix` and `commit-ref`,
and `final-fix` leads directly to `completed`. Other commit modes retain the
commit stage and its required output. Selected ports, data endpoints and
compatibility maps must name only present contracts. Every decision consumer
receives the passed decision path. Worktree/fork and commit/PR instructions keep
their respective duties. The five required shapes are none/none, none/commit,
worktree/none, worktree/commit, and fork/pr, at root and mounted boundaries.

Both review and fix completion/cancellation routes use the wrapper's public
`completed`/`cancelled` pair through §7.1. Human review can still cancel through
its owner escape; successful approval follows the completion seam. Standalone
blocks retain their own terminal pair and remain independently composable.

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
and incompatible identity maps. It also applies to unknown exposure fields or
public names, unresolved local or child targets, duplicate target claims,
public/generated identity collisions, wrong-kind uses, ambiguous ownership,
and references to undeclared private members. Exposure diagnostics identify
the authored manifest path, full mount chain, member kind and offending
reference, and include valid public alternatives and a corrective action when
applicable. Every exposure declaration is validated after rendering and before
qualification. No partial output is accepted after one of these failures.
Static and selected closed-schema failures retain the typed declaration path
(for example, `expose.states.ready` or `expose.settings`), the unsupported field,
and the allowed fields as corrective alternatives.

Catalog/discovery UX beyond existing template discovery, a replacement textual
authoring language, richer provenance, exposure of values, defaults, secrets,
files, paths, profiles, task exports, or other identity kinds,
conditional/gated seams, and arbitrary pass expressions are deliberately
outside this contract. They are linked non-blocking follow-up work.

## 9. Related specifications

- [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
  owns template discovery, input types, rendering, placement, and the CLI shell
  around compilation.
- [§AR-rhei-library](../architecture/rhei-library.spec.md#ar-rhei-library-block-compiler-architecture)
  owns the typed compiler boundary.
- [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
  and [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
  own the generated runtime contracts.
