# AR-rhei-library: Block Compiler Architecture

Block composition is a front-end compile step between template authoring and
ordinary Rhei workspace parsing. It realizes
[§FS-rhei-library](../functional-spec/rhei-library.spec.md#fs-rhei-library-composable-blocks)
without adding a block-aware runtime.

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
