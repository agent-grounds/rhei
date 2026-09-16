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
`Binding`, `Seam`, `Pass`, `CompatibilityMap`, alias-chain, and owned-reference
values. It exposes total operations for recursive expansion, sequencing,
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
2. Resolve typed inputs and render each manifest's own files in isolation.
3. Parse plan and state fragments into shared typed representations.
4. Recursively expand `use`, retaining resolved source and alias-chain data.
5. Validate aliases, cycles, ports, binds, seams, passes, ownership, and
   compatibility maps before lowering.
6. Qualify every owned definition and typed reference with the injective alias
   encoding in [§FS-rhei-library.4](../functional-spec/rhei-library.spec.md#4-qualification-and-generated-workspace).
7. Derive the outer control profile, retain internal lanes/fan-out, lower data
   passes and compatibility identities, and merge settings.
8. Emit one plan tree, one flat state machine, private mounted files, and short
   provenance headers.
9. Materialize through the CLI and run existing workspace/state validation.

No output from an earlier stage is treated as valid final output. `--dry-run`
executes the same stages through validation in scratch; `--execute` begins only
after stage 9 succeeds.

## 4. Ownership and total merge

Every parsed definition carries an owner: the root identity or one alias chain.
References resolve against typed owner tables before qualification. Local
definitions receive the same owner's prefix; declared cross-block endpoints
resolve through the seam/binding tables; external settings references remain
external. An unresolved or multiply owned reference is rejected.

The alias encoder and mounted-path rebasing are injective, so two valid owned
definitions never collide. Merge is therefore total after validation: maps can
be inserted without winner rules, source-order shadowing, or text-dependent
renames. A collision at that stage is an internal invariant failure and must be
reported as such rather than resolved by overwriting.

Project settings still use their existing project-values-win policy after
block-owned ids and references are qualified. Compatibility lowering is an
explicit checked rename at the root, not an exception to ownership.

## 5. Runtime invariants

The compiler's output must pass every ordinary plan, state, profile,
node-policy, settings, artifact, and reference validator. Existing commands see
only that output and preserve their current behavior. In particular:

- state-file passes lower to existing state input/output paths and enforcement;
- task-export passes lower to existing `Provides`/`Consumes` metadata;
- internal transitions, conditions, gates, callbacks, and fan-out retain their
  typed forms; and
- execution roots remain those of the one generated rhei.

Composition adds no project-global readiness rule and no cross-member runtime
channel. Future authoring languages, catalogs, or provenance stores must lower
through this same typed boundary if introduced.
