# AR-rhei-library: Block Compiler Architecture

Block composition is a front-end compile step between template authoring and
ordinary Rhei workspace parsing. It realizes
[§FS-rhei-library](../functional-spec/rhei-library.spec.md#fs-rhei-library-composable-blocks)
without adding a block-aware runtime.

Sections 1-5 specify that compiler. Section 6 specifies the **graph union** that
replaces it: one module behind `rhei instantiate` that unions authored names
rather than qualifying them, reusing the ticket writer and the validator instead
of lowering through a typed core of its own.

## 1. Boundary

The pipeline has three layers:

```text
template.yaml + rendered block files
                  |
                  v
rhei_core::blocks typed graph and compiler
                  |
                  v
ordinary plan workspace + flat states.yaml
                  |
                  v
existing parser, validator, commands, and runtime
```

The CLI owns existing template discovery, input resolution, MiniJinja
rendering, output placement, project settings hoisting, final validation, and
optional execution. It resolves and renders each block in isolation, gives the
typed forms to the compiler, materializes the result, then invokes the existing
validator. Runtime modules never resolve `use`, mount aliases, seams, binds, or
data ports.

One invocation produces one rhei and one state machine. It does not combine
Panta members or weaken the per-rhei state-machine boundary in
[§AR-rhei-panta.4](rhei-panta.spec.md#4-state-machine-binding).

## 2. Typed core

`rhei_core::blocks` owns typed `Block`, `Mount`, `ControlPort`, `DataPort`,
`Exposure`, `Binding`, `Seam`, `Pass`, `CompatibilityMap`, alias-chain,
public-table, and owned-reference values. Public tables remain separated by
state, task, agent, model, MCP-server, and skill kind. It also owns the typed
source, mount, declaration, and node provenance store carried by
`CompiledBlock`. Source resolution supplies identity before lowering, and
selected/rendered fragments attach declaration identities before
qualification. The module exposes total
operations for recursive expansion, exposure resolution, sequencing,
qualification, routing derivation, compatibility lowering, and compilation.
The textual YAML surface deserializes into those values; an optional future
builder API would construct the same values rather than sit above the YAML.

The state-machine parser and validator representations are shared with the
compiler as the state-fragment representation. The compiler must not maintain a
second state schema that can drift from runtime validation. Plan nodes likewise
enter qualification as parsed task structures, not as markdown search/replace.

## 3. Compile stages

Compilation is ordered:

1. Resolve the root block or ordered direct mounts.
2. Parse the static input schema, resolve typed inputs, select the opt-in
   declaration groups (§FS-rhei-library.1.1), and render each manifest's own
   files in isolation. Only the selected groups pass through MiniJinja.
3. Parse plan and state fragments into shared typed representations.
4. Recursively expand `use`, retaining resolved source and alias-chain data.
5. Validate aliases, cycles, ports, binds, seams, passes, ownership, typed
   exposure tables, and compatibility maps before lowering.
6. Resolve exposed local or immediate-child identities into typed public
   tables, attach declaration provenance, then qualify every owned definition,
   its provenance, and typed references with the injective alias encoding in
   [§FS-rhei-library.4](../functional-spec/rhei-library.spec.md#4-qualification-and-generated-workspace).
7. Derive the outer control profile, retain internal lanes/fan-out, lower data
   passes and compatibility identities, merge settings, and carry or union
   provenance through each rewrite.
8. Emit one plan tree, one flat state machine, private mounted files, stable
   provenance headers, and the composition lock from the same typed store.
9. Materialize all emitted files through the CLI's existing transaction and
   run existing workspace/state validation.

No output from an earlier stage is treated as valid final output. `--dry-run`
executes the same stages through validation in scratch; `--execute` begins only
after stage 9 succeeds.

## 4. Ownership and total merge

Every parsed definition carries an owner: the root identity or one alias chain.
References resolve against typed owner tables before qualification. Local
definitions receive the same owner's prefix; declared cross-block endpoints
resolve through the seam/binding tables; declared public identity references
resolve through the immediate child's same-kind public table; external settings
references remain external. An unresolved or multiply owned reference is
rejected. Each wrapper constructs a new public table only from its own
declarations, so child tables are not transitively visible.

The alias encoder and mounted-path rebasing are injective, so two valid owned
definitions never collide. Merge is therefore total after validation: maps can
be inserted without winner rules, source-order shadowing, or text-dependent
renames. A collision at that stage is an internal invariant failure and must be
reported as such rather than resolved by overwriting.

Project settings still use their existing project-values-win policy after
block-owned ids and references are qualified. Compatibility lowering is an
explicit checked rename at each wrapper boundary after exposure resolution.
Exposure changes the public suffix used by qualification but never coalesces
definitions. The only many-to-one
exception is checked terminal equivalence (§FS-rhei-library.7.1): compare
effective operative contracts before removing any duplicate definition, then
rewrite every typed reference with the same visitor and union every member's
origin records. Qualification, settings/profile/routing synthesis, and
one-target compatibility renames move provenance with their definitions; they
may neither manufacture one owner nor drop a contributor. All other merges
remain injective and collision-refusing.

## 5. Runtime invariants

The flat schema has two general, backward-compatible properties used by the
compiler: cancellation roles (§FS-rhei-states.1.4) and scoped wildcard sources
(§FS-rhei-transitions.4.6). Ordinary consumers interpret these properties without
discovering blocks. Qualification records inferred cancellation before renaming
and scopes wildcards without turning them into exact forward edges. This is the
bounded exception to an entirely unchanged flat schema/runtime.

The compiler's output-side provenance store serializes to
`.agent-grounds/rhei/composition.lock.json` in the same transaction as the
ordinary files. The lock is not part of the runtime schema. The compiler's flat
output must pass every ordinary plan, state, profile, node-policy, settings,
artifact, and reference validator. Existing commands ignore the sidecar, see
only that flat output, and preserve their current behavior. Older workspaces
without a lock remain valid. In particular:

- state-file passes lower to existing state input/output paths and enforcement;
- task-export passes lower to existing `Provides`/`Consumes` metadata;
- internal transitions, conditions, gates, callbacks, and fan-out retain their
  typed forms; and
- execution roots remain those of the one generated rhei.

Composition adds no project-global readiness rule and no cross-member runtime
channel. This ticket adds no lock reader, inspection command, or replay
operation. Future authoring languages, catalogs, or additional provenance
stores must lower through this same typed boundary if introduced.

## 6. The union architecture

Graph union replaces the compiler above. It is not a second compiler: it is one
module behind `rhei instantiate` that reuses the ticket writer, the lock and the
validator that already exist, and its output is what the union of two authored
files would have been if a person had typed it.
[§FS-rhei-library.9](../functional-spec/rhei-library.spec.md#9-composition-by-graph-union)

```text
template.yaml (+ includes:) + rendered template files
                  |
                  v
templates_union: render each part, union the graph, place the tickets
                  |
                  v
the host's own flat files, with the union's lines inserted
                  |
                  v
existing parser, validator, commands, and runtime
```

### 6.1. Where it runs and what it owns

`templates_union`, under `crates/rhei-cli/src/cli/`, replaces
`rhei_core::blocks` and the `templates_blocks_*` CLI surface. It sits behind
`rhei instantiate` and nowhere else; no runtime module knows a union happened,
because the artifact of a union is an ordinary plan.

It owns exactly four things, and every one of them is a decision about whether
two authored definitions agree:

1. **Equality under rule 1** — whether two same-named states, transitions,
   profiles, kinds, prompt templates or scripts are the same thing, ignoring
   `description` and declaration order, and the terminal clause that decides two
   terminals by role rather than by spelling
   ([§FS-rhei-library.11.1](../functional-spec/rhei-library.spec.md#111-same-name-same-thing)).
2. **Scoping** — deriving the `sources:` set of a template's own `from: "*"`
   rule from that template's own states
   ([§FS-rhei-library.11.2](../functional-spec/rhei-library.spec.md#112-wildcards-stay-home)).
3. **Projection** — keeping `node_policy.by_type` and dropping `root`,
   `default`, `rhei` and `overrides`
   ([§FS-rhei-library.11.3](../functional-spec/rhei-library.spec.md#113-the-host-routes)).
4. **Re-parenting** — rewriting relative ids, heading depth, `**Prior:**`,
   `**Consumes:**` and `metadata.tasks` keys against a parent
   ([§FS-rhei-library.12](../functional-spec/rhei-library.spec.md#12-placement-ids-tickets-and-frontmatter)).

Nothing else is its own. The union holds no second state schema: state fragments
enter as the parser's and validator's shared representation, and plan nodes
enter as parsed task structures, never as markdown search and replace — the same
rule [§AR-rhei-library.2](rhei-library.spec.md#2-typed-core) sets for the compiler, and for the same reason.

### 6.2. One re-parenting, two callers

`under:` in `includes:` and `--into <rhei>.<task>` are one function called
against different parents, and that is a load-bearing property rather than an
implementation convenience: it is what makes "a template works at any level" a
fact about one code path instead of a promise two code paths have to keep
([§FS-rhei-library.14.1](../functional-spec/rhei-library.spec.md#141-under)).
Composing the two — a template whose entry carries `under:`, instantiated
`--into <rhei>.<task>` — is the same function applied twice, and both depth
limits are evaluated once on the result rather than at each application
([§FS-rhei-library.12.1](../functional-spec/rhei-library.spec.md#121-depth)).

A second placement mechanism for `under:` would be the defect this architecture
exists to prevent, so a reviewer should read any new re-parenting code path as a
design error rather than as an optimization.

### 6.3. What it reuses rather than restates

| Concern | Owner it reuses |
|---|---|
| where a ticket's file goes, and its three-digit name | [§FS-rhei-new.3.1](../functional-spec/rhei-new.spec.md#31-where-it-is-written) |
| the permanent sibling lock a rewriting command takes | [§FS-rhei-new.4](../functional-spec/rhei-new.spec.md#4-ids) |
| discovery, input resolution, rendering, settings hoisting | [§FS-rhei-templates.6.1.2](../functional-spec/rhei-templates.spec.md#612-behavior) |
| the byte discipline a diff of added lines depends on | [§FS-rhei-new.3.1](../functional-spec/rhei-new.spec.md#31-where-it-is-written) |
| frontmatter rewriting | [§FS-rhei-plan-language.1.2](../functional-spec/rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency) |
| whether a placed ticket travels fresh | [§FS-rhei-budgets.6.1](../functional-spec/rhei-budgets.spec.md#61-the-transaction) |

The last row is the one worth stating as architecture rather than as reuse: the
union never queries the ledger. It refuses a source that declares an identity
and strips the key from the clone, and the admission transaction answers the
identity question later, from the binding it already holds
([§FS-rhei-library.13](../functional-spec/rhei-library.spec.md#13-placement-and-ticket-identity)). A union that read the ledger would be a second
authority on travel.

### 6.4. Transactional shape

The union is **all-or-nothing against the target**. Every part is rendered,
unioned and placed in memory; the whole result is validated; only then is
anything written, under the lock, as one insertion per block. There is no
staging directory, which is why `--keep-on-error` has nothing to keep and is
refused ([§FS-rhei-library.15.3](../functional-spec/rhei-library.spec.md#153-flags---into-refuses)), and why a refusal leaves the target
byte-identical without a rollback path to get wrong.

`--dry-run` runs the same stages and prints the insertion it would have made.
Byte-exact serialization of every field the union writes is therefore a property
the diff depends on, rather than a property of a lock file
([§FS-rhei-library.15.1](../functional-spec/rhei-library.spec.md#151-the-fence-comment-is-the-whole-of-provenance)).

### 6.5. What the flat boundary keeps

The two flat-schema properties [§AR-rhei-library.5](rhei-library.spec.md#5-runtime-invariants) names — cancellation roles
([§FS-rhei-states.1.4](../functional-spec/rhei-states.spec.md#14-reserved-state-names)) and scoped wildcard sources
([§FS-rhei-transitions.4.6](../functional-spec/rhei-transitions.spec.md#46-wildcard-semantics)) — are what the union writes with instead of
qualification, and they are the only runtime properties it depends on. There is
no alias encoding, no `composition.lock.json`, no `runtime/blocks/`, and no
generated header, so there is no sidecar for a consumer to learn and no second
name for a state. Provenance is one comment line in `states.yaml` per inclusion,
which the YAML parser discards and a reader does not.

One invocation still produces one rhei and one state machine, and a union still
does not combine Panta members or weaken the per-rhei state-machine boundary of
[§AR-rhei-panta.4](rhei-panta.spec.md#4-state-machine-binding) — `--into` writes into one rhei's own machine, and a rhei
that has none is refused rather than given the project's
([§FS-rhei-library.10.1](../functional-spec/rhei-library.spec.md#101-the-machine-the-target-must-have)).
