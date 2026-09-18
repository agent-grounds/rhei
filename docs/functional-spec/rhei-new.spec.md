# FS-rhei-new: `rhei new`

Add a rhei or a ticket without knowing the plan format. Everything Rhei
executes is authored markdown, and that is a feature — a plan is readable,
diffable, and editable by hand. But it makes the *first* file a wall: before
`rhei new`, adding a rhei meant knowing that the file is `<id>.rhei.md`, that
its id comes from the file stem, that the heading is `# Rhei: <title>`, and
that a `## Tasks` section has to follow it. Adding a ticket meant knowing the
metadata field order and which state the rhei's machine starts in.

`rhei new` writes that file, or that ticket, from a title and flags. It is the
zero-friction "add a rhei" action Panta exists to make possible
([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)), extended to tickets because the same wall stands in front
of the second thing a user does.

The command is deliberately **complete**: every field the plan language lets an
author write on a new ticket has a flag, so `rhei new` is never the thing that
gets you started and then abandons you to an editor for `**Prior:**`. What it
does not do is *change* anything — it only creates. Editing an existing ticket
stays a file edit, and changing state stays `rhei transition`
([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)).

## 1. Usage

```bash
rhei new <TITLE> [--under <PARENT>] [options]
```

`TITLE` is the human-readable title. With no `--under`, `rhei new` creates a
**rhei** under Panta (§2). With `--under`, it creates a **ticket** inside the
named rhei or under the named ticket (§3). The presence of `--under` is what
selects the mode, and flags belonging to the other mode are a hard error
rather than a silent no-op (§5.3).

### 1.1. Shared options

| Flag                  | Default          | Description                                                       |
|-----------------------|------------------|-------------------------------------------------------------------|
| `--project <PATH>`    | inferred         | The project or plan to write into, resolved exactly as every other command resolves one: omitted, the enclosing project, workspace, or lone plan; named, a member rhei widens to the project it belongs to ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)) |
| `--id <ID>`           | derived          | Explicit id: for a rhei, the one otherwise derived from the title; for a ticket, the segment otherwise taken from the sibling numbering — a name works there too, so `--id review --under plat` writes `plat.review` (§4) |
| `--description <TEXT>`| empty            | Body content — the ticket's description, or the rhei's lead paragraph (§3.4) |
| `--description-file <PATH>` | —          | Read the description from a file; `-` reads standard input (§3.4) |
| `--dry-run`           | off              | Preview the create: plan data is written, validated, and always rolled back; permanent coordination artifacts may remain (§5.4) |
| `--json`              | off              | Emit the created id, kind, path, and state as JSON                |
| `--keep-on-error`     | off              | Keep the write when validation fails, instead of rolling it back (§5.2) |

`TITLE` occupies the positional slot that every other command gives to a plan
path, which is why the plan is named with `--project` here. Creating something
is the one operation whose subject is a name rather than a file, and taking the
title positionally is what makes `rhei new "Authentication"` read as one
thought.

`TITLE` is checked as an argument, for the same reason a description is (§3.4).
It becomes the rest of a `# Rhei:` or `### Task 4:` heading line, so an empty or
whitespace-only title writes a heading with nothing after the colon and a title
carrying a newline writes a second line the parser reads as plan content. Both
come back as a parse error with a code frame pointing into a file the rollback
has already removed — a line number in a file that no longer exists is the one
report a user cannot act on, and it is avoidable here because the offending text
is an argument.

### 1.2. Options that create a rhei

| Flag                    | Default              | Description                                                    |
|-------------------------|----------------------|----------------------------------------------------------------|
| `--dir`                 | off                  | Create a Directory Workspace rhei, or adopt the prospective workspace §2.1.1 permits |
| `--states <NAME>`       | project default      | Write a `**States:**` declaration; normal binding resolves it from the created rhei's execution root ([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)) |
| `--max-levels <N>`      | unset                | Write `structure.maxLevels`                                    |
| `--node-kinds <K,...>`  | unset                | Write `structure.nodeKinds`                                    |

### 1.3. Options that create a ticket

| Flag                       | Default            | Description                                              |
|----------------------------|--------------------|----------------------------------------------------------|
| `--under <PARENT>`         | —                  | Owning rhei id (`auth`, `basin`) for a top-level ticket, or ticket id (`auth.1`) for a subtask |
| `--kind <KIND>`            | `task`             | Heading keyword, checked against `structure.nodeKinds` ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)) |
| `--state <STATE>`          | the machine's initial | `**State:**`, checked against the owning rhei's machine (§3.2) |
| `--prior <ID>`             | none               | `**Prior:**` entry; repeatable, and a comma-separated list is accepted |
| `--inherits <RULE>`        | none               | `**Inherits:**`; accepts `none` or `'<name> from <self|ancestor|prior>'` ([§FS-rhei-plan-language.3.14](rhei-plan-language.spec.md#314-task-snapshot-inheritance-overrides)) |
| `--provides <NAME>`        | none               | `**Provides:**` entry; repeatable ([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)) |
| `--consumes <ID:NAME>`     | none               | `**Consumes:**` entry; repeatable. Selects prompt injection, not filesystem visibility. Undeclared sibling exports remain readable; pair every producer with `--prior <ID>` or validation rejects the created ticket ([§FS-rhei-plan-language.3.12.1](rhei-plan-language.spec.md#3121-declaration-integrity)) |
| `--excludes <ENTRY>`       | none               | `**Excludes:**` entry; repeatable. Each value is one `checkout=<path>`, `artifact=<path>`, or export reference ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)) |
| `--assignee <WHO>`         | none               | `**Assignee:**`, which is a claim: `rhei next` and `rhei run` skip an assigned ticket until `rhei release <id>`, and the create says so (§5.4) |
| `--model <MODEL>`          | none               | `**Model:**`; mutually exclusive with `--target` ([§FS-rhei-plan-language.3.11](rhei-plan-language.spec.md#311-task-execution-overrides)) |
| `--target <TARGET>`        | none               | `**Target:**`; mutually exclusive with `--model`, which the identity already carries ([§FS-rhei-plan-language.3.11](rhei-plan-language.spec.md#311-task-execution-overrides)) |

Repeatable fields are written in the order given, comma-separated, on one line.
`--excludes` preserves the authored entry order and writes its line after
`**Consumes:**` and before `**Assignee:**`; create-time validation applies the
same graph and path rules as `rhei validate`.
A `--prior` value is written through unchanged, so both authored forms work:
`--prior "Task 1"` keeps the node-kind keyword the plan language allows, and
`--prior auth.1` writes the bare cross-rhei reference. `rhei new` deliberately
does not invent a keyword the author did not ask for, and does not resolve the
reference itself — an unresolvable prior is a validation error with a code
frame ([§FS-rhei-validate.4.1](rhei-validate.spec.md#41-unresolved-prior-references)), which is a better report than anything the
create path could produce.

`--inherits` writes the normalized rule after `**Prior:**` and before the
remaining optional metadata. It validates the complete value before creating
or changing any file. Omitting the flag writes no field and therefore preserves
state-level inheritance.

## 2. Creating a rhei

`rhei new "Authentication"` derives the id `authentication` from the title (§4)
and writes `authentication.rhei.md` next to `index.panta.md`:

```markdown
# Rhei: Authentication

## Tasks
```

That is the whole file. A rhei with no tickets is valid — the Directory
Workspace format has always accepted one, and as of this command the
single-file format does too ([§FS-rhei-plan-language.1.1](rhei-plan-language.spec.md#11-single-file-plan-1-agent-or-low-concurrency)). Seeding a
placeholder ticket instead was rejected: a placeholder is indistinguishable
from real work to `rhei next` and `rhei run`, so the first thing a new project
would do is dispatch an agent onto a ticket nobody wrote.

With the optional header fields, the order is fixed by the plan language —
heading, `**States:**`, frontmatter, description, `## Tasks`:

```markdown
# Rhei: Billing
**States:** billing-review

---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

Everything invoice-related, including dunning.

## Tasks
```

`--states` only writes the declaration; it does not create the machine. A rhei
declaring a machine that no `states.yaml` provides is an error at the next
load, naming where the file is looked for, the machine names the project does
provide, and `/rhei-state-machine-writer` for authoring the one that is missing
([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding), §6). Every other flag with a declared set of legal values
lists that set when the value is wrong, and this one is no exception just
because its set lives in files rather than in the plan. This is the
honest order: the machine is authored, and the rhei points at it — so the
create is rolled back like any other invalid one (§5.2), and writing the rhei
*before* its machine takes `--keep-on-error`.

### 2.1. Layout

The default is a single file, because that is the format a rhei can be read in
one screen and converted out of later. `--dir` creates the Directory Workspace
shape instead ([§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)):

```text
billing/
  index.rhei.md      ← the header fields above, without `## Tasks`
  tasks/             ← empty; ticket files land here
```

The workspace index carries no `## Tasks` section: in that format tickets live
in `tasks/` files, and an empty `tasks/` directory is a valid empty rhei.

Creating a rhei requires a Panta project, and the target is resolved the way
every other command resolves one. Standing in a member rhei's directory — or
naming that member with `--project` — creates the new rhei in the project the
member belongs to, and the widening is announced exactly as `rhei validate`
announces it ([§FS-rhei-validate.1.1](rhei-validate.spec.md#11-why-there-is-no---rhei)). Only when the resolved target really is a
lone plan or a bare workspace does the command say so and point at `rhei init`
— a rhei is a *member* of a project, and there is nowhere to put a second one
otherwise.

#### 2.1.1. Prospective Directory Workspace adoption

`--dir` may create its index and task directory inside a same-id directory that
already exists but is not yet a rhei. The directory is adoptable only when its
entries are an authored `states.yaml` and optional `prompt_templates/`
directory, as before, plus at most the exact coordination entry
`index.rhei.md.lock`. That entry must be an empty regular file and is also
admissible by itself in an otherwise empty directory. Its presence proves
neither ownership nor origin: creation acquires it before the authoritative
admission and absence decision. `prompt_templates/` must be a directory and is
admissible only beside `states.yaml`; a template directory alone, a pre-existing
`tasks/`, any differently named lock, a nonempty or non-regular index sidecar,
or any other entry makes the destination occupied. The refusal happens before
plan publication, names the obstruction, and tells the author to move or remove
it.

The allowlist classifies filesystem shape, not machine validity. Once the new
`index.rhei.md` makes the directory a rhei, the ordinary project load resolves a
declared `--states <NAME>` from that rhei's execution root and the ordinary
post-write validation accepts or rejects the authored machine. This is the only
resolution path: creation does not search a prospective root before the index
exists and does not add a second state-machine precedence rule. A successful
adoption adds `index.rhei.md`, an empty `tasks/`, and establishes or reuses the
permanent empty `index.rhei.md.lock`; it preserves every pre-existing authored
byte and never treats the sidecar as owned plan data.

Without `--dir`, creation remains the single-file layout at `<id>.rhei.md`, with
the project directory as its execution root. A same-id non-rhei directory is a
layout conflict: the command does not search it for a machine or leave the
single-file destination behind, and points to `--dir` when the directory has an
adoptable shape.

## 3. Creating a ticket

`--under` names where the ticket goes:

```bash
rhei new "Rotate signing keys" --under auth       # top-level ticket in rhei auth
rhei new "Handle expiry" --under auth.3           # subtask of ticket auth.3
rhei new "Fix the typo in the footer" --under basin   # unfiled capture
```

A single-segment value naming a rhei is the owning rhei; any other value is
read as a ticket id and the new ticket becomes its child. A value that is
neither is an error listing the rhei ids in the project.

`--under basin` is how a ticket gets captured without choosing a domain rhei
first ([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)), inside a Panta project and nowhere else (§3.5).
The basin directory is created on demand together with the permanent sidecar
for the chosen task destination; no authored basin manifest is generated,
because the basin's manifest is synthetic by design
([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)). Filing it into a domain rhei later stays a file move.

The written ticket carries the fields that were asked for, in plan-language
order:

```markdown
### Task 4: Rotate signing keys
**State:** pending
**Prior:** Task 3
**Assignee:** vj

Rotate the JWT signing keys and publish the new JWKS.
```

### 3.1. Where it is written

In a single-file rhei the ticket is appended inside `## Tasks` — at the end for
a top-level ticket, and immediately after the parent's existing subtree for a
subtask, so the file stays in id order. In a Directory Workspace, a top-level
ticket becomes a new file in `tasks/`, while a subtask is appended to the file
that already holds its parent: a task file owns a subtree, and splitting one
across files would put a parent and its child in different diffs.

A task file's name is the ticket's id, zero-padded to three digits, followed by
a slug from the title: `004-rotate-signing-keys.md`. The padding is not
cosmetic. Path order *is* plan order ([§FS-rhei-plan-language.1.2](rhei-plan-language.spec.md#12-directory-workspace-agent-teams-high-concurrency)) and commands
that scan in plan order schedule in it, so an unpadded `10-…` sorts between
`1-…` and `2-…` and the eleventh ticket in a rhei is handed out second — with
nothing to notice, because the file itself is valid and `rhei validate`
succeeds. Three digits matches what every shipped template already writes. A
ticket created with a named `--id` keeps its name, which has no numeric order to
preserve.

A project that already holds unpadded task files keeps the order those files
give it. `rhei new` renames nothing it did not write: renaming a file moves work
someone may have open, and a create is not the command that should do it. Padded
and unpadded names in one directory sort by the rules of the names that are
there — the fix is to rename the old files, and the new ones are already right.

Nothing else in the file is rewritten. `rhei new` inserts its own block and
leaves every other byte — including hand-authored spacing and comments — as it
found it. The block is written with the line terminator the file already uses,
so a CRLF file stays CRLF; existing blank lines are never removed, and a single
blank separator is emitted only where the line before the insertion point is
not already blank. A `git diff` after a create shows the added lines and
nothing else, which is the whole point: a create the author cannot review as a
three-line diff is a create they have to re-read the file to trust.

### 3.2. State

`--state` is omitted in the normal case and the ticket starts in the initial
state of the machine that governs it — resolved through the *owning rhei*
([§AR-rhei-panta.4](../architecture/rhei-panta.spec.md#4-state-machine-binding)), and through the node profile matching the new ticket's kind
and depth when the machine declares profiles ([§FS-rhei-states.9.2](rhei-states.spec.md#92-resolution)). A ticket
created in rhei `billing` starts in `billing`'s initial state even when the
project default machine starts somewhere else.

`--state` names a different starting state, and is checked against that same
machine. This is initial authoring, which is the one moment the plan language
permits a state to be written directly; every change after it goes through
`rhei transition` and its compare-and-swap, callbacks, and ledger
([§FS-rhei-plan-language.1.4](rhei-plan-language.spec.md#14-directory-workspace-metadata)). `rhei new` does not run `on_enter` callbacks for
the state it writes, for the same reason authoring a plan by hand does not: the
ticket is being created in that state, not transitioning into it.

### 3.3. Depth and kind

A subtask deeper than the rhei's `structure.maxLevels` is refused before
anything is written, naming the limit and where it is declared. A rhei created
without `--max-levels` carries no frontmatter block at all, so the refusal spells
out the block to add rather than naming a field that is not there. `--kind` is
checked against `structure.nodeKinds` the same way ([§FS-rhei-plan-language.3.7](rhei-plan-language.spec.md#37-node-kind-validity)),
so a mistyped kind is a message about the kinds this rhei declares rather than
a parse error on the next command; and a rhei that declares kinds not including
`task` is a rhei where `--kind` is *required*, which is what the refusal says
rather than blaming the user for a word they never typed.

Every argument that has a checkable shape is checked in argument handling, and
that includes the two reference fields: `--consumes` takes
`<task-id>:<export-name>` and `--provides` takes an export name
([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)). Round-tripping a flag through the file and
reporting it back as a parse error with a line number is the failure this
section exists to prevent — the user typed a flag, so the message is about the
flag. Whether the reference *resolves* is a different question, and nothing
answers it yet (§6).

### 3.4. What a description may contain

A description is prose, and `rhei new` writes it into the plan verbatim. A line
the plan language reads as *structure* would therefore stop being the ticket's
description and start being part of the plan, so those lines are refused before
anything is written — as an argument error naming the offending line, not as a
parse error with a line number in a file the author never opened.

Three shapes are refused: an ATX heading at any level (`#` through `######`), a
line opening with a `**Field:**` metadata marker, and a line that is exactly
`---`. An `### Task 9: Injected` line in a description is not a formatting
mistake, it is a second ticket: the parser reads it as one, so a create could
report a single new id while writing two — the second one carrying whatever
`**State:**` the text supplied, including a terminal one that makes `rhei next`
and `rhei run` skip work forever. A `---` line is the same forgery one level up:
a rhei's description is written directly under `# Rhei: <title>`, which is
exactly where the parser looks for frontmatter, so a description opening with one
authors the rhei's `structure:` — its node kinds and its depth limit — or its
`metadata.tasks.*.stateVisits`, which drives counted state-machine loops. This
matters most for `--description-file`, which exists so an issue body or a design
doc can be piped in, and that is exactly the content that carries `### ` headings
and `---` blocks.

Lines inside a fenced code block are content rather than structure and are
accepted unchanged — which makes the fences themselves load-bearing, so a
description whose ``` fences do not balance is refused as well, naming the line
the unclosed one opened on. An open fence is the most destructive thing a
description can carry: it is written verbatim, and every ticket *after* the
insertion point then reads as fenced text rather than as a plan node. An odd
number of fences is the ordinary shape of a pasted issue body that was cut
short, so the refusal is an argument error about the fence rather than the
whole-file guard in §5.1 catching the wreckage afterwards.

The refusal names the three ways to keep an offending line — escape the marker,
fence it, or make it bold text — and `rhei new` applies none of them itself.
Indenting is deliberately not offered: the plan lexer trims each line before
reading it, so leading whitespace does not protect a heading or a marker, and
the refusal says so. Demoting or escaping the author's line automatically would
mean a create that edits its input behind the author's back, and a description
carrying someone's issue body has to come out the way they wrote it.

### 3.5. The basin needs a project

`--under basin` requires a Panta project, exactly as creating a rhei does
(§2.1). Outside one there is no project directory to hold `basin/`, and the plan
the command resolved is itself the only rhei there is
([§AR-rhei-panta.2](../architecture/rhei-panta.spec.md#2-load-model)) — so there is no unfiled inbox to file into, and
none the command could create on demand. Against a lone plan or a bare
workspace it says so, naming the plan it resolved, and points at `rhei init`:

```
  × the basin exists only inside a Panta project; plan.rhei.md is a lone plan
  help: run `rhei init` to make this a project, then capture with `--under
        basin`.
```

Every other `--under` value still works there, because it resolves inside that
plan: the plan's own id takes a top-level ticket, and a ticket id takes a
subtask. `basin` is the one value a lone plan cannot answer for, which is why
the refusal names the plan rather than listing ids that were never the problem.

The refusal is the whole of the command's part in it. `rhei new` does not create
the project (§6), so the help line is advice to whoever ran it and not a step
the command will take on its own. That matters most for a caller who may not
take it either — an agent whose instructions reserve creating a project for a
human has no capture path here at all. Any document that prescribes `--under
basin` to such a caller therefore has to carry this precondition with it and say
what to do instead: report the capture and carry on, rather than create a
project to hold it.

## 4. Ids

A rhei id is derived from the title: lowercased, with every run of characters
outside `[a-z0-9_-]` replaced by a single `-`, trimmed, and with any leading
non-letter dropped — `"Authentication"` → `authentication`, `"Billing & Dunning"`
→ `billing-dunning`. A title that derives nothing usable is an error asking for
`--id`.

A ticket id is the next free number among its siblings: one more than the
highest numeric sibling, starting at 1. Named ids are not derived — a title
never becomes a named ticket id, because ticket ids are referenced by hand in
every `**Prior:**` and a generated name is a worse identifier than a number.
`--id fix-cache` writes one explicitly.

`--id` bypasses derivation in both modes and is validated, not sanitized: an
id that is not legal is refused with the reason, never quietly repaired.

The following ids are refused outright:

- one that **collides** with an existing rhei or sibling ticket, naming the
  holder and pointing at `--id`; a same-id `*.rhei.md` or a directory containing
  `index.rhei.md` is an existing rhei in either requested create layout;
- `basin` as a *rhei* id, which is permanently reserved for the synthetic
  basin rhei ([§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis)) — the refusal is at create time rather than at
  the next load, where it would arrive as a broken project;
- `index` as a *rhei* id, reserved for the same reason: `index.rhei.md` is the
  name that marks a Directory Workspace's index, and writing one beside
  `index.panta.md` would make the project directory read as a workspace as
  well;
- an id that is not a legal single-segment rhei id ([§AR-rhei-panta.3](../architecture/rhei-panta.spec.md#3-identity-and-id-namespacing)) or a
  legal ticket id segment.

A same-id directory that has no `index.rhei.md` is not an existing-rhei
collision. For `--dir` it is either the prospective workspace admitted by
§2.1.1 or an occupied destination; for the default single-file layout it is a
layout conflict. These refusals describe the filesystem conflict truthfully and
leave both the existing directory and the selected output path untouched.

Concurrent creates are serialized by canonical sibling sidecars, taken in a
fixed order. They are the sole writer locks: Rhei never locks a plan
destination, and every authoritative read is through the current destination
pathname after acquisition.

The **scope sidecar** comes first and is held for the whole invocation — before
the first load or selection, through the write, both validation passes, success
or rollback — beside the project manifest (`index.panta.md`) for a project, or
beside the plan file for a lone plan or bare workspace. Holding it project-wide
is what the pre/post validation diff (§5.2) needs.

The **destination sidecar** comes second, beside the plan file the create
actually writes. That is the identity `rhei complete`, `rhei transition`,
`rhei reset`, and `rhei run` use; the scope sidecar alone does not exclude
them. For an absent destination, creation prepares the necessary missing parent
directories while holding the scope sidecar, establishes the destination
sidecar, and only then makes the authoritative absence or adoption decision and
first publication. A coincident scope and destination identity is acquired
once. Taking distinct identities only scope-then-destination prevents a cycle.

Because a provisional destination is selected before its sidecar is acquired,
the decision bytes are **witnessed before the lock and compared after it**. A
create that finds them changed decides again, against the current pathname, up
to three times before giving up and saying another command is rewriting that
plan. Sidecars established for abandoned candidates remain permanent.

Both sidecar locks are released on every exit path, and the OS releases them if
the process dies. Their empty files and every containing directory needed to
preserve their pathname identities do not go away: success, failed validation,
rollback, dry run, re-decision, and later cleanup all retain them. Presence or
age is not ownership; only the OS lock on the open sidecar handle is.
Preparation or lock failure prevents publication. This is participating-writer
exclusion, not read-only observer isolation, external-writer exclusion, atomic
first publication, or crash recovery.

What the lock guarantees is that every create which exits 0 is in the file
afterwards: sibling numbering, the write, and the verification all happen inside
it, so `xargs -P8 rhei new` against one rhei allocates a distinct number per
invocation. Without it a create is a read-modify-write over a whole file, and
concurrent creates lose tickets rather than colliding on an id — a losing writer
overwrites the winner's ticket, and one that then rolls back restores a snapshot
taken *before* the winner's write, deleting committed work and reporting
success. Agent fan-out is the thing Rhei exists to make ordinary, so a bulk
create is an expected use rather than a race worth one re-run.

Plan directories shared with older binaries have an upgrade boundary: stop all
older writers, upgrade every writer, and then resume. Persistent sidecars are
reused and must not be deleted, but their presence does not prove that the
upgrade is complete; live mixed-version writing is unsupported.

## 5. Write, validate, report

### 5.1. A create is verified, not assumed

`rhei new` never writes over a file that is already there. The destination of a
new task file is derived from the id and the title, so a file already sitting
at that name holds someone else's work — the refusal names the path and points
at `--id`, before anything is written. Creating is not editing (§6), and an
unconditional write is editing with the diff thrown away.

Adopting a prospective Directory Workspace does not make its existing root or
authored bundle part of the create. Before preparation, the write's ownership
record distinguishes pre-existing authored content, removable plan data the
invocation creates, and permanent coordination sidecars and ancestors.
Verification and every later cleanup use those three classes, so no
pre-existing path is claimed merely because the create writes another entry
beside it, and no retained coordination path becomes owned plan data.

After writing, `rhei new` loads and validates the project the way
`rhei validate` does, then compares the *whole set of ids* the project holds
against the set it held before the write, and finally *reloads the plan and
looks up the id it just created*. All three are needed, and none implies the
others. Validation says the project still parses; the id-set comparison says
nothing that was already there stopped existing; the reload says the new node is
actually in it.

The id-set comparison is the general guard, and it is general on purpose.
`rhei new` only ever adds, so any id present before the write and absent after
it is work this create destroyed — and the ways a splice can destroy one are
open-ended. A description ending inside an unclosed code fence is the case that
proves it: the fence is written verbatim, every node after the insertion point
becomes fenced text, the project still parses, still validates, and still hands
back the *new* id, so every narrower check passes while three tickets quietly
stop existing. Checking the whole set does not need to know which bug produced
the loss, which is the reason to have it rather than a check per known fault.
That specific fault is refused one step earlier, as an argument error about the
unbalanced fence (§3.4), because "your fence is unclosed" is something the
author can act on and "ids disappeared" is only the last line of defence.

The reload of the created id covers the opposite miss: a block spliced where the
parser ends a section earlier than the writer assumed leaves the project valid
and the ticket absent — and reported as success, that is a file the author has
to debug by hand, one id short and with the next create about to reuse the
number. When the id does not come back from the file it was written to, the
create has failed like any other, names the file, and is rolled back (§5.2).

The id-set comparison runs first, because it is the only one of the three that
does not need the project to load under its state machines: a write that deleted
work has to be undone even in a project that was already failing for a reason of
its own.

A create that leaves the project unloadable has likewise not succeeded, and
reporting it at the next unrelated command is how a small mistake — a mistyped
`--model`, a `--prior` naming nothing — becomes a confusing one.

Replacing an existing plan is a whole-file replacement, so it is written through
a temp file in the same directory and renamed into place, keeping the file's own
permissions. A create interrupted halfway can then leave the plan untouched but
never leave it truncated — an appended ticket is not worth the file it was
appended to. A file that does not exist yet is written directly: there is no
previous content to lose, and a failed create removes it whole (§5.2).

### 5.2. A create answers for the errors it introduced

The validation pass runs twice: once *before* the write and once after it. What
decides the outcome is the difference between them.

The difference is taken over error *strings*, which only works while an error
says the same thing about the same fault however the rest of the project
changes. So it is a rule on validation messages, not only on this command: a
message never enumerates something project-global that a create elsewhere can
change. The project's rhei ids, the node kinds merged from every rhei, the depth
limit merged from every rhei, and the nearest-id suggestion computed from any of
them are all guidance, and they are reported in the diagnostic's `help` rather
than in the error text. Nothing is withheld from the reader — the same words
reach the same screen — but a create that adds a rhei no longer rewrites the
text of an error in a rhei it never touched, and so no longer takes the blame
for it. The `rhei init` on-ramp is where this shows: `rhei init` prints
`rhei new "<title>"`, that create prints `rhei new "<first ticket>" --under
<id>`, and with a single dangling `**Prior:**` anywhere in the project the second
step was refused for an error the first step's own rhei list had reworded.

- Errors the create introduced — a `--prior` naming nothing, a `--states`
  naming a machine no `states.yaml` provides — undo the write: a created file
  is removed, and a modified file is restored byte-for-byte. For an adopted
  workspace, this removes only the invocation-created index and task directory;
  the pre-existing root, state machine, and prompt templates survive
  byte-for-byte. Permanent sidecars and the directories necessary to preserve
  their identities also survive and are not described as authored output. The
  report lists only those new errors, with the validator's own code frames.
- Errors that were already there do not. When the post-write errors are the
  ones the pre-write pass already found, the write is **kept** and the command
  succeeds, with a warning saying the project was already failing validation
  and that the failure is not this create's.

A rhei that will not *parse* follows the same rule, one step earlier: it blocks
creates **into it** — that failure is genuinely this create's business, and the
refusal names the rhei and its parse error — and blocks nothing else. Every
other create loads the project leniently, skipping the unreadable rhei the way
`rhei list` does, and the pre/post diff then keeps the write with the inherited
warning. Basin capture is the case that proves it: `--under basin` is the
operation that has to work while someone is mid-edit somewhere else in the
project, and a strict load anywhere on this path takes it out.

Two consequences follow from being lenient, and both are part of the rule.

A `**Prior:**` or `**Consumes:**` whose leading segment names a *skipped* rhei
is refused, naming that rhei and the parse error that skipped it. The target
rhei is not the only one a create depends on being readable: nothing can check a
reference into a rhei that does not load, so written anyway it passes the
pre/post diff — the reference resolves against no ticket on either side — and
the create then reports that the errors are not its own. That is exactly
backwards. The error that appears the moment the sibling is repaired *is* this
create's `--prior`, sitting in a file nobody has looked at since.

A rhei whose declared `**States:**` machine cannot be resolved does not fail a
create anywhere else in the project. A declaration written before its
`states.yaml` exists is the ordinary half-finished state — it is what
`--keep-on-error` produces on purpose — and resolving strictly makes one empty
rhei take out every create in the project, basin capture included, which
[§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis) says is the one thing that must survive somebody else's
mid-edit. The rhei being written *to* is different, because the new ticket's
starting state comes out of its machine, so that one must resolve. The project
still fails validation for the unresolved machine, identically before and after,
so the create keeps its write and says the failure is not its own — nothing is
hidden, it is only not this create's to refuse.

The second rule is the point of running the pass twice. `rhei new` is the
on-ramp: a project with one broken rhei is exactly the project someone is
trying to add a working rhei to, and a create that refuses until everything
else is fixed refuses precisely when it is most needed. It is also simply
untrue to say "nothing was written because the project would not validate with
it" about a create whose own output is fine — the command would be blaming
itself for the state it found.

Rolling back a create's *own* errors is still the right default, because that
failure is nearly always in the flags rather than in the file: re-running with a
fixed flag is the fix, and a half-created ticket in the way of that re-run is
pure friction. `--keep-on-error` keeps the write for inspection, and then says
that the project is left failing validation. In an adopted workspace it keeps
the new index and task directory as well as the authored bundle it never owned.

### 5.3. Mode confusion is an error

`--dir`, `--states`, `--max-levels`, and `--node-kinds` create a rhei; every
flag in §1.3 creates a ticket. Passing one with the other mode's selector is
refused, naming both flags. A flag silently ignored because the *other* flag
decided the mode is the worst outcome available: the command reports success
and the field the user asked for is not there.

### 5.4. What it prints

The default output names what was created and where, then the one command that
follows:

```text
Created rhei "Authentication" as `authentication` at panta/authentication.rhei.md
Next: `rhei new "<first ticket>" --under authentication`
```

```text
Created ticket auth.4 "Rotate signing keys" [pending] in panta/auth.rhei.md
Next: `rhei list` shows the rhei; `rhei next` picks up the work
```

Both modes print a next step, because both are half of a two-step flow: the
rhei create points at its first ticket, and the ticket create points at the
commands that read what is now there.

`--assignee` earns a note of its own. An assignee means "claimed, in progress"
to the engine and "this is Alice's ticket" to whoever is writing the plan, so
assigning work up front builds a plan `rhei run` will not start — and
`rhei list --ready` and `rhei next` then disagree about the same ticket. The
create says so: the ticket is marked claimed, and `rhei next` and `rhei run`
skip it until `rhei release <id>`. Refusing the flag would be wrong — authoring
a claimed ticket is legitimate — but letting it look like a label is not.

Widening is announced here too, in `rhei new`'s own words rather than
`rhei validate`'s: what a create is about to do is *write into the project*, and
a line ending "validating the whole project" describes a command the user did
not run.

`--json` emits the same facts as an object (`kind`, `id`, `title`, `path`, and
`state` for a ticket) for scripts that create tickets in bulk.

`--dry-run` prints the target path and the exact markdown block, and restores
all plan data while allowing only permanent coordination sidecars and their
necessary containing directories to remain. It gets there by doing the whole create — the
write, both validation passes, the id-set comparison, the reload — and then
rolling back *unconditionally*, success included. Skipping the write would make
the preview a report of the flags: every failure worth previewing (a `--prior`
naming nothing, a `--states` naming no machine, a splice the parser reads
differently than the writer did) is visible only once the bytes are on disk and
the project is reloaded. `--dry-run` is the flag reached for *before* writing,
so previewing happily and then failing for real is the one answer it must never
give; a dry run that would have failed reports the real failure and exits
non-zero. Because the rollback is unconditional, `--keep-on-error` has no effect
alongside it. The same ownership boundary applies to an adopted workspace: dry
run removes only its temporary index and task directory, preserves the
pre-existing directory, machine, and prompt-template tree byte-for-byte whether
validation succeeds or fails, and retains established scope and destination
sidecars and necessary ancestors. JSON fields remain unchanged; residue
guidance belongs on stderr so neither prose nor JSON stdout changes shape.

Together, `--dry-run --json` emits that same object with `"dry_run": true` and
the block under `"markdown"`, so a script can preview a bulk create the same
way it reads a real one. A flag that selects the output format has to keep
working under a flag that only selects whether the write happens: silently
handing prose to a caller that asked for JSON is the §5.3 failure again, in the
one place a caller cannot notice it.

## 6. What `rhei new` does not do

- It does not edit or move anything that already exists. With `--dir`, §2.1.1
  permits creating workspace-owned entries inside one narrowly admissible
  directory, but its existing machine bundle remains authored content. There is
  otherwise no re-titling, re-parenting, or state change — `rhei transition` and
  `rhei complete` own state, and everything else is a file edit.
- It does not scaffold from a template. `rhei instantiate` writes a rhei
  complete with its tickets and its own state machine ([§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification));
  `rhei new` writes an empty one. They are separate because a blank rhei should
  not require a template system to exist.
- It does not create a project. That is `rhei init` ([§FS-rhei-init](rhei-init.spec.md#fs-rhei-init-rhei-init)), and the
  two compose: `rhei init && rhei new "Auth"`.
- It does not write a `states.yaml`. `--states` declares which machine a rhei
  runs under; authoring the machine is `rhei-state-machine-writer` territory
  ([§FS-rhei-state-machine-writer](rhei-state-machine-writer.spec.md#fs-rhei-state-machine-writer-rhei-state-machine-writer)).
- It does not infer `--prior` from `--consumes`, create the producer's
  `**Provides:**`, or repair either side. The normal post-write validation
  rejects an incomplete handoff and rolls the create back; authors must state
  both the data-flow reference and its direct ordering edge
  ([§FS-rhei-plan-language.3.12.1](rhei-plan-language.spec.md#3121-declaration-integrity)).
