# FS-rhei-library: Composable Blocks

A **block** is a template whose plan, state-machine fragment, settings, files,
and typed inputs can be mounted with other blocks. Composition is an
instantiation-time compile step: it produces one ordinary plan workspace and
one flat `states.yaml`, validates that output, and adds no composition syntax
to runtime parsing or execution. This makes reusable flows first-class while
preserving the predictable output required by
[§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals).

Composition by **graph union** is the normative model, and it begins at [§FS-rhei-library.9](rhei-library.spec.md#9-composition-by-graph-union): a
template's states, edges, profiles, kinds and tickets are added to a machine and
a plan that already exist, under the names their authors wrote, by
`rhei instantiate --into` ([§FS-rhei-library.10](rhei-library.spec.md#10---into-placing-a-template-into-a-plan)) or by `includes:` in `template.yaml` ([§FS-rhei-library.14](rhei-library.spec.md#14-includes-a-template-built-from-templates)).
Sections 1–8 specify the mount-and-seam block compiler that graph union
replaces.

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

When a wrapper's primary lane refers to an exposed child state also present in
the child's primary lane, combining those lanes retains one reference to that
state at its first occurrence. The derived profile preserves the order of all
distinct state identities. This normalization applies only to overlap between
lanes; duplicate entries authored within a profile remain validation errors,
and distinct state definitions remain distinct.

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
`# block:` line per resolved block naming its stable locator and source id, and
the alias chain. Markdown uses the equivalent HTML comment. Headers never
contain a temporary extraction path or canonical host path and point readers
to `.agent-grounds/rhei/composition.lock.json` for machine-readable detail.

A mounted block's task file may **not** open with the metadata-only frontmatter
block a plain workspace task file may carry (§FS-rhei-plan-language.1.4).
Composition rewrites task ids, so an authored `metadata.tasks.<id>` entry in a
block task file would have to be rewritten alongside them; the compiler refuses
it instead, naming the file and the block's own `index.rhei.md` as where block
metadata goes. Carrying such a block through qualification is a contract of its
own and is deliberately outside this one (§8).

### 4.1. Composition lock and per-node provenance

Every curated composition with `use` and every direct `--mount` composition
writes `.agent-grounds/rhei/composition.lock.json`. A legacy single-template
instantiation with neither `use` nor `--mount` remains unchanged. The lock is
UTF-8 JSON with `schema_version: 1`, two-space indentation, lexicographically
sorted object keys, and one trailing LF. It contains exactly four normalized
top-level tables named `sources`, `mounts`, `declarations`, and `nodes`.
Consumers must reject an unsupported version before interpreting its tables;
this release writes the lock but adds no lock reader, inspection command, or
replay operation.

**Sources.** Each `sources` key is `src:sha256:<lowercase-hex>`, computed from
the canonical source identity and effective source-tree digest. The digest
inventory sorts slash-normalized, source-relative paths by UTF-8 bytes and
frames each path, entry kind, and file bytes or symlink target by its byte
length. A symlink contributes both its authored target spelling and the
effective file bytes read through it, under the link's source-relative path,
even when the target is hidden or ignored. Entries sharing a path sort by
entry kind. Resolved host paths are used only to verify tracking and containment,
never as digest input. It excludes directories, timestamps, permissions,
traversal order, temporary extraction paths, and canonical host paths. The source record has a
`locator` describing the requested reference and winning discovery tier, plus
a `revision` describing the strongest identity Rhei could establish. A
relative, named, or repository-relative locator is portable. An explicitly
absolute or user-global locator is retained for diagnosis with
`portable: false`; it is not content identity and is excluded from hashes.
Every locator has `requested`, `tier`, and `portable`; `resolved` is present
when Rhei can express the winning location without a temporary or canonical
host path. Equivalent source identities reconcile diagnostic locators in a
duplicate-free `locators` array, sorted by `(requested,tier,portable,resolved)`
(absent `resolved` sorts first); `locator` is its first member. Every mount also
retains its own winning `locator`, so reconciliation loses no requested path or
portability marking. Locator reconciliation does not relax revision or owned-node
collision checks. Every revision has `kind`, `content`, `status`, and `replay`, plus
the case-specific fields below. Content is `sha256:<lowercase-hex>` when bytes
were available and `null` otherwise. A source id hashes the compact canonical
JSON tuple `[kind,portable-locator,revision-marker,block-path,content]`;
non-portable locator strings occupy `null`, built-ins use template and release
as their marker, Git uses commit, and unavailable fields remain `null`.

Revision records use these honest cases:

- a built-in records the template name, Rhei release, embedded-tree content
  digest, `kind: "built-in"`, `status: "shipped"`, and `replay: "exact"`;
  matching release and
  digest identify immutable shipped bytes;
- clean Git records the commit, repository-relative block path, effective-tree
  digest, `kind: "git"`, `status: "clean"`, and `replay: "exact"` only when every source byte
  used by selection, rendering, or compilation is tracked at that commit and
  no used file or symlink escapes that tree. Symlink targets, including hidden
  targets and intermediate links, must resolve within the source tree and their
  consumed bytes must match the commit; tracking the visible link alone is
  insufficient;
- Git with changed tracked bytes, untracked used bytes, or both records HEAD
  only as context, the effective-tree digest, respectively `status: "dirty"`,
  `"untracked"`, or `"dirty-untracked"`, and `replay: "not-guaranteed"`;
- a non-Git source records the effective-tree digest,
  `kind: "local"`, `status: "unversioned"`, and
  `replay: "not-guaranteed"`; and
- when revision discovery is unavailable, fields that could not be learned
  are explicit `null`, available locator and content data are retained,
  `kind: "unavailable"`, `status: "unavailable"`, and replay is not guaranteed.
  Missing or unexecutable Git is metadata unavailability, not a source error.
  Known non-Git discovery respects Git's configured discovery ceiling; source
  read and render failures remain errors.

A content digest verifies bytes; it does not promise that those bytes can be
retrieved. Exact replay is claimed only for immutable source content under the
built-in or clean-Git conditions above. Source ids are used literally by both
the source table and every mount/declaration chain.

**Mounts.** `mounts` contains `root` for the declaring composition, whose
`alias` is `null`, `chain` is empty, and `encoded` is empty. Every mounted
instance has a distinct key equal to the injective encoded alias chain from
§4, and records its immediate `alias`, full `chain`, `encoded`, and `source`.
Repeated use of one source therefore shares a source id but not a mount key;
nested mounts retain every segment.

**Declarations.** A declaration record is captured after input selection and
rendering and records `source`, slash-normalized source-relative `file`,
`kind`, optional local name, rendered digest, and, where needed, occurrence.
Its key is `decl:<kind>:sha256:<lowercase-hex>`. The digest input is the compact
JSON tuple `[source,kind,file,local,rendered,occurrence]`, using the emitted
source id and recorded canonical rendered digest: strings use JSON escaping,
object keys in a rendered value are recursively sorted, arrays retain semantic
order, and no insignificant whitespace is present. `occurrence` is `null`
unless two otherwise identical unnamed declarations exist; those declarations
receive one-based ordinals in authored order. Host paths, timestamps, and
filesystem traversal order never participate. `file` names the authored file;
single-file plan tasks retain `plan.rhei.md` even when emitted into `tasks/`.

**Nodes.** `nodes` has tables `states`, `tasks`, `profiles`, and `routing`.
Named-node keys are the final flat state, task, or profile ids after
qualification and compatibility lowering. Routing keys are `root`, `rhei`,
`default`, `by_type/<final-qualified-kind>`, or
`overrides/<canonical-rendered-digest>~<duplicate-ordinal>`. The override
digest uses the declaration canonicalization above; its ordinal is one-based
among byte-identical overrides in authored order, so unrelated array insertion
does not change its identity. These keys are computed from final emitted rules,
after level-only expansion and primary-profile folding; each expanded rule keeps
its contributing declaration origins.

Every node value is a sorted, duplicate-free array of origins. An origin
records a `mount`, `declaration`, and `via`; following the declaration's
`source` must reach an existing source record. Origins sort by mount,
declaration, via, then synthesis reason. `via: "declared"` identifies a direct
definition. Qualification and compatibility renaming change only the node
lookup key. Checked terminal coalescing retains every member origin.
Compiler-synthesized profiles and routing entries use `via: "synthesized"`,
record a stable `reason`, and retain an origin for every declaration that
contributed to the derived value. Thus every final state, task, profile, and
routing rule has a complete node-to-declaration-to-source trace; synthesis and
many-to-one lowering never pretend to have a single declaring block.

Canonical table and origin ordering makes the complete lock byte-identical for
the same authored inputs and source bytes on every supported platform. The
ordinary flat workspace remains the execution contract; provenance is an
auditable sidecar rather than block semantics added to runtime commands.

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
The surviving terminal's provenance is the sorted union of every member's
origins; none may be selected as a representative or discarded. One-target
compatibility renames likewise retain the renamed definition's origins.

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
authoring language, exposure of values, defaults, secrets, files, paths,
profiles, task exports, or other identity kinds, conditional/gated seams, and
arbitrary pass expressions are deliberately outside this contract. They remain
linked non-blocking follow-up work.

## 9. Composition by graph union

Composition is **graph union over names as their authors wrote them**. A
template adds its states, transitions, profiles, node kinds, node-policy routes,
prompt templates, scripts and tickets to a machine and a plan that already
exist; what two templates share, they share because both authors wrote the same
name, and nothing is renamed, qualified or alias-encoded to make a union
possible.

The union happens entirely inside `rhei instantiate` and ends in the ordinary
flat files every other command already reads: one `states.yaml`, one plan, one
set of task files. There is no grammar change, no loader change and no
namespace — at runtime a composed plan is indistinguishable from one a person
typed, which is what keeps output predictable
([§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)) and is the reason the union refuses rather
than invents wherever two authors disagree.

Two things carry it, and they are the same mechanism seen from two sides:

- `--into <target>` places a template into a plan that already exists ([§FS-rhei-library.10](rhei-library.spec.md#10---into-placing-a-template-into-a-plan));
- `includes:` in `template.yaml` builds a template out of templates ([§FS-rhei-library.14](rhei-library.spec.md#14-includes-a-template-built-from-templates)), each
  entry optionally placing its tickets `under:` a task of the host.

Nothing is written until the whole union validates. On error the target is
byte-identical to what it was, and the error names both sources of the
disagreement and the field they disagree on.

## 10. `--into`: placing a template into a plan

`rhei instantiate` gains one flag with two forms:

```text
rhei instantiate <template> [inputs] --into <rhei>         # tickets at the rhei's top level
rhei instantiate <template> [inputs] --into <rhei>.<task>  # tickets under that task
rhei instantiate <template> [inputs] --output <dir>        # standalone, unchanged
```

`<template>` resolves through the discovery chain of
[§FS-rhei-templates.1.2](rhei-templates.spec.md#12-the-ancestor-walk-checks-both-names-at-each-level) or is a
path, exactly as it does for `--output`. `--into` resolves its target the way
`rhei new --under` resolves a parent ([§FS-rhei-new.3](rhei-new.spec.md#3-creating-a-ticket)), and it is a flag for the
same reason `--under` is: the positional arguments are the template's inputs.

Rendering, input collection and settings hoisting are the single-template path
unchanged ([§FS-rhei-templates.6.1.2](rhei-templates.spec.md#612-behavior) steps 3–5;
`settings.json` hoists as [§FS-rhei-templates.6.2](rhei-templates.spec.md#62-instantiating-inside-a-panta-project) specifies, to the project
inside one and to the rhei's own settings root outside one). What `--into` adds
is where the result goes:

1. The rendered index's **title and description are dropped** — the host's index
   describes the composition. Its frontmatter unions ([§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter)).
2. `states`, `transitions`, `profiles` and `node_policy.by_type` join the
   target's `states.yaml` under the union rules of [§FS-rhei-library.11](rhei-library.spec.md#11-the-union-rules); `models` joins as a set;
   every other top-level key stays the host's.
3. `prompt_templates/*.md` and `scripts/*` are copied beside the rhei's own,
   under rule 1 of [§FS-rhei-library.11](rhei-library.spec.md#11-the-union-rules). Other bundled files travel as they do for `--output`,
   except `README.md`, which describes the template and stays behind.
4. The template's tickets are written where `rhei new` would write them
   ([§FS-rhei-new.3.1](rhei-new.spec.md#31-where-it-is-written)) with their ids re-parented ([§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter)).
5. One fence comment records the inclusion ([§FS-rhei-library.15.1](rhei-library.spec.md#151-the-fence-comment-is-the-whole-of-provenance)).

The write takes the permanent sibling lock every rewriting command takes
([§FS-rhei-new.4](rhei-new.spec.md#4-ids)), so a union into a rhei with a live `rhei run` serializes
against it rather than racing it, and it keeps the host's bytes and inserts the
template's rendered entries at the end of each block — so a `git diff` after
`--into` shows the added lines and nothing else. The placed tickets are appended
work, which creates no capacity: the project's invocation and spend accounts are
exactly where they were ([§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)).

`--dry-run` prints that diff — one block per inclusion, with its ticket count —
and writes nothing.

### 10.1. The machine the target must have

`--into` requires the target rhei's **effective** state machine to be the
`states.yaml` in its own execution root, because that is the only file a union
can be written into and have the rhei run under it. Under
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution) rule 2 a rhei that omits `**States:**` inherits the
project default wholesale and its own root is **not consulted**, so a
`states.yaml` written into such a rhei's root would be inert. The four cases:

- the index declares `**States:** <name>` and the root file's `name` matches —
  union into that file; no declaration is written or changed;
- the root file exists and the index declares nothing — the union is written and
  `--into` adds `**States:** <that file's name>` to the index in the same write,
  reporting that it did so in the summary;
- there is no file in the root — **refused**, printing *both* remedies, because
  the copy alone changes nothing: `cp <project>/states.yaml <rhei>/states.yaml`
  *and* the `**States:** <name>` line to add to the index, with the note that
  the rhei then stops following the project default;
- the index declares a name the root file's `name` does not match — **refused**,
  naming both. This is a pre-existing load error, not one `--into` introduces.

The declaration-writing case is an **interim** rule of the slice that introduces
`--into`: it exists only because machine resolution is still the resolution
above. When resolution is simplified so that a rhei's own root is consulted
whatever its index declares, this clause and the line it writes go.

The basin ([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)) is never a `--into` target: it holds unfiled
tickets that run under the project default and has no machine of its own to add
to.

## 11. The union rules

Three rules decide every union, and each of them refuses rather than resolves.

### 11.1. Same name, same thing

A state, transition, profile, node kind, prompt template or script that both
sides define must be **identical apart from `description`**. Declaration order
and descriptive prose are not differences; every operative field is. A
difference is an error naming both sources and the differing field, and there is
no namespace, no qualification and no rename to escape it: a template that must
exist twice in one machine takes a prefix as a declared input.

One definition standing for several identically named ones is therefore the
ordinary case of a union rather than an exception to single ownership.

For a **terminal** the rule is not a name test. Two terminals coalesce when
`final: true` holds on both, neither has an outgoing exact transition, and both
have the same effective cancellation role as
[§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names) classifies it — the three refusals of [§FS-rhei-library.7.1](rhei-library.spec.md#71-equivalent-terminal-identities),
carried into the general rule. Keying on the spellings `completed` and
`cancelled` would refuse a template whose terminals are named something else,
and there is a shipped built-in with six terminals and no `completed`. A
template with terminals of its own keeps them: the assembled machine has as many
terminal leaves as its parts have, and the two that coincide coincide.

Nothing in a union ever un-finalizes a terminal. A template meant to be
continued from exposes a non-terminal exit state, and making a terminal
continuable is an edit its **author** makes.

### 11.2. Wildcards stay home

A template's `from: "*"` transition is written into the union with `sources:`
set to that template's own states — a field every consumer already reads
([§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)) — so a template's cancel-from-anywhere edge never
captures another template's states.

The **host's** own wildcards are left exactly as written and span the union: a
rhei that cancels from anywhere cancels the states it took in, and that is the
host's call to have made. The scoped and unscoped forms already coexist in one
valid machine, so this needs no new field.

### 11.3. The host routes

An included template validates standalone, so it carries a whole `node_policy`.
At union only its `by_type` entries survive. Its `root`, `default` and `rhei`
are dropped because the host's stand ([§FS-rhei-states.9](rhei-states.spec.md#9-node-policy)), and its `overrides`
are dropped because they key on `level`, which is a fact about where the host
put the template rather than about the template.

A ticket's kind is its lane ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)), so a template routes
its own tickets through node kinds of its own, and uses the bare `task` kind
only where it means the host's default lane.

### 11.4. Inputs union

Two templates that declare an input of the same name declare **one** input, and
the same name requires the same type. An including template fixes an included
value by declaring the input itself, and its declaration wins over any included
default. Two included defaults that differ with no declaration above them are an
error naming both templates and the input.

`--list-inputs` and `rhei templates` show the union, so an including template is
used exactly as a flat one is, and `--values`, `--set`, `--set-file` and
positional values resolve against the unioned schema unchanged.

An artifact path that crosses templates is therefore an input with a default on
both sides: agreeing on it is giving one value, and no mechanism is needed to
pass data from one part to another.

### 11.5. Chaining two templates is the host's work

A profile's `allowed` set is wholesale and profiles never merge
([§FS-rhei-states.8.2](rhei-states.spec.md#82-per-profile-validation)). So joining two templates into one path takes two things
the host writes, not one: the transition from the first's exit state to the
second's entry state, and a profile whose `allowed` spans both, mapped by
`by_type` to the kind of the tickets that walk the chain. A host-authored
spanning profile resolves the ordinary settings chain and needs no
`transition_limit` of its own ([§FS-rhei-budgets.2.2](rhei-budgets.spec.md#22-transition_limit-on-a-profile)), so there is nothing for a
union to synthesize.

A path across templates is the one thing no single template knows. That is why
it is the host's, written in `states.yaml` after `--into` or in the including
template's own machine.

## 12. Placement: ids, tickets, and frontmatter

Ids in a template are **relative**: `claim`, `review`, `ticket.triage`. Placing
them does four things, each of which `rhei new --under` already does for one
ticket.

**Re-parent the ids.** Placed under `<rhei>.<task>`, every template id gains
that prefix, headings deepen by the parent's level, and every `**Prior:**` and
`**Consumes:**` inside the template that names a template task is rewritten to
the placed id. At a rhei's top level the ids stay as written. Files are named as
`rhei new` names them ([§FS-rhei-new.3.1](rhei-new.spec.md#31-where-it-is-written)): a new three-digit-padded file in
`tasks/` for a top-level ticket, appended to the parent's file for a subtask
because a task file owns its subtree, and inside `## Tasks` in a single-file
rhei.

**Refuse id collisions.** A ticket id already taken in the target is refused
before anything is written:

```text
cannot place ticket '<placed-id>': task id already exists in target
```

A template with tickets can therefore be placed once per parent. Two rounds of
one workflow are not two placements: they are one set of states and two ticket
sets the host authors in them. A template may ship states and no tickets for
exactly this, and standalone it is a valid empty rhei.

**Union the frontmatter.** The template's node kinds join
`structure.nodeKinds`; `maxLevels` becomes the larger of the two, grown to the
depth the placement actually needs; and `metadata.tasks.<id>` entries — visit
budgets, callback data — are re-keyed by the placed ids and added. A key both
sides hold is a collision like any other. A placed task file that opens with its
own metadata-only `metadata.tasks.<id>` block
([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)) is re-keyed in place exactly as the index's
entries are; that point's rule that one key may not appear in both places is
the id collision already refused.

**Leave paths alone.** No artifact path is rewritten. A per-task artifact
therefore carries `{task_id}`, and a rhei-scoped literal path is fine while one
ticket walks the state and is a hazard the moment two do — which [§FS-rhei-library.15.2](rhei-library.spec.md#152-artifact-paths) reports.

### 12.1. Depth

Two re-parentings compose — `under:` inside a template and `--into
<rhei>.<task>` outside it — and both limits are evaluated **once**, on the final
placed ids against the final target:

- `structure.maxLevels` is **grown** to the depth the placement needs, which is
  the frontmatter union above rather than the refusal an ordinary create gets
  ([§FS-rhei-new.3.3](rhei-new.spec.md#33-depth-and-kind));
- ticket depth **4 is a hard ceiling**, because `######` is the deepest heading
  Markdown gives. A placement past it is refused naming the id that overflows.

Where both could fire the ceiling wins, because growing `maxLevels` past 4 is
not a thing a host can be given.

## 13. Placement and ticket identity

A budget identity belongs to a run and never to a template. Placement therefore
does one refusal and one deletion, and reads nothing of the host's:

- **Refuse the source.** A rendered template entry carrying
  `metadata.tasks.<id>.budgetTicketId` is refused before anything is written.
  The scan covers the index's `metadata.tasks` **and** a task file's own
  metadata block, because [§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter) re-keys both. The refusal fires in `--output`
  mode as well as under `--into`.
- **Strip the clone.** The key is removed unconditionally from the cloned entry
  after re-parenting and after the id-collision check, immediately before
  insertion.
- **Touch nothing of the host's.** The host's own `metadata.tasks` entries are
  never read, compared or rewritten.

Whether a placed ticket travels fresh is **not** a question about either
document. It is the project's ledger that decides, and the admission transaction
already asks it ([§FS-rhei-budgets.6.1](rhei-budgets.spec.md#61-the-transaction)): a placed ticket is a genuinely new
identity with a travel bound of its own exactly where the account holds no
binding for the pair (placed full rhei-qualified display id, target metadata
file), and it keeps its history exactly where that pair is bound — including a
ticket deleted and re-placed at the same id, whose binding outlives its document
([§FS-rhei-budgets.5.2](rhei-budgets.spec.md#52-the-journal)). Nothing in placement looks the binding up, and nothing
in placement may assert an identity the account did not settle
([§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)).

The strip loses nobody's history. A source carrying the key has already been
refused; a target ticket's history comes back from the ledger whatever its
document says. Carrying a key across would be the silent failure rather than the
loud one: the engine would rebind the binding's display id, both tickets would
draw one counter, and the **host** ticket's history would be destroyed and
replaced by a fresh bound.

Three mechanisms with three jobs, and placing one template twice stays legal
because of how they divide: the id-collision refusal ([§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter)) stops live
replacement, the ledger's binding stops delete-and-re-place from refreshing
travel, and the project's accounts are what bound total work. Where the
id-collision refusal and the ledger could each claim a case, the refusal wins,
because it fires before anything is written.

## 14. `includes:`: a template built from templates

A template built out of templates lists them in `template.yaml`:

```yaml
name: grounded-ticket
includes:
  - claim
  - supervising-root
  - { template: triage,           under: ticket }
  - { template: review-fix-round, under: ticket }
```

An entry is a bare name, or a mapping with `template:` and an optional `under:`.
The name resolves through the same discovery as `<template>` does, or as a path
relative to the including template, so a library may keep private pieces beside
the template that composes them. A cycle is an error naming the chain.

Instantiation renders every included template with the unioned input values
([§FS-rhei-library.11.4](rhei-library.spec.md#114-inputs-union)), unions them **in list order**, then validates the whole. The including
template's own `states.yaml`, `tasks/` and `index.rhei.md` are the host: the
edges into and out of the included templates, `node_policy.root` and `default`,
the spanning profiles of [§FS-rhei-library.11.5](rhei-library.spec.md#115-chaining-two-templates-is-the-hosts-work), and the inputs whose values it fixes are all
written there.

Every template stands alone, so every template is discovered, listed, validated
and gated exactly as a template is today. A template with `index.rhei.md` or
`plan.rhei.md` lays a rhei with `--output` and joins one with `--into`; no field
declares which, the layout does.

### 14.1. `under:`

`under: <task>` names a task **template-relative**, and the included template's
tickets are placed beneath it exactly as `--into <rhei>.<task>` places them:
`under:` is the `<task>` half of `--into`'s target applied one level in, and it
calls the same re-parenting, the same heading deepening, the same `**Prior:**`
and `**Consumes:**` rewrite, the same writer and the same refusals ([§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter)). There
is no second placement mechanism — if `under:` needed one, `--into
<rhei>.<task>` would be wrong.

It resolves against the task tree **as it stands when the entry is reached**:
the including template's own `tasks/` are the base, and each entry's placements
join it in list order. So `under: ticket` works whether the including template
authored `ticket` itself or an earlier entry brought it. A name not in the tree
yet is an error naming the entry, the id, and the ids that *are* available, so a
forward reference arrives as a message about order rather than as a mystery.
Omitting `under:` places at the including template's top level.

Four consequences, none of them further machinery:

- **The host names included tasks by their placed ids.** A host task whose
  `**Prior:**` is `ticket.triage` reads as it will resolve, so `under:` values
  are part of the including template's own contract and a typo is caught by the
  validation the union already runs at the end.
- **Depth composes and is checked once**, on the final ids, per [§FS-rhei-library.12.1](rhei-library.spec.md#121-depth). A
  template with `triage` under `ticket` puts `ticket.triage` at depth 2
  standalone; `--into release.ticket` makes it `release.ticket.triage` at depth 3
  and grows the host's `maxLevels` accordingly.
- **The same template under two parents is legal.** `{ template: t, under: a }`
  and `{ template: t, under: b }` are different parents and different ids, which
  is "once per parent" with a way to say the parent. Twice under one parent is
  the id collision of [§FS-rhei-library.12](rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter).
- **States do not move.** `under:` is about tickets. The machine is flat, so an
  entry's states, edges, profiles and kinds union identically whatever `under:`
  says, and `metadata.tasks.<id>` entries travel with their tickets and are
  re-keyed by the composed id. An entry that renders no tickets is legal with
  `under:` and is reported as `0 tickets`, so no input value turns a legal
  composition into a silent one.

## 15. Provenance, diagnostics, and what `--into` refuses to combine

### 15.1. The fence comment is the whole of provenance

One comment line is appended to `states.yaml` per inclusion:

```text
# --- agora 1.2.0 src:sha256:… inputs: {…} ---
```

That is all of it. There is no lock file, no generated header, no per-node
provenance record, and no alias-encoded identity: names in the output are the
names the authors wrote, so what a state came from is answered by reading it.

Byte-exact serialization of the fields a union writes — `transition_limit`
included — is what lets `--dry-run` promise a diff of added lines and nothing
else, so it is a property of the union rather than of any lock.

### 15.2. Artifact paths

Two states from two templates declaring the **same literal path with no
per-task variable in it** is a union-time collision and is refused, naming both
states and the path.

One state with a rhei-scoped path walked by **two tickets** is not a union
question and is not refused: whether the second overwriting the first matters is
the author's call — one supervisor writing one plan note per rhei is correct.
`--into` **warns** there, naming the path and both tickets.

### 15.3. Flags `--into` refuses

Each combination is an error naming the pair rather than a meaning invented for
it:

| With `--into` | Why |
|---|---|
| `--output` | two destinations for one instantiation |
| `--execute` | it means `rhei run` on a **new** workspace; the target may already be running, and a second run is not what the flag promises |
| `--keep-on-error` | there is no staging directory to keep: nothing is written until the union validates, and on error the target is byte-identical |
| `--state-machine` | an invocation override; a union is a durable write and must go into the file the target actually runs under |

`--state-machine` keeps its meaning everywhere else.

## 16. Related specifications

- [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
  owns template discovery, input types, rendering, placement, and the CLI shell
  around compilation.
- [§AR-rhei-library](../architecture/rhei-library.spec.md#ar-rhei-library-block-compiler-architecture)
  owns where the union runs, what it reuses, and the typed compiler boundary it
  replaces.
- [§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new)
  owns the ticket writer, the placement rules and the lock that a union reuses.
- [§FS-rhei-budgets](rhei-budgets.spec.md#fs-rhei-budgets-bounded-ticket-travel-project-invocations-and-a-days-spend)
  owns what a placed ticket's travel identity is; [§FS-rhei-library.13](rhei-library.spec.md#13-placement-and-ticket-identity) only refuses and strips.
- [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
  and [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
  own the generated runtime contracts.
