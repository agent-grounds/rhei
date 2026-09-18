# FS-rhei-language-reference: Rhei Language Reference

Rhei must have one canonical language-reference entry point so humans and
agents can quickly answer "what files and syntax make up a valid Rhei
workflow?" without reconstructing the model from scattered feature specs. This
supports readable, reviewable plans and predictable execution. [§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)

This page is the entry point for the Rhei language surface. It does not replace
the narrower normative specs; it maps each language surface to the document
that owns it.

## 1. Language Surfaces

Rhei has four user-authored language surfaces. The first — plan and project
markdown — spans several file kinds; the file-kind map below names each one,
its role, and the spec that owns its grammar and behavior:

| File kind | Role | Owner |
|-----------|------|-------|
| `index.panta.md` | Panta project manifest: title, optional default `**States:**`, content; no authored nodes | [§FS-rhei-panta.1](rhei-panta.spec.md#1-what-panta-is), [§FS-rhei-plan-language.1.5](rhei-plan-language.spec.md#15-panta-project) |
| rhei entry (in the project dir) | One rhei per entry — a `*.rhei.md` or a Directory Workspace — discovered at project scope | [§FS-rhei-panta.1](rhei-panta.spec.md#1-what-panta-is), [§FS-rhei-plan-language.1.5](rhei-plan-language.spec.md#15-panta-project) |
| `*.rhei.md` | Single-File Plan: a rhei with its `## Tasks` inline | [§FS-rhei-plan-language.1.1](rhei-plan-language.spec.md#11-single-file-plan-1-agent-or-low-concurrency) |
| `index.rhei.md` + `tasks/**/*.md` | Directory Workspace rhei: manifest plus merged workspace task files | [§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency) |
| `basin/` task files (optional) | Unfiled tickets loaded as the reserved synthetic `basin` rhei | [§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis), [§FS-rhei-plan-language.1.5](rhei-plan-language.spec.md#15-panta-project) |

A bare rhei — a lone `*.rhei.md` or workspace with no enclosing
`index.panta.md` — loads as a one-rhei project. Load order, id namespacing,
execution roots, and state-machine binding for all of the above are specified in
[§AR-rhei-panta](../architecture/rhei-panta.spec.md#ar-rhei-panta-panta-root-architecture). The `**States:**` declaration in these files resolves to
`states.yaml`, which belongs to the state-machine surface below — not to this
map.

The remaining three surfaces:

- State machines: `states.yaml`. Owned by [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) and
  [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification).
- Templates and composable blocks: template directories with `template.yaml`
  plus rendered plan and state files. Template discovery, rendering, and
  single-template instantiation are owned by
  [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification);
  recursive mounts, ports, seams, qualification, and lowering are owned by
  [§FS-rhei-library](rhei-library.spec.md#fs-rhei-library-composable-blocks).
  Input-selected declaration groups are owned by §FS-rhei-library.1.1, and
  checked terminal equivalence by §FS-rhei-library.7.1.
- Execution references: agent, model, MCP server, skill, snapshot, and program
  references. Owned by [§FS-rhei-agents](rhei-agents.spec.md#fs-rhei-agents-rhei-agents-specification), [§FS-rhei-programs](rhei-programs.spec.md#fs-rhei-programs-rhei-program-states-specification), and
  [§FS-rhei-snapshots](rhei-snapshots.spec.md#fs-rhei-snapshots-rhei-session-snapshots-specification).

The plan and project markdown surface is the primary source of truth for project
membership, task state, dependencies, hierarchy, assignees, and result links.
State machines constrain which states and transitions are legal. Cancellation roles (§FS-rhei-states.1.4) and scoped wildcard source sets
(§FS-rhei-transitions.4.6) are ordinary flat-machine properties. Templates are a
preprocessing layer that materializes ordinary plan markdown and optional state
machines before runtime parsing.

## 2. Reading Path

Use this order when learning or auditing the language:

1. Read [§FS-rhei-authoring](rhei-authoring.spec.md#fs-rhei-authoring-rhei-plan-language-usage-guide) for practical authoring patterns.
2. Read [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) for the formal markdown grammar and semantic
   constraints, including Panta Project layout and the checked
   `**Provides:**` / `**Consumes:**` handoff contract
   ([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)).
3. Read [§FS-rhei-panta](rhei-panta.spec.md#fs-rhei-panta-panta-the-project-root-above-all-rheis) for project-root behavior and command scope.
4. Read [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) for the state-machine schema and default states.
5. Read [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification) when a workflow depends on explicit transition
   rules, callbacks, visits, polling, or artifact enforcement.
6. Read [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) when the authored source is a reusable template
   rather than a concrete plan workspace.
7. Read [§FS-rhei-library](rhei-library.spec.md#fs-rhei-library-composable-blocks)
   when one template mounts reusable blocks or `rhei instantiate` composes
   several blocks directly.

Command specs such as [§FS-rhei-validate](rhei-validate.spec.md#fs-rhei-validate-rhei-validate), [§FS-rhei-next](rhei-next.spec.md#fs-rhei-next-rhei-next), [§FS-rhei-transition-cmd](rhei-transition-cmd.spec.md#fs-rhei-transition-cmd-rhei-transition),
[§FS-rhei-complete](rhei-complete.spec.md#fs-rhei-complete-rhei-complete), and [§FS-rhei-run](rhei-run.spec.md#fs-rhei-run-rhei-run) define command behavior over the language;
they are not the primary grammar reference.

## 3. Ownership Rules

Language changes must preserve a single discoverable entry point:

- New syntax in plan markdown belongs in [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification) and must be
  linked from this page.
- Task read boundaries are authored with `**Excludes:**`; their syntax and
  validation are owned by [§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions), while prompt and process enforcement are owned by
  [§FS-rhei-agents.3](rhei-agents.spec.md#3-prompt-composition).
- Adding, removing, or renaming a user-authored project or rhei file kind or
  directory must update the file-kind map in §1 in the same change as the
  owning spec edit.
- New state-machine fields belong in [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification) or [§FS-rhei-transitions](rhei-transitions.spec.md#fs-rhei-transitions-rhei-transitions-specification)
  and must be linked from this page when users author them directly.
- Per-state `effort` is authored in `states.yaml`; its syntax and applicability
  are owned by [§FS-rhei-states.1](rhei-states.spec.md#1-schema-additions), while
  agent-native mappings and argument composition are owned by
  [§FS-rhei-agents.1.1.2](rhei-agents.spec.md#112-agents) and
  [§FS-rhei-agents.2.2](rhei-agents.spec.md#22-modes).
- New template syntax or manifest fields belong in [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification) and must
  be linked from this page.
- Block-composition semantics belong in
  [§FS-rhei-library](rhei-library.spec.md#fs-rhei-library-composable-blocks);
  additions to `template.yaml` or `rhei instantiate` must also update
  [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification).
- New execution references that appear in authored files must identify their
  owner spec from this page.

Task metadata may also control execution without changing the state machine:
`**Target:**` and `**Model:**` select an execution identity
([§FS-rhei-plan-language.3.11](rhei-plan-language.spec.md#311-task-execution-overrides)),
while `**Inherits:**` opts a task into or out of snapshot inheritance
([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-snapshot-inheritance-overrides)).
The plan-language specification owns that markdown syntax; the snapshot
specification owns its lineage, selection, compatibility, and fallback behavior
([§FS-rhei-snapshots.4](rhei-snapshots.spec.md#4-state-machine-schema)).

If a feature changes what a valid Rhei workflow can contain, this reference must
be updated in the same change. The goal is not to centralize every rule here;
the goal is to make the authoritative rule easy to find.
