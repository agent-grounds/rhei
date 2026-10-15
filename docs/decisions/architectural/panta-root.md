# DA-panta-root: Panta is the per-project virtual root above all rheis

## Status

proposed

## Context

Rhei has no entity above an individual rhei. A plan is loaded by path, executed,
and rendered in isolation; there is no default home for a newly created rhei, no
registry of the rheis in a project, and no single graph that spans them. This
limits the monitoring and predictability goals — an operator cannot see "what is
everything in this project doing" from one root, only one plan at a time.
[§GOAL-rhei-outcomes](../../goals.md#goal-rhei-outcomes-goals)

The plan language already models a virtual level-0 `rhei` root that "denotes the
plan itself," is not authored in markdown, is not persisted as a task node, and
derives its status from authored task nodes. [§FS-rhei-plan-language.3](../../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints) That
established pattern — a derived, non-authored root that owns rollups and is
excluded from claim/transition/complete — is exactly what a cross-rhei container
needs, one level higher.

## Decision

Introduce **Panta**: a single, per-project, *virtual* root node that sits above
all rheis and their tickets. It is the default place rheis are added and the
single anchor for a project-wide unified view. The name follows *panta rhei* —
the still root that contains all the flows. [§FS-rhei-panta](../../functional-spec/rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis)

## 1. Position in the hierarchy

The node hierarchy gains one level at the top:

```
Panta            level 0   kind `panta`   (virtual, one per project)
└── Rhei         level 1   kind `rhei`    (a plan / flow)
    └── Task     level 2+  kind `task`…   (a ticket — a unit of work)
```

The former per-plan virtual `rhei` root is promoted to a first-class level-1
node: a rhei is now *a node among siblings* under Panta rather than the top of
its own isolated tree. `panta` becomes the new reserved, virtual root kind and
takes over the role previously held by the level-0 `rhei` root
([§FS-rhei-plan-language.3](../../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints)): not authored, not persisted, no `**State:**`,
`**Prior:**`, `**Assignee:**`, or `> **Result:**`, and never claimed,
transitioned, completed, cancelled, or reset.

## 2. One unified graph, built by merge

Panta reuses the Directory Workspace merge mechanism rather than inventing a new
structure. Today, task files under `tasks/` merge into one global graph at load
([§FS-rhei-plan-language.1.2](../../functional-spec/rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)); under Panta, the rheis in a project merge into one
graph the same way. Consequences that fall out of the merge:

- `**Prior:**` dependencies resolve **across rheis**, not just within one.
- Status rolls up Panta ← rheis ← tickets through one uniform tree walk.
- A project loads as one graph rooted at Panta instead of N disconnected plans.

This is the concrete meaning of "unified root node" as opposed to a registry
that only references independent plans (see Alternatives).

## 3. Default container for new rheis

Creating a rhei without naming a parent attaches it under Panta. Panta is the
implicit default parent — analogous to `/` in a filesystem — so "add a rhei"
needs no location argument. A loose ticket created with no rhei lands in the
reserved synthetic `basin` rhei rather than directly under Panta, so unfiled
work has a home while the Panta -> rhei -> ticket hierarchy stays intact.

## 4. Invisible by construction

Panta has no authored heading and no node file of its own, so there is nothing
to hide: default listings, claim selection, and monitoring present rheis as the
top level. Tooling exposes the root only behind an explicit opt-in
(e.g. `--show-root`). Its status is always derived from its rheis, never stored.

## Alternatives considered

- **Registry by reference.** A singleton index that lists independent plan files
  and caches a per-plan rollup, leaving each plan a self-contained isolated tree.
  Smaller and non-invasive, but it cannot express cross-rhei dependencies or a
  true single-graph rollup without re-deriving structure on every read. Rejected
  in favour of the unified graph, which is a direct reuse of the existing merge
  pattern and gives cross-rhei dependencies for free.

- **A real, authored Panta heading.** Author Panta as an explicit top-of-file
  node. Rejected because it breaks the "root is derived, not authored" invariant
  the language already relies on ([§FS-rhei-plan-language.3](../../functional-spec/rhei-plan-language.spec.md#3-semantic-constraints)), and would force the
  root to carry state it should only ever roll up.

## Consequences

- **Clean break, no migration.** Promoting the top authored level from task to
  rhei reinterprets existing plans. Backward compatibility is explicitly out of
  scope for this decision; no migration path is provided.
- **ID namespacing.** A ticket's project-wide identity is its rhei id joined with
  its rhei-local task id (a dotted Panta path). This yields cross-rhei
  uniqueness without authors hand-coordinating ids, but the exact id-extension
  rules and grammar productions are deferred to [§AR-rhei-panta](../../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture).
- **State-machine profile.** The state machine's root profile semantics move from
  the `rhei` root to the `panta` root; rheis gain their own node profile. The
  binding is specified in [§AR-rhei-panta](../../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture) and the states spec follow-up.
