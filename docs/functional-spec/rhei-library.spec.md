# FS-rhei-library: Composition by Graph Union

Two rheis compose by **graph union**: a template's states, transitions,
profiles, node kinds, node-policy routes, prompt templates, scripts and tickets
are added to a machine and a plan that already exist, under the names their
authors wrote. Nothing is renamed, qualified or alias-encoded, so the result is
what the union of two authored files would have been if a person had typed it —
which is what keeps output predictable
([§GOAL-rhei-outcomes](goals.md#goal-rhei-outcomes-goals)) and what lets a
prompt name a state instead of being sent to look one up.

Composition happens entirely inside `rhei instantiate`, through two surfaces
that are one mechanism seen from two sides: `--into` places a template into a
plan that already exists ([§FS-rhei-library.2](rhei-library.spec.md#2---into-placing-a-template-into-a-plan)), and `includes:` in `template.yaml`
builds a template out of templates ([§FS-rhei-library.6](rhei-library.spec.md#6-includes-a-template-built-from-templates)). Three rules decide every
union and each of them refuses rather than resolves ([§FS-rhei-library.3](rhei-library.spec.md#3-the-union-rules)).

## 1. Composition by graph union

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

- `--into <target>` places a template into a plan that already exists ([§FS-rhei-library.2](rhei-library.spec.md#2---into-placing-a-template-into-a-plan));
- `includes:` in `template.yaml` builds a template out of templates ([§FS-rhei-library.6](rhei-library.spec.md#6-includes-a-template-built-from-templates)), each
  entry optionally placing its tickets `under:` a task of the host.

Nothing is written until the union validates in the scope the target runs in
([§FS-rhei-library.2](rhei-library.spec.md#2---into-placing-a-template-into-a-plan)). On error the target is byte-identical to what it was, and
the error names both sources of the disagreement and the field they disagree
on.

## 2. `--into`: placing a template into a plan

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
inside one and to the rhei's own settings root outside one, and nothing is left
beside a member rhei, whose own settings root is read by nothing). What `--into`
adds is where the result goes:

1. The rendered index's **title and description are dropped** — the host's index
   describes the composition. Its frontmatter unions ([§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)).
2. `states`, `transitions`, `profiles` and `node_policy.by_type` join the
   target's `states.yaml` under the union rules of [§FS-rhei-library.3](rhei-library.spec.md#3-the-union-rules); `models` joins as a set;
   every other top-level key stays the host's.
3. `prompt_templates/*.md` and `scripts/*` are copied beside the rhei's own,
   under rule 1 of [§FS-rhei-library.3](rhei-library.spec.md#3-the-union-rules). Other bundled files travel as they do for `--output`,
   except `README.md`, which describes the template and stays behind.
4. The template's tickets are written where `rhei new` would write them
   ([§FS-rhei-new.3.1](rhei-new.spec.md#31-where-it-is-written)) with their ids re-parented ([§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)).
5. One fence comment records the inclusion ([§FS-rhei-library.7.1](rhei-library.spec.md#71-the-fence-comment-is-the-whole-of-provenance)).

The union is validated **in the scope the target actually runs in**, and
nothing is written until it passes. For a member rhei that scope is its project:
its settings and its machine both resolve there, so a union validated against
the member alone can report success over a write that leaves every
project-scoped command failing — the isolation
[§FS-rhei-templates.6.2](rhei-templates.spec.md#62-instantiating-inside-a-panta-project)'s "Validation scope" already names for
`--output`. Outside a project the rhei's own root is the scope. Either way the
union answers for the errors it **introduced**: a defect the scope already
carried — a member the union never read, a result artifact outside the plan
files — is not a placement's to refuse, which is the same reading `rhei new`
takes of its own write ([§FS-rhei-new.5.2](rhei-new.spec.md#52-a-create-answers-for-the-errors-it-introduced)).

The write takes the permanent sibling lock every rewriting command takes
([§FS-rhei-new.4](rhei-new.spec.md#4-ids)) — the scope's own sidecar, which for a member is
the project's and so also covers the settings the hoist rewrites there, and
each plan file the placement rewrites, from before the first host read through
the write — so a
union into a rhei with a live `rhei run` serializes against it rather than
racing it, and it keeps the host's bytes and inserts the template's rendered
entries at the end of each block, so a `git diff` after `--into` shows the added
lines and nothing else. The placed tickets are appended
work, which creates no capacity: the project's invocation and spend accounts are
exactly where they were ([§REQ-bounded-neural-work.4](../requirements/bounded-neural-work.spec.md#4-nothing-creates-capacity)).

`--dry-run` prints that diff — one block per inclusion, with its ticket count —
and writes nothing.

### 2.1. The machine the target must have

`--into` requires the target rhei's **effective** state machine to be the
`states.yaml` in its own execution root, because that is the only file a union
can be written into and have the rhei run under it. Under
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution) clause 1 a rhei's own root is consulted whatever
its index says, so writing the union into that file is enough — there is
nothing for `--into` to declare. The three cases:

- the root file exists — union into that file; no declaration is written or
  changed, whether the index declares the matching name or declares nothing;
- there is no file in the root — **refused**, printing the one remedy that
  makes the target eligible: `cp <project>/states.yaml <rhei>/states.yaml`,
  with the note that the rhei then stops following the project default;
- the index declares a name the root file's `name` does not match —
  **refused**, naming both. This is a pre-existing load error, not one `--into`
  introduces, and it survives the deprecation window
  ([§FS-rhei-states-deprecation.1](rhei-states-deprecation.spec.md#1-the-deprecated-resolution-runs-first)) for as long as the declaration does: during
  the window a union written into a file the declaration defers would not
  govern the rhei for a release, which makes the refusal more necessary rather
  than less.

The basin ([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)) is never a `--into` target: it holds unfiled
tickets that run under the project default and has no machine of its own to add
to.

## 3. The union rules

Three rules decide every union, and each of them refuses rather than resolves.

### 3.1. Same name, same thing

A state, transition, profile, node kind, prompt template or script that both
sides define must be **identical apart from `description`**. Declaration order
and descriptive prose are not differences; every operative field is. A
difference is an error naming both sources and the differing field, and there is
no namespace, no qualification and no rename to escape it: a template that must
exist twice in one machine takes a prefix as a declared input.

One definition standing for several identically named ones is therefore the
ordinary case of a union rather than an exception to single ownership.

`settings.json` is the one bundled file the rule applies to **key by key**
rather than whole. Two templates that each declare an agent of their own declare
two agents, and one key both set differently is a refusal naming it — the
granularity [§FS-rhei-templates.6.2](rhei-templates.spec.md#62-instantiating-inside-a-panta-project) already merges at, and what lets
`changeset-review` compose out of two templates that each ship one agent. Where
the destination is a project, that section's precedence governs instead of a
refusal: values the project already defines win, and the summary names both what
was added and what was kept.

For a **terminal** the rule is not a name test. Two terminals coalesce when
`final: true` holds on both, neither has an outgoing exact transition, and both
have the same effective cancellation role as
[§FS-rhei-states.1.4](rhei-states.spec.md#14-reserved-state-names) classifies it. Keying on the spellings `completed` and
`cancelled` would refuse a template whose terminals are named something else,
and there is a shipped built-in with six terminals and no `completed`. A
template with terminals of its own keeps them: the assembled machine has as many
terminal leaves as its parts have, and the two that coincide coincide.

Nothing in a union ever un-finalizes a terminal. A template meant to be
continued from exposes a non-terminal exit state, and making a terminal
continuable is an edit its **author** makes.

### 3.2. Wildcards stay home

A template's `from: "*"` transition is written into the union with `sources:`
set to that template's own states — a field every consumer already reads
([§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)) — so a template's cancel-from-anywhere edge never
captures another template's states.

The **host's** own wildcards are left exactly as written and span the union: a
rhei that cancels from anywhere cancels the states it took in, and that is the
host's call to have made. The scoped and unscoped forms already coexist in one
valid machine, so this needs no new field.

### 3.3. The host routes

An included template validates standalone, so it carries a whole `node_policy`.
At union only its `by_type` entries survive. Its `root`, `default` and `rhei`
are dropped because the host's stand ([§FS-rhei-states.9](rhei-states.spec.md#9-node-policy)), and its `overrides`
are dropped because they key on `level`, which is a fact about where the host
put the template rather than about the template.

A ticket's kind is its lane ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)), so a template routes
its own tickets through node kinds of its own, and uses the bare `task` kind
only where it means the host's default lane.

### 3.4. Inputs union

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

### 3.5. Chaining two templates is the host's work

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

## 4. Placement: ids, tickets, and frontmatter

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
ticket walks the state and is a hazard the moment two do — which [§FS-rhei-library.7.2](rhei-library.spec.md#72-artifact-paths) reports.

### 4.1. Depth

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

## 5. Placement and ticket identity

A budget identity belongs to a run and never to a template. Placement therefore
does one refusal and one deletion, and reads nothing of the host's:

- **Refuse the source.** A rendered template entry carrying
  `metadata.tasks.<id>.budgetTicketId` is refused before anything is written.
  The scan covers the index's `metadata.tasks` **and** a task file's own
  metadata block, because [§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter) re-keys both. The refusal fires in `--output`
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
because of how they divide: the id-collision refusal ([§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)) stops live
replacement, the ledger's binding stops delete-and-re-place from refreshing
travel, and the project's accounts are what bound total work. Where the
id-collision refusal and the ledger could each claim a case, the refusal wins,
because it fires before anything is written.

## 6. `includes:`: a template built from templates

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
([§FS-rhei-library.3.4](rhei-library.spec.md#34-inputs-union)), unions them **in list order**, then validates the whole. The including
template's own `states.yaml`, `tasks/` and `index.rhei.md` are the host: the
edges into and out of the included templates, `node_policy.root` and `default`,
the spanning profiles of [§FS-rhei-library.3.5](rhei-library.spec.md#35-chaining-two-templates-is-the-hosts-work), and the inputs whose values it fixes are all
written there.

Every template stands alone, so every template is discovered, listed, validated
and gated exactly as a template is today. A template with `index.rhei.md` or
`plan.rhei.md` lays a rhei with `--output` and joins one with `--into`; no field
declares which, the layout does.

### 6.1. `under:`

`under: <task>` names a task **template-relative**, and the included template's
tickets are placed beneath it exactly as `--into <rhei>.<task>` places them:
`under:` is the `<task>` half of `--into`'s target applied one level in, and it
calls the same re-parenting, the same heading deepening, the same `**Prior:**`
and `**Consumes:**` rewrite, the same writer and the same refusals ([§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)). There
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
- **Depth composes and is checked once**, on the final ids, per [§FS-rhei-library.4.1](rhei-library.spec.md#41-depth). A
  template with `triage` under `ticket` puts `ticket.triage` at depth 2
  standalone; `--into release.ticket` makes it `release.ticket.triage` at depth 3
  and grows the host's `maxLevels` accordingly.
- **The same template under two parents is legal.** `{ template: t, under: a }`
  and `{ template: t, under: b }` are different parents and different ids, which
  is "once per parent" with a way to say the parent. Twice under one parent is
  the id collision of [§FS-rhei-library.4](rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter).
- **States do not move.** `under:` is about tickets. The machine is flat, so an
  entry's states, edges, profiles and kinds union identically whatever `under:`
  says, and `metadata.tasks.<id>` entries travel with their tickets and are
  re-keyed by the composed id. An entry that renders no tickets is legal with
  `under:` and is reported as `0 tickets`, so no input value turns a legal
  composition into a silent one.

## 7. Provenance, diagnostics, and what `--into` refuses to combine

### 7.1. The fence comment is the whole of provenance

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

### 7.2. Artifact paths

Two states from two templates declaring the **same literal path with no
per-task variable in it** is a union-time collision and is refused, naming both
states and the path.

One state with a rhei-scoped path walked by **two tickets** is not a union
question and is not refused: whether the second overwriting the first matters is
the author's call — one supervisor writing one plan note per rhei is correct.
`--into` **warns** there, naming the path and both tickets.

### 7.3. Flags `--into` refuses

Each combination is an error naming the pair rather than a meaning invented for
it:

| With `--into` | Why |
|---|---|
| `--output` | two destinations for one instantiation |
| `--execute` | it means `rhei run` on a **new** workspace; the target may already be running, and a second run is not what the flag promises |
| `--keep-on-error` | there is no staging directory to keep: nothing is written until the union validates, and on error the target is byte-identical |
| `--state-machine` | an invocation override; a union is a durable write and must go into the file the target actually runs under |

`--state-machine` keeps its meaning everywhere else.

## 8. Related specifications

- [§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)
  owns template discovery, input types, rendering, placement, and the CLI shell
  around compilation.
- [§AR-rhei-library](../architecture/rhei-library.spec.md#ar-rhei-library-graph-union-architecture)
  owns where the union runs, what it reuses, and the typed compiler boundary it
  replaces.
- [§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new)
  owns the ticket writer, the placement rules and the lock that a union reuses.
- [§FS-rhei-budgets](rhei-budgets.spec.md#fs-rhei-budgets-bounded-ticket-travel-project-invocations-and-a-days-spend)
  owns what a placed ticket's travel identity is; [§FS-rhei-library.5](rhei-library.spec.md#5-placement-and-ticket-identity) only refuses and strips.
- [§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)
  and [§FS-rhei-plan-language](rhei-plan-language.spec.md#fs-rhei-plan-language-rhei-plan-language-specification)
  own the generated runtime contracts.
