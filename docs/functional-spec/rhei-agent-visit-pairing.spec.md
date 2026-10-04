# FS-rhei-agent-visit-pairing: Finished Work Across an In-Place Target Edit

An operator may edit an agent state's target between two runs without the
ticket moving: the run that spawned the old target exited, and the next run
reads a plan that names a different identity for the same state. This point
says what the pre-spawn completion check (§FS-rhei-agents.3.2) does with the
work the old target finished in that visit. It extends the spawn-record trust
rule of §FS-rhei-agents.8.4 and changes neither the record nor its file name.

Work finished in a visit is not undone by an in-place edit of the state's
target, whether or not the new target's file-name slug equals the old one's.
`rhei reset` is the redo.

## 1. Own Names and Orphaned Records

An invocation reads as its own every name its own-record lookup consults, in
the order it tries them — the name its identity spells, and any fallback name
that lookup also trusts for it — not only the name that answered. A spawn record
of the ticket's task and canonical state at the current move count that no
current invocation reads as its own is **orphaned**.

A record on any current invocation's list of own names is never orphaned, so a
current fan-out sibling's record can never answer for another sibling.

## 2. Recordless Invocations

An invocation is **recordless** when it has no spawn record of its own whose
`moves` equals the current move count, whatever that record's ending. An
invocation whose own current-visit record failed, was interrupted, timed out,
was provider-limited, or is still `running` is not recordless: that record
decides for it alone, and no orphan answers for it.

## 3. The Unique Pairing

An orphaned record can stand in for a recordless invocation only when it names
the same `task` and canonical `state` as fields, its `moves` equals the current
move count, and it proves successful work by the test of §FS-rhei-agents.8.4.
The pairing is unique: exactly one such orphan and exactly one recordless
invocation. The orphan then answers for that invocation's visit eligibility.

Pairing proves only visit eligibility: the paired invocation's own declared
outputs and required result are still checked under its own identity
(§FS-rhei-agents.3.2).

## 4. What a Pairing Prints

`rhei run` says so on standard output, one line per pairing, once at the
pre-spawn decision:

```text
note: task <id> state '<state>': reusing this visit's finished spawn (<record path>) for <target>; `rhei reset` redoes it
```

## 5. Ambiguity Pairs Nothing

An ambiguous edit — at least one orphan and at least one recordless invocation,
but not exactly one of each, such as two orphans or two recordless invocations —
pairs nothing. The run prints a warning on standard error naming every orphaned
record, and the invocations are spawned.

An orphan with no invocation to answer for (a removed fan-out member) or an
invocation with no orphan (an added one) pairs nothing and passes silently.

## 6. The Legacy First-Visit Branch

The legacy first-visit interpretation of §FS-rhei-agents.3.2, which may reuse
pre-populated work without a spawn, applies only while no spawn record of this
task and state, of any identity, exists at the current move count. Such a record
means the state already ran in this visit, so the records decide, by
§FS-rhei-agents.8.4 and this point, not the files on disk.

## 7. Nothing Persisted

The orphaned record keeps its name, `worker` and `log`, so it still says which
identity did the work, and the pairing writes nothing. This rule adds no
persisted field or format.
