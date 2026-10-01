# FS-rhei-states-deprecation: The deprecated States declaration and the cross-root name match

Two mechanisms for saying which state machine a rhei runs under are deprecated
and are removed one release after this one:

- the `**States:**` declaration in `index.rhei.md` and `index.panta.md`;
- resolving a declaration from a `states.yaml` in **another** rhei's execution
  root — the unique-`name` match across candidate roots
  ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)).

Both are still parsed, still resolved, and still win over
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)'s clauses wherever they resolve, so **anything that
resolves today resolves to the same machine**: no plan written against the
previous rules changes the machine it runs under in this release. Where the two
mechanisms and the new clauses disagree, one `warning:` line says which and what
to do about it.

`name:` inside a states file and `--state-machine` are not deprecated. `name:`
is what clause 1 reports as the effective machine name, and `--state-machine`
remains the whole-scope override.

This document exists to be deleted. It is the whole of the window, so the
removal release removes this declaration, the citations to it, and the
deprecated resolution pass behind it, and leaves
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)'s three clauses untouched ([§FS-rhei-states-deprecation.4](rhei-states-deprecation.spec.md#4-what-the-removal-release-deletes)).

## 1. The deprecated resolution runs first

After `--state-machine`, and before
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution) clause 1, a rhei that **carries a
declaration** — its own `**States:**`, or `index.panta.md`'s by inheritance —
resolves it exactly as the previous release did: its own execution root for a
custom same-name declaration, then one flat candidate set — the project root's
`states.yaml` together with every candidate rhei root's — in which a unique
`name:` match resolves. The project root is one of the roots that can be
ambiguous rather than a step ahead of them, so two declaring files among them
are the ambiguity error wherever the two sit. A rhei that carries no
declaration at all has nothing to resolve here and goes straight to clause 1,
which is why a project whose manifest declares nothing and whose member
declares nothing reads that member's own `states.yaml` in this release rather
than the next.

Where that pass resolves a machine, **that machine wins**, even where clause 1
would have found a different file. Where it resolves several candidates it is
still the ambiguity error, with its message and its two fixes unchanged. Where
it resolves nothing, resolution continues at clause 1 — and where clause 1 then
finds nothing either, the declaration is the validation error
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution) keeps for it.

So the window does turn some of the previous release's errors into resolutions,
and only in that direction. A declaration naming a machine nothing supplies, in
a rhei whose own execution root holds *some* `states.yaml`, failed that release
and runs under that file in this one, with [§FS-rhei-states-deprecation.2.1](rhei-states-deprecation.spec.md#21-a-declaration-nothing-supplies-with-an-own-root-file-behind-it)'s
warning beside it. What does not change is the refusal with nowhere to fall
through to — the same declaration with no file in that root at all — and the
ambiguity error above, which is a disagreement between candidates rather than
an absence of them. Nothing that resolves today resolves to a different machine,
and nothing starts erroring that did not.

One caller is deliberately held to the stricter rule: `rhei new --states <name>`
still refuses a name no `states.yaml` declares, because creation authors the
tree rather than reading one, and the one command whose job is to write a
correct index must not write a line it would warn the author to delete
([§FS-rhei-new.1.2](rhei-new.spec.md#12-options-that-create-a-rhei)).

Two consequences are decisions rather than readings. A declaration that resolves
to the **built-in** machine counts as resolving, so a rhei restating the
project's effective built-in `rhei` default keeps running under the built-in
machine this release even where its own root now holds a file named `rhei`: the
rule is about which machine a plan runs under, not about what the machine is
called. And the pass is per *declaration*, not per rhei — `index.panta.md`'s
declaration is resolved once as the project default, however many rheis inherit
it.

## 2. The three warnings

Each warning fires only where a deprecated mechanism and
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution) disagree, so the shape every instantiated
template ships — a rhei declaring `X` with `X` in its own execution root — is
silent, as is a rhei that inherits a project default it has no file of its own
to contradict. Each names the file it read and what to do about it: a warning
that only says "deprecated" sends the reader hunting.

**At most one warning per declaration.** The second and third occasions can both
hold of one declaration — a cross-root match used while a file sits unread in the
rhei's own root. The cross-root warning is the one printed there, because it
names the file that actually resolved; the other would tell the reader to delete
a line and leave them on the stale local file without saying so.

### 2.1. A declaration nothing supplies, with an own-root file behind it

The deprecated pass resolved nothing and clause 1 resolved the rhei's own file.
The previous release failed this tree; this one runs it.

```text
warning: rhei 'a-ticket' declares state machine 'grounded-ticket', which no
  states file declares; 'panta/a-ticket/states.yaml' (agora) in its own root
  resolves instead. The `**States:**` declaration is deprecated and is removed
  in the next release — delete the line.
```

### 2.2. An own-root file deferred for one release

The deprecated pass resolved, and the rhei's own execution root holds a
`states.yaml` that is not the file it resolved. The resolution is the previous
release's; the warning is how a tree that would otherwise change machines
silently next release says so now.

```text
warning: rhei 'm1' runs under 'proj-machine' because its deprecated
  `**States:**` declaration still resolves; 'panta/m1/states.yaml' (leftover)
  in its own root takes over in the next release. Delete the line to move now,
  or remove the file to stay on 'proj-machine'.
```

### 2.3. A cross-root name match used

The deprecated pass resolved from a `states.yaml` in a rhei root other than the
one the declaration belongs to. The remedy names where the file belongs instead:
the declaring rhei's own root, or the project root to make it the default.

```text
warning: rhei 'm1' declares state machine 'mach-x', resolved from
  'panta/m2/states.yaml' in another rhei's root. Resolution across rhei roots
  is deprecated and is removed in the next release — move the file to
  'panta/m1/states.yaml', or to the project root to make it the default.
```

Where the declaration resolved this way is `index.panta.md`'s own, the warning
names the manifest and the project default rather than a rhei, and offers the
project root as the single remedy — there is no declaring rhei root to move the
file to.

## 3. The warning contract

The same contract as [§FS-rhei-templates.1.3](rhei-templates.spec.md#13-the-deprecation-warning), which this project already holds
its other deprecation to. A second deprecation with its own guard, its own
completion check, and a slightly different idea of "once" is how two
deprecations start behaving differently for no reason.

- Written to **stderr**, never stdout, so `--json` output stays parseable.
- A `warning:` line that names the file it read and what to do about it, in the
  same message.
- Fires **once per distinct declaration per process**, not once per lookup.
  Resolution runs per rhei and repeatedly inside one `rhei run`, and a per-lookup
  warning would bury the run's own output. The subject is the declaration — the
  rhei that carries it, or the manifest — so a manifest declaration inherited by
  five rheis warns once.
- Suppressed while the process is serving a shell completion request. Every
  completion is a fresh process, so the once-per-process guard cannot hold
  there; without this, a project in a deprecated shape prints the paragraph over
  the candidate list on every Tab press. A run is serving one only when
  `COMPLETE` names a shell: the values that turn dynamic completion *off* —
  unset, empty, and `0` — are ordinary runs and warn like any other.

`rhei states` is where resolution is read, so it is where all three are most
often seen: on stderr, beside rather than inside the `Source:` lines
([§FS-rhei-states-cmd.3.1](rhei-states-cmd.spec.md#31-source-line)).

## 4. What the removal release deletes

The release after this one deletes, together: the `**States:**` declaration from
the plan language, the cross-root name match, the deprecated pass of [§FS-rhei-states-deprecation.1](rhei-states-deprecation.spec.md#1-the-deprecated-resolution-runs-first), the
three warnings of [§FS-rhei-states-deprecation.2](rhei-states-deprecation.spec.md#2-the-three-warnings), and this declaration. What is left is
[§FS-rhei-plan-language.1.3](rhei-plan-language.spec.md#13-state-machine-resolution)'s three clauses and its `--state-machine` override,
unchanged by the removal — which is the point of keeping the window in one
declaration rather than in a paragraph beside the rule it suspends.

A tree still in a deprecated shape at that release changes which machine it runs
under. That is what the warnings are for: a release has been spent telling it so.

## Related Specifications

- [Plan Language Specification](rhei-plan-language.spec.md) — the resolution clauses this window suspends
- [Panta root architecture](../architecture/rhei-panta.spec.md) — the architecture half of both rules
- [Templates Specification](rhei-templates.spec.md) — the deprecation-warning contract this one copies
