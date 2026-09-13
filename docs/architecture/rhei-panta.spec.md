# AR-rhei-panta: Panta root architecture

This document specifies how Panta — the per-project virtual root above all rheis
([§FS-rhei-panta](../functional-spec/rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)) — is laid out on disk, loaded into a single graph, and
addressed. It realizes the decision recorded in [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis) and extends the
node hierarchy and virtual-root model of [§FS-rhei-plan-language.3](../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints).

## 1. On-disk layout

A Panta is a project directory. It reuses the Directory Workspace machinery
([§FS-rhei-plan-language.1.2](../functional-spec/rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)) one level up: where a Directory Workspace merges
task files into one plan, a Panta merges rheis into one project.

```
panta/                   ← the Panta: a project directory (point the tools here)
  index.panta.md         ← Panta manifest: project title, default **States:**
  auth.rhei.md           ← a single-file rhei, directly in the project dir
  billing/               ← or a rhei as a Directory Workspace
    index.rhei.md
    tasks/
    runtime/             ← this rhei's own per-ticket artifacts
  basin/                 ← optional: project basin rhei for unfiled tickets
  runtime/               ← single-file rhei artifacts + project-level ROLLUPS
```

The Panta is itself the project directory: its rhei entries (each a `*.rhei.md`
single-file plan or a Directory Workspace subdirectory), the optional `basin/`,
and the `runtime/` artifact tree all live directly inside it — there is no
nested folder grouping the rheis. The directory is conventionally named `panta/`
but any name works; it is a Panta because it contains `index.panta.md`. This is
the Directory Workspace shape lifted one level: where a workspace holds
`index.rhei.md` plus task files, a Panta holds `index.panta.md` plus rhei
entries. Per-ticket artifacts live under each rhei's own execution root; a
single-file rhei's root is the project directory, so its artifacts and the
project-level rollups share the project `runtime/` (§6).

`index.panta.md` is the Panta manifest. It plays the role `index.rhei.md` plays
for a workspace: project title, default state-machine declaration, and content
sections; it contains no authored nodes. Rhei discovery scans the project
directory's **immediate children** in deterministic, `/`-normalized order: each
non-hidden `*.rhei.md` file is a single-file rhei, and each non-hidden
subdirectory containing `index.rhei.md` is a Directory Workspace rhei.
Discovery does not descend into other subdirectories — rheis live directly in
the project directory, so a `*.rhei.md` buried in a grouping folder is not
promoted to a rhei. The `runtime/` artifact tree and the reserved `basin/`
directory are skipped (the latter is loaded as the synthetic basin rhei). A
directory is a Panta when it contains `index.panta.md`.

## 2. Load model

Loading a Panta produces one graph rooted at the virtual `panta` node:

1. Parse `index.panta.md` for project title, default states, and content.
2. Discover the rheis within the project directory in deterministic order. Each
   rhei is loaded by the existing single-file or Directory Workspace loader.
3. If `basin/` is present, load it as a synthetic Directory Workspace rhei with
   id `basin`. It has no authored `index.rhei.md`; its files parse as workspace
   task files, inherit the Panta default state declaration, and use `basin/` as
   their execution root. An `index.rhei.md` found anywhere under `basin/` is a
   **load error** naming the file: the basin's manifest is synthetic, so such a
   file is an author who believed the basin was an ordinary Directory
   Workspace, and its tickets would otherwise never load. Skipping it silently
   is the one outcome the basin must not have — unfiled work that disappears
   behind a green `rhei validate` is precisely what [§FS-rhei-panta.4](../functional-spec/rhei-panta.spec.md#4-invisibility) forbids.
4. Synthesize the virtual `panta` root and attach each loaded rhei, including the
   synthetic basin rhei, as a level-1 child.
5. Merge into one task graph and resolve `**Prior:**` across the whole project,
   exactly as workspace task files merge today.

The virtual root is materialized in memory only; it is never written back. A
source map records, for each node, the rhei (and file) that defines it, so
targeted rewrites during transitions still target the owning file — the same
contract `task_sources` provides for workspace task files.

Frontmatter runtime metadata follows the same owning-file contract. On disk, a
rhei's `metadata.tasks.<id>` keys stay rhei-local — a rhei file never learns its
own qualification prefix. At load time each rhei's entries are re-keyed under
project-qualified ids and merged into the project metadata, so counted-loop
visit counters and poll timers resolve against the merged graph's qualified
ticket ids; writes route back through the source map to the owning rhei file
under the rhei-local key.

Every load path yields a Panta-rooted graph. A bare rhei loaded directly — a
`.rhei.md` file or a Directory Workspace with no enclosing `index.panta.md` — is
treated as the single rhei of an **implicit Panta**: the loader synthesizes the
virtual `panta` root and attaches that one rhei as its sole level-1 child, so the
in-memory graph has the same shape whether or not an `index.panta.md` exists. The
implicit Panta has no manifest, so the project default state machine is the
built-in `rhei` machine and the project directory is the source location from
which an inherited declaration would resolve; the single rhei still resolves its
own `**States:**` from its own source (§4).

The implicit Panta's one rhei is not anonymous: it has a derived id (§3), and its
tickets are project-qualified exactly as in a multi-rhei project, so a bare
`auth.rhei.md` exposes ticket ids `auth.1`, artifact paths
`runtime/results/auth.1.md`, and CLI targets `rhei complete auth.1`. The only
structural difference between an implicit and an explicit Panta is the number of
rheis and the presence of a manifest; there is one loader and one graph shape.

## 3. Identity and id namespacing

Ids are dotted paths rooted at Panta. A rhei contributes its id as the prefix for
its tickets:

- A rhei has a single-segment id (`auth`, `billing`). The id is derived from the
  rhei's source location: the file stem for a single-file rhei (`auth.rhei.md` ->
  `auth`) and the directory name for a Directory Workspace rhei (`billing/` ->
  `billing`). This rule is the same for an explicit Panta and for the implicit
  Panta wrapping a bare rhei (§2), so loading `auth.rhei.md` directly yields the
  rhei id `auth`. Where the path carries no last component of its own — `.`,
  `./`, a trailing `..`, or a Directory Workspace named by its own
  `index.rhei.md` — the name comes from where the path *resolves*, so those
  spellings derive the same id as the absolute path
  ([§FS-rhei-panta.6](../functional-spec/rhei-panta.spec.md#6-project-scope-and-command-behavior)); a path that carries a name keeps it, symlinks
  included, and only a path that resolves to no usable directory name — the
  filesystem root — has no id to derive. The same resolution decides which
  project encloses an entry, so a member addressed as `..` still loads through
  its project. A derived id that is
  not a valid single-segment rhei id, or that collides with another rhei or the
  reserved `basin`, is a load/validation error.
- A ticket's project-wide id is its rhei id joined with its rhei-local id:
  rhei-local `1` under rhei `auth` is the project id `auth.1`; rhei-local `1.2`
  is `auth.1.2`.
- A ticket captured without an owning rhei is authored under `basin/`, which is
  loaded as the `basin` rhei, so rhei-local `3` becomes project id `basin.3`.
- Project ids must be unique across the whole Panta. Because the rhei id prefixes
  every ticket beneath it, authors only need uniqueness within a rhei, plus
  unique rhei ids across the project.
- `basin` is a permanently reserved rhei id. A discovered domain rhei with id
  `basin` (for example a `basin.rhei.md` in the project directory) is a
  load/validation error whether or not `basin/` content exists, so the
  synthetic basin rhei can never collide with a domain rhei ([§FS-rhei-panta.2](../functional-spec/rhei-panta.spec.md#2-default-home-for-new-rheis)).

Within a rhei, tickets are authored and validated exactly as today
([§FS-rhei-plan-language.3.4](../functional-spec/rhei-plan-language.spec.md#34-hierarchical-task-consistency)): the rhei-local id space is unchanged, including
`structure.maxLevels` (1–4) counted from the rhei-local level 1. The Panta prefix
is applied at merge time and is not authored into the rhei's task headings.
`**Prior:**` references may be written as project ids to point across rheis;
within a rhei, rhei-local references continue to resolve locally. Merge-time
qualification of a `**Prior:**` follows one rule:

1. An id naming a ticket in the citing rhei is rhei-local and gets the citing
   rhei's prefix. A rhei-local id wins whenever it is ambiguous with a
   project-qualified one.
2. Otherwise a **dotted id whose leading segment is a name** is a cross-rhei
   reference and is left exactly as authored — whether or not the leading
   segment names a real rhei.
3. Every other id (single-segment, or dotted under a numeric lead) is
   rhei-local and gets the citing rhei's prefix.

Rule 2 deliberately does not test the leading segment against the project's
rhei ids. A mistyped rhei name is shaped like a cross-rhei reference and reads
like one to the author; prefixing it with the citing rhei would make it
surface — in validation errors, `rhei list`, and every renderer — as an id that
appears in no file and matches no search. An unresolved reference is a
validation error either way ([§FS-rhei-validate.4.1](../functional-spec/rhei-validate.spec.md#41-unresolved-prior-references)), so the only thing
qualification could add is a wrong id.

## 4. State-machine binding

The state machine is a property of the **rhei**, defaulted by the project.
`index.panta.md` supplies the project *default* machine — the built-in `rhei`
machine when the manifest declares none — and every rhei that does not declare
its own `**States:**` runs under that default, as do the synthetic `basin` rhei
and the Panta root's node policy. A rhei that declares its own `**States:**`
runs under the machine it names: a state machine here is the *process* — state
vocabulary, transitions, per-state agent bindings, artifact contracts — and
different workstreams in one project legitimately follow different processes
(every instantiated template is one). The merge records which rhei declared
which machine ([§DA-per-rhei-state-machines](../decisions/architectural/per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest)); the graph stays one merged,
project-qualified task list, and every consumer resolves a ticket's machine
through its owning rhei. Inheritance stays a default, never a merge: a rhei
that omits `**States:**` takes the project default wholesale, and machines are
never combined or namespaced.

Cross-rhei semantics need no shared vocabulary. The one computation where two
machines meet is readiness: a `**Prior:**` into another rhei is satisfied when
the target ticket is terminal-and-not-cancelled **under the target's own
machine** ([§FS-rhei-panta.6.1](../functional-spec/rhei-panta.spec.md#61-readiness-and-rhei-next)). Every other operation — validation of a
ticket's state, transition legality, completion-target selection, artifact
contracts, agent bindings — is a per-ticket question answered by the owning
rhei's machine. The resolved source of each machine is surfaced in diagnostics
— CLI override path, rhei declaration, `index.panta.md` declaration, or
built-in `rhei` fallback.

A machine's *definition file* resolves per declaration. For a rhei's own
declaration that differs from the project default, the rhei's execution root
`states.yaml` resolves it first when its `name:` matches — the shape every
instantiated template ships. Otherwise, and
for a custom manifest default, the project root's `states.yaml` resolves first;
when that is absent or names a different machine, a `states.yaml` in a
discovered rhei's root whose declared `name:` matches may resolve it — but only
a **unique** match. The explicitly declared built-in project default
`**States:** rhei` is the exception: only a matching project-root file replaces
the built-in machine. A member that inherits or restates that explicit default
cannot make a member-root file the project default. When the effective
declaration is omitted after inheritance, automatic project-root and member-root
files are ignored and built-in `rhei` is used. When several candidate roots
hold files declaring a custom default's
name, resolution is ambiguous and errors, naming the candidates and the fixes
(move the definitive file to the project root, or pass
`--state-machine`): a stale copy silently driving tickets would be far worse
than asking once. A candidate that fails to load is likewise an error, not a
silent non-match — it would otherwise surface as a misleading "no states file
found". Name-match resolution is file resolution, not shadowing, and it is
what lets a project initialized over existing plans by `rhei init` keep each
machine file where its rhei always kept it ([§FS-rhei-init.2](../functional-spec/rhei-init.spec.md#2-behavior)).

`--state-machine <path>` stays a whole-scope override: it replaces resolution
for every rhei in scope, and errors when any in-scope rhei declares a machine
name different from the override file's `name:` — an override that silently
reinterpreted one rhei's states under another rhei's process would corrupt
exactly the runs the flag exists to debug.

The state-machine profile that previously resolved the level-0 `rhei` root now
resolves the `panta` root: Panta resolves through `node_policy.root`. A rhei node
resolves through the dedicated `node_policy.rhei` key when declared; when it is
omitted, the rhei is a pure structural rollup with no profile-driven state. The
reserved names `panta` and `rhei` may not appear in `structure.nodeKinds` or as
`by_type` keys. Panta carries no stored state; any project-level status is a
rollup derived from its rheis ([§FS-rhei-panta.3](../functional-spec/rhei-panta.spec.md#3-one-unified-view)). The full resolution and
validation rules are specified in the states spec node-policy section
([§FS-rhei-states](../functional-spec/rhei-states.spec.md#fs-rhei-states-rhei-states-specification)).

## 5. Execution root and per-rhei runtime

Each rhei keeps its own **execution root**, so artifact and relative-link
resolution is **per rhei**, not per project. A rhei's execution
root is defined exactly as a standalone plan's is today
([§FS-rhei-plan-language.3.10](../functional-spec/rhei-plan-language.spec.md#310-state-artifact-contracts)): the containing directory for a Single-File Plan,
the workspace directory for a Directory Workspace rhei. State-machine `inputs`,
`outputs`, `> **Result:**` paths, and content links inside a rhei all resolve
against that rhei's root.

Result and artifact paths use the project-qualified ticket id
(`runtime/results/auth.1.md`), so single-file rheis — which share the project
directory as their execution root — never collide on artifact names. A rhei
authored as a workspace directory gets a fully isolated runtime tree; that is
the escape hatch when an operator wants per-rhei isolation rather than the shared
project `runtime/`.

The project `runtime/` therefore holds two coexisting things: the per-ticket
artifacts of the single-file rheis (whose execution root is the project
directory) and the cross-rhei rollups — aggregate cost accounting, project-level
snapshots, and the unified visualization. Distinct subtrees
(`runtime/results/`, `runtime/accounting/`, …) and project-qualified ids keep
them from colliding, exactly as a standalone plan coexists its results and
accounting in one `runtime/`.

## 6. Command scope mechanics

`rhei run` resolves the single project state machine (§4) once and applies it to
every ticket it transitions; cross-rhei dependency readiness judges the prior
ticket against that same machine and requires a successful terminal state
(`final: true` and a normalized state name other than `cancelled`). This is the
same predicate normal in-rhei scheduling uses; implementations may share one
predicate, resolve the prior directly, or read an exported/cached readiness
result, but only when that result is behaviorally equivalent and fresh. A
dependency fails closed — the dependent stays blocked — whenever the prior
state or its state-machine meaning cannot be resolved reliably. Per-rhei
machines — and with them, readiness judged against the prior's own machine —
are deferred (§4). The ready-set
scan, claim selection, and rollup all walk the single merged graph (§2), so
project-wide is the natural default and `--rhei` is a filter applied to candidate
nodes after the merge. The fan-out commands — `rhei run` and `rhei reset` —
report the resolved scope and the set of rheis they will touch before acting;
other mutating commands act without a scope report ([§FS-rhei-panta.6](../functional-spec/rhei-panta.spec.md#6-project-scope-and-command-behavior)).

## 7. Invisibility surface

Panta is excluded from default output by the layers that present nodes:
listing, `rhei next` claim selection, rendering, and visualization treat rheis as
the top level ([§FS-rhei-panta.4](../functional-spec/rhei-panta.spec.md#4-invisibility)). Tooling may expose the root behind an explicit
opt-in. Because the root is virtual and derived, no command may claim,
transition, complete, cancel, or reset it.

The synthetic `basin` rhei is de-emphasized rather than excluded: it loads
after every discovered rhei, so its tickets order last in the merged graph and
in every surface that walks it, but it participates normally in readiness,
claim selection, execution, and rollup. Rendering it in a visually
de-emphasized form is deferred presentation work tracked on the roadmap; the
de-emphasis is presentational only and never alters scheduling
([§FS-rhei-panta.4](../functional-spec/rhei-panta.spec.md#4-invisibility)).

## Related Specifications

- [Panta (functional)](../functional-spec/rhei-panta.spec.md) [§FS-rhei-panta](../functional-spec/rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)
- [Plan Language](../functional-spec/rhei-plan-language.spec.md) [§FS-rhei-plan-language.3](../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints)
- [Panta Root Decision](../decisions/architectural/panta-root.md) [§DA-panta-root](../decisions/architectural/panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis)
