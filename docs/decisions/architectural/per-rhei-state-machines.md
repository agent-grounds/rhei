# DA-per-rhei-state-machines: The state machine is a per-rhei property, defaulted by the manifest

## Status

accepted

## Context

Panta shipped with a deliberate limit: one state machine governed a whole
project. The `index.panta.md` declaration (or the built-in `rhei` machine) was
the law; a member rhei could restate it but declaring a different machine was a
load error, and per-rhei divergence was parked on the roadmap as polish.
[§FS-rhei-panta.6](../../functional-spec/rhei-panta.spec.md#6-project-scope-and-command-behavior) [§AR-rhei-panta.4](../../architecture/rhei-panta.spec.md#4-state-machine-binding)

Two later product moves invalidated the limit's weighting without revisiting
it. Templates became the front door — eleven built-ins ship in the binary, each
necessarily bundling its own state machine, because each template *is* a
distinct process (a review loop is not a product-management loop). And
`rhei instantiate` learned to land templates in the enclosing project by
default, adopting the first template's machine as the project default so the
first instantiation would work at all. The combination armed a first-session
wall: template #1 succeeded frictionlessly and silently locked the project to
its machine; every template after it was refused, with only a second-class
standalone workspace as the way out — while the README promised "automate your
complex daily routines in minutes", plural, in one repository. Adoption also
broke the other direction: once a template's machine became the project
default, a later hand-written rhei with no `**States:**` line validated
against the *template's* machine and failed.

The technical root was not the guard rails but the merge: member `Rhei`
structs dissolve into one flat, project-qualified task list, and while
per-*file* ownership survives (task sources and roots, for routing writes
back), per-*machine* ownership was checked at load and then discarded. With
ownership-of-meaning gone from the model, every downstream consumer had to
assume one machine. The redundant declaration was the tell: `**States:**`
existed at two levels with a rule that the lower must restate the upper — a
field whose only legal value is "same as the parent" is a field that wants to
be an override. The plan language even said so already:
[§FS-rhei-plan-language.1.3](../../functional-spec/rhei-plan-language.spec.md#13-state-machine-resolution) resolved the effective declaration "per rhei", with
"a declaration in the rhei itself wins" — the Panta layer clamped it back to
uniformity.

## Decision

The state machine is a property of the **rhei**, defaulted by the project.

1. `index.panta.md`'s `**States:**` declaration is the project **default** —
   the built-in `rhei` machine when absent. It governs every rhei that
   declares nothing, the synthetic `basin` rhei, and the Panta root's node
   policy.
2. A rhei runs under the `states.yaml` in its own execution root when there is
   one, whatever its index says; a rhei with no file of its own inherits the
   already-resolved project machine wholesale. Divergence is not an error; it
   is the normal shape of a project holding more than one instantiated
   template. *Amended by #347*: this item said the `**States:**` declaration
   was what chose, and that omitting the line inherited the project machine
   with the rhei's own root never consulted. The declaration is deprecated and
   survives one release ahead of this rule
   ([§FS-rhei-states-deprecation](../../functional-spec/rhei-states-deprecation.spec.md#fs-rhei-states-deprecation-the-deprecated-states-declaration-and-the-cross-root-name-match)), which is why this `## Status` is still
   `accepted` and the removal is its own decision.
3. The merge **records** machine ownership instead of discarding it: the
   project model carries, per rhei, the declared machine name (when declared)
   and the rhei's execution root, and every consumer resolves a ticket's
   machine through its owning rhei.
4. The graph stays one merged, project-qualified task list ([§DA-panta-root](panta-root.md#da-panta-root-panta-is-the-per-project-virtual-root-above-all-rheis) is
   unchanged). Machines are never combined, namespaced, or merged.
5. Cross-rhei readiness is the one computation where two machines meet: a
   `**Prior:**` is satisfied when the target ticket is
   terminal-and-not-cancelled **under the target's own machine**. Every other
   operation — state validity, transition legality, completion-target
   selection, artifact contracts, agent bindings — is a per-ticket question
   answered by the owning rhei's machine.
6. Machine files resolve by place, not by declaration: every rhei's own
   execution root `states.yaml` first (the shape every template ships),
   whatever its `name:` and whatever the index says, then the project root,
   then the built-in machine. An invalid candidate errors rather than falling
   back. *Amended by #347*: this item said a rhei's own root was consulted only
   on an explicit declaration's behalf, and not at all for a restated built-in
   default. The `## Context` above already argued for the collapse — a field
   whose only legal value is "same as the parent" is a field that wants to be
   an override — so this is the argument carried through rather than reversed.
   The declaration's own precedence survives one release
   ([§FS-rhei-states-deprecation](../../functional-spec/rhei-states-deprecation.spec.md#fs-rhei-states-deprecation-the-deprecated-states-declaration-and-the-cross-root-name-match)). [§AR-rhei-panta.4](../../architecture/rhei-panta.spec.md#4-state-machine-binding)
7. `--state-machine` stays a whole-scope override and errors when any
   in-scope rhei declares a name different from the override file's.
8. Machine **adoption** is removed everywhere it existed — `rhei instantiate`
   no longer writes a template's machine into `index.panta.md`, and
   `rhei init` no longer adopts a unanimously-declared machine — because
   adoption's only purpose was to satisfy the uniformity rule this decision
   removes, and its side effect was re-governing future silent rheis.

## Consequences

- Instantiating any number of templates into one project works; the
  machine-collision refusal and its standalone-workspace escape hatch are
  gone from that path.
- Projects that already adopted a machine keep loading: their members restate
  the default, which stays legal.
- Monitoring surfaces (`rhei states`, list/render/viz) present per-rhei
  machines grouped by rhei; state names are meaningful only relative to an
  owning rhei, and name collisions across machines are permitted.
- The validator, readiness, execution, and snapshot paths dispatch per ticket
  through the recorded ownership; the single-machine call surface survives
  only as a compatibility wrapper for single-rhei loads.
