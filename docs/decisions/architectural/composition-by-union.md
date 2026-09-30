# DA-composition-by-union: Composition is graph union over names as their authors wrote them

## Status

accepted

## Context

Rhei shipped a block compiler to make workflow reusable. A template declared
`ports`, `data`, `expose`, `use`, `bind`, `seams` and `compatibility`;
`rhei instantiate --mount review=code-review --mount fix=fix --seam
review.done=fix.entry --pass review.decision=fix.decision` composed two of them,
and the result was one flat workspace, validated, with a
`composition.lock.json` recording where every definition came from.
[§FS-rhei-library](../../functional-spec/rhei-library.spec.md#fs-rhei-library-composable-blocks) [§AR-rhei-library](../../architecture/rhei-library.spec.md#ar-rhei-library-block-compiler-architecture)

It worked, and two things about it did not.

**The output stopped being readable.** Qualification is injective by design, so
that two valid owned definitions can never collide and merge can be total. The
price is that a composed machine's states are `m6_review__split` and
`m3_fix__final-fix`. A reviewer cannot follow it; worse, a prompt cannot name a
state, which is why `changeset-review`'s coordinate ticket has to instruct its
agent to "use the compiled state names declared in states.yaml" instead of
saying which state to move to. Readable, reviewable plans are the goal the
project is held to ([§GOAL-rhei-outcomes](../../functional-spec/goals.md#goal-rhei-outcomes-goals)), and an alias-encoded machine is the
one artifact in the system that a person is not expected to read.

**It could only express one shape.** A mount is a stage of one node's
lifecycle: parts run consecutively, seamed exit to entry. Every ticket template
in the agent-grounds workspace has the other shape — sibling steps under a
supervisor, each sent one at a time — and the compiler cannot place one part's
tasks under another part's task at all. So the three ticket templates there
share ten states and five scripts by **copy**, and the copies have drifted: of
the ten states only two are still identical in any two templates, and three
copies of one script are two versions.

There was also no way to add a piece of workflow to a plan that already exists.
`--mount` composes into a new workspace only, so continuing an existing plan
meant opening its `states.yaml` and pasting eight states, eight edges and a
profile by hand.

## Decision

**Composition is graph union over names as their authors wrote them.** A
template's states, transitions, profiles, node kinds, `node_policy.by_type`
routes, prompt templates, scripts and tickets are added to a machine and a plan
that already exist, and nothing is renamed, qualified or alias-encoded to make
the union possible.
[§FS-rhei-library.9](../../functional-spec/rhei-library.spec.md#9-composition-by-graph-union)

Five things follow, and they are the decision rather than its implementation.

1. **Names are the names the authors wrote.** What two templates share, they
   share because both authors spelled it the same way. Sharing is therefore
   deliberate, and the union refuses where two authors disagree instead of
   inventing a third name: a state, edge, profile, kind, prompt or script both
   sides define must be identical apart from `description`, and a template that
   must exist twice in one machine takes a prefix as a declared input.
   [§FS-rhei-library.11.1](../../functional-spec/rhei-library.spec.md#111-same-name-same-thing)
2. **One noun and one verb.** A template is instantiated — into a new place with
   `--output`, into an existing one with `--into` — and a template may include
   templates. No second manifest, no tier, no catalog, and no vocabulary of
   block, mount, alias, port, seam, pass or lock.
   [§FS-rhei-library.10](../../functional-spec/rhei-library.spec.md#10---into-placing-a-template-into-a-plan) [§FS-rhei-library.14](../../functional-spec/rhei-library.spec.md#14-includes-a-template-built-from-templates)
3. **Placement is one mechanism.** `under:` inside `includes:` is the `<task>`
   half of `--into <rhei>.<task>` applied one level in: the same re-parenting,
   heading deepening, `**Prior:**` rewrite, writer and refusals. A second
   placement code path would be the defect, because "a template works at any
   level" is then a fact about one function rather than a promise two functions
   have to keep.
   [§FS-rhei-library.14.1](../../functional-spec/rhei-library.spec.md#141-under)
4. **The union happens at instantiation and ends in flat files.** No grammar
   change, no loader change, no namespace. Every other command reads what it
   already read, so a composed plan is indistinguishable at runtime from one a
   person typed — which is what keeps output predictable.
   [§AR-rhei-library.6](../../architecture/rhei-library.spec.md#6-the-union-architecture)
5. **Provenance is one comment line.** `# --- <template> <version> src:sha256:…
   inputs: {…} ---` in `states.yaml` per inclusion. No lock file, no generated
   header, no per-node record: when a state keeps its author's name, what it
   came from is answered by reading it.
   [§FS-rhei-library.15.1](../../functional-spec/rhei-library.spec.md#151-the-fence-comment-is-the-whole-of-provenance)

The block compiler and its vocabulary are removed rather than kept beside this.
Two mechanisms for one job is the cost the readable one exists to avoid, and
nothing in the workspace passes `--mount`, `--seam` or `--pass` or reads a
`composition.lock.json`.

### Alternatives rejected

**Auto-prefix a template's names on collision** — the compiler's own answer, and
what makes its output unreadable. Refusing is worse ergonomics for one
instantiation and better for every later read, and it is what makes sharing a
decision somebody made rather than a coincidence of spelling.

**Extend the compiler rather than replace it.** Its parts can only be
consecutive stages of one node's lifecycle, which is not the shape the templates
that need composing have. The one thing it got right — that composition ends in
flat files the runtime already reads — is kept and is item 4 above.

**A manifest field of its own for nesting.** `under:` is `--into`'s `<task>`
under another name; a second spelling or a second code path for it is precisely
the duplication this decision exists to remove.

**Delete `AR-rhei-library` with the compiler.** Two citations outside the
compiler's own code survive it, and a citation that stops resolving fails the
gate. It is rewritten in place as the union architecture.

## Consequences

**A reviewer can read a composed machine, and a prompt can name a state.** This
is the point. `changeset-review`'s instruction to look up compiled state names
goes away, because there are no compiled state names.

**Two workflows can be joined only by a host that says how.** A profile's
`allowed` set is wholesale and profiles never merge
([§FS-rhei-states.8.2](../../functional-spec/rhei-states.spec.md#82-per-profile-validation)), so chaining two templates takes an edge and a
spanning profile that the host writes. That is not a gap: a path across two
templates is the one thing neither template knows.
[§FS-rhei-library.11.5](../../functional-spec/rhei-library.spec.md#115-chaining-two-templates-is-the-hosts-work)

**Rule 1 has an author's bill.** Two built-ins that both declare
`profiles.primary` cannot be unioned, so each is re-authored with a profile and
a node kind of its own. Their names and inputs do not change. This is the rule
working — the alternative is a merge policy deciding which author's `allowed`
set wins.

**Silence becomes a refusal in several places.** A same-named definition that
differs, a taken ticket id, a placement past depth four, a rhei-scoped artifact
path two states claim, an `includes:` cycle, a target with no machine of its
own, and a template that declares a budget identity are all errors now. Each
fires on a composition the compiler could not express at all, so nothing that
worked stops working.

**A template must stop assuming it is the whole rhei.** Prose that names the
template's own file paths is wrong once the template is placed, so templates
refer to things through runtime variables. That is an authoring rule a library's
gate enforces, not something the union can check.

**Travel identity stays the ledger's question.** Placement refuses a template
that declares a `budgetTicketId` and strips the key from the clone; whether a
placed ticket travels fresh is decided by whether the project's account holds a
binding for the pair it lands on. A union that read the ledger would be a second
authority on travel.
[§FS-rhei-library.13](../../functional-spec/rhei-library.spec.md#13-placement-and-ticket-identity) [§REQ-bounded-neural-work.4](../../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)

**`**States:**` and machine resolution are not settled by this.** `--into`
requires the target's effective machine to be the `states.yaml` in its own root,
and today's resolution does not consult that root for a rhei whose index is
silent — so `--into` writes the declaration as an interim. Simplifying
resolution is separate work, and
[§DA-per-rhei-state-machines](per-rhei-state-machines.md#da-per-rhei-state-machines-the-state-machine-is-a-per-rhei-property-defaulted-by-the-manifest) is untouched here.
[§FS-rhei-library.10.1](../../functional-spec/rhei-library.spec.md#101-the-machine-the-target-must-have)
