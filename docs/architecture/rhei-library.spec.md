# AR-rhei-library: Graph Union Architecture

Composition is one module behind `rhei instantiate` that reuses the ticket
writer, the lock and the validator that already exist, and whose output is what
the union of two authored files would have been if a person had typed it. It
replaces a typed block compiler that lowered two templates into one flat machine
by qualifying every name; nothing lowers now, so there is no second
representation to keep in step with the runtime's.
[§FS-rhei-library.1](../functional-spec/rhei-library.spec.md#1-composition-by-graph-union)

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

## 1. The shared representation

The union holds no state schema of its own. State fragments enter as the
parser's and validator's shared representation — `rhei_validator` re-exports
`rhei_core::state_machine` for exactly this — and plan nodes enter as parsed
task structures, never as markdown search and replace. A second schema beside
the runtime's is what would drift from it, and a union writes into a file the
runtime then reads, so the two must be the same values.

That is why every union decision below is taken over parsed values while the
*bytes* written are the author's own: equality is judged on the representation,
insertion is done on the text.

## 2. Where it runs and what it owns

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
   ([§FS-rhei-library.3.1](../functional-spec/rhei-library.spec.md#31-same-name-same-thing)).
2. **Scoping** — deriving the `sources:` set of a template's own `from: "*"`
   rule from that template's own states
   ([§FS-rhei-library.3.2](../functional-spec/rhei-library.spec.md#32-wildcards-stay-home)).
3. **Projection** — keeping `node_policy.by_type` and dropping `root`,
   `default`, `rhei` and `overrides`
   ([§FS-rhei-library.3.3](../functional-spec/rhei-library.spec.md#33-the-host-routes)).
4. **Re-parenting** — rewriting relative ids, heading depth, `**Prior:**`,
   `**Consumes:**` and `metadata.tasks` keys against a parent
   ([§FS-rhei-library.4](../functional-spec/rhei-library.spec.md#4-placement-ids-tickets-and-frontmatter)).

Nothing else is its own, and in particular no representation is: every one of
the four is decided over the shared values of [§AR-rhei-library.1](rhei-library.spec.md#1-the-shared-representation).

## 3. One re-parenting, two callers

`under:` in `includes:` and `--into <rhei>.<task>` are one function called
against different parents, and that is a load-bearing property rather than an
implementation convenience: it is what makes "a template works at any level" a
fact about one code path instead of a promise two code paths have to keep
([§FS-rhei-library.6.1](../functional-spec/rhei-library.spec.md#61-under)).
Composing the two — a template whose entry carries `under:`, instantiated
`--into <rhei>.<task>` — is the same function applied twice, and both depth
limits are evaluated once on the result rather than at each application
([§FS-rhei-library.4.1](../functional-spec/rhei-library.spec.md#41-depth)).

A second placement mechanism for `under:` would be the defect this architecture
exists to prevent, so a reviewer should read any new re-parenting code path as a
design error rather than as an optimization.

## 4. What it reuses rather than restates

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
([§FS-rhei-library.5](../functional-spec/rhei-library.spec.md#5-placement-and-ticket-identity)). A union that read the ledger would be a second
authority on travel.

## 5. Transactional shape

The union is **all-or-nothing against the target**. Every part is rendered,
unioned and placed in memory; the whole result is validated; only then is
anything written, under the lock, as one insertion per block. There is no
staging directory, which is why `--keep-on-error` has nothing to keep and is
refused ([§FS-rhei-library.7.3](../functional-spec/rhei-library.spec.md#73-flags---into-refuses)), and why a refusal leaves the target
byte-identical without a rollback path to get wrong.

`--dry-run` runs the same stages and prints the insertion it would have made.
Byte-exact serialization of every field the union writes is therefore a property
the diff depends on, rather than a property of a lock file
([§FS-rhei-library.7.1](../functional-spec/rhei-library.spec.md#71-the-fence-comment-is-the-whole-of-provenance)).

## 6. What the flat boundary keeps

Two flat-schema properties the runtime already has — cancellation roles
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
([§FS-rhei-library.2.1](../functional-spec/rhei-library.spec.md#21-the-machine-the-target-must-have)).
