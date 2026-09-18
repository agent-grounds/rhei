# FS-rhei-init: `rhei init`

Set up a Panta project. By default `rhei init` creates a **`panta/`** folder
inside the host directory and makes *that* the project: the plans and their
generated output live in one place, out of the host repository's way, and the
folder is gitignored — planning state is treated as working material of the
repository, not part of its content. `--here` instead makes the host
directory itself the project (the adoption mode for a directory that already
holds plans meant to be versioned).

Init writes the one file a project requires — the `index.panta.md` manifest —
plus the low-cost conveniences that keep a project healthy from day one:
ignore rules, and an agent-discovery note so coding agents working in the
repository find the Rhei workflow on their own. Everything else about a
project (rheis, tickets, state machines) is added with `rhei new`
([§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new)) or by creating files by hand, per [§FS-rhei-panta.2](rhei-panta.spec.md#2-default-home-for-new-rheis).

Initialization is a convenience, not a gate: a project is defined by its
files, and a hand-written `index.panta.md` is exactly as valid as a generated
one ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)). This deliberately contrasts with database-backed
trackers, where `init` must provision storage before anything works.

## 1. Usage

```bash
rhei init [DIR] [--here] [--title <title>] [--no-agents] [--force]
```

`DIR` is the **host** directory (default: the current directory; created when
missing). The project directory is `DIR/panta/`, or `DIR` itself with
`--here`.

### 1.1. Options

| Flag                | Default            | Description                                             |
|---------------------|--------------------|---------------------------------------------------------|
| `--here`            | off                | Make `DIR` itself the project instead of `DIR/panta/`. The mode for adopting a directory of existing, versioned plans |
| `--title <title>`   | from the host      | Project title written to the manifest heading           |
| `--no-agents`       | off                | Skip the `AGENTS.md` agent-discovery note (§4)          |
| `--force`           | off                | Re-initialize an existing project: overwrite the manifest (re-deriving the title); companion files update in place. Pair with `--here` when the host itself is the project (§2) |

The default title is derived from the **host** directory's name in both
modes — `panta/` is a location, not an identity: `-` and `_` become spaces
and each word is capitalized (`my-project` → `My Project`).

## 2. Behavior

Before any initialization effect, discover the affected existing execution-root
owners of both the host and destination and acquire their shared access guards
in sorted canonical order (§FS-rhei-panta.6.6). Check pending recovery before
and after acquisition and retain the guards through all writes
(§FS-rhei-recover.4). This includes host ignore/agent-note updates and creation
of a new destination: a missing `host/panta` never exempts an existing host from
the boundary. Neither `--force` nor `--here` bypasses it.

1. **Refuse an existing project** unless `--force`. When the project
   directory already contains `index.panta.md` — and, in default mode, also
   when the host itself is already a project — the command fails stating the
   directory is already a project, names the re-init path, and changes
   nothing. With `--force` the manifest is rewritten from scratch — a
   hand-edited manifest is deliberately clobbered, which is why force is
   opt-in — and the companion files update in place: the `.gitignore`
   entries and the marked `AGENTS.md` block are idempotent, so a forced
   re-init never duplicates them. A host that is itself a project refuses
   default mode even under `--force` — the refusal names `--force --here` —
   because force means re-initialize, never "nest a fresh `panta/` project
   inside this one": the child would lose every target resolution to the
   host manifest ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)) and could never be reached by a bare
   command.
2. **Refuse to shadow existing plans** in default mode. When the host holds
   bare `*.rhei.md` files or workspace rheis, a `panta/` project would not
   discover them; the error offers both fixes — `--here` to adopt them in
   place, or moving them into `panta/` first. `--here` and `--force` skip
   this check. The mirror-image check guards `--here`: when the host is not
   itself a project but already holds a default-mode project at `panta/`,
   adopting the host would shadow that project — target resolution prefers
   the host manifest ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)), so every ticket in `panta/` would
   become unreachable by inference. The command refuses, names the child
   project, and offers the fixes (keep using `panta/`, or move its contents
   into the host and remove it first); `--force` does not skip this refusal,
   for the same reason it cannot nest a fresh `panta/` inside a host project
   (point 1).
3. **Warn about an enclosing project.** When an ancestor directory contains
   `index.panta.md`, init proceeds but warns on stderr: nested projects are
   almost always a mistake, and the outer project will not discover the inner
   one ([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout) discovery does not recurse into rhei roots).
4. **Write the manifest**: `# Panta: <title>`, and nothing else. The state
   machine is per-rhei, defaulted by the project ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)): a
   discovered rhei that declares its own machine runs under it, and one that
   declares nothing was authored against the built-in default and keeps it.
   There is nothing for init to adopt — writing a discovered machine into the
   manifest would silently re-govern every *future* rhei that declares
   nothing. (An earlier revision adopted a unanimously-declared machine as
   the project default, back when a divergent member was a load error; the
   per-rhei model removed both the error and the need.)
5. **Seed ignore rules** (§3).
6. **Write the agent-discovery note** unless `--no-agents` (§4).
7. **Report what became visible** (§5).

## 3. Ignore rules

In default mode the whole project folder is ignored at the host —

```gitignore
panta/
```

— because a `panta/` project is working state by design. The project folder
additionally gets its own `.gitignore` covering generated output, so a user
who later decides to version their plans (by deleting the host's `panta/`
entry) does not start committing runtime state:

```gitignore
runtime/
.rhei/cache/
```

With `--here` only the generated-output entries are seeded, into the host's
`.gitignore` — adopted plans are assumed to be content worth versioning.

In both modes entries are appended one at a time and only when missing, so a
hand-maintained ignore file is never rewritten or reordered.

## 4. Agent-discovery note (`AGENTS.md`)

Rhei projects are driven by coding agents, and an agent dropped into a
directory has no way to know it plans work with Rhei. Init creates
`AGENTS.md` in the **host directory** — the directory `rhei init` was given —
or appends to an existing one, with a short note between stable markers naming
where the project lives. Every path init writes is one it chose inside the
host directory; where the user has symlinked a host instruction file to an
ancestor, the bytes land wherever they pointed it, because following a pointer
somebody set deliberately is honoring it, not escaping the host.

One exception: when the host has no `AGENTS.md` but does have a `CLAUDE.md`,
the note is appended to `CLAUDE.md` instead. A project whose agent
instructions live only in `CLAUDE.md` has an agent that never opens
`AGENTS.md`, so creating one there files the note where nobody reads — the
note goes into the instruction file that is actually used. A host with both
files (including the common `CLAUDE.md → AGENTS.md` symlink) keeps `AGENTS.md`
as the target, and re-running init finds and rewrites the note in whichever
file carries it. Init names the file it changed either way, in the host-changes
list (§5).

The note:

```markdown
<!-- rhei:begin -->
## Rhei

The Rhei (Panta) project for this repository lives in `panta/`. Plans are
`*.rhei.md` files and workspace directories; ticket ids are
project-qualified (`<rhei>.<id>`). Add work with
`rhei new "<title>" --under <rhei>`, and capture a ticket that has no
rhei yet with `--under basin`. Work tickets with `rhei list`,
`rhei next`, and `rhei complete`; validate edits with `rhei validate`.
Orchestration (`rhei run`) is started by humans, never by agents.
<!-- rhei:end -->
```

The note is anchored at the host because the host is the only directory the
user named. An earlier revision anchored it at the enclosing **repository
root** — the nearest ancestor containing `.git` — so that a directory of plans
adopted with `rhei init <subdir> --here` was advertised where coding agents
read. That walk cannot tell a plans subdirectory of the repository the agent
works in from a host that merely happens to sit inside an unrelated
repository, and in the second case init appended the note to a tracked,
hand-written instruction file the user never named. An enclosing repository's
`AGENTS.md` or `CLAUDE.md` is therefore never modified; init reads one only to
word the hint below.

Discoverability from an enclosing root is the user's call, and init only
prompts it: it prints one hint line naming that root's instruction file, so a
user who wants agents starting there to find the project can add a pointer
themselves. The hint is a suggestion, never a write. Three conditions have to
hold together for it to print — the note is being written (`--no-agents` says
nothing at all), the host lies strictly inside a git repository (a host that
*is* the root has nothing above it to point from), and that root's instruction
file does not already read as carrying a Rhei note — a marker-delimited
region, or a `## Rhei` section whose body still carries the note's own
sentence. That last one is a test on the text, not a claim about who wrote it.
The case it is for is the upgrade: a note an earlier revision wrote to the
enclosing root is still sitting there, and asking again for a pointer that is
already on the page is noise. But init cannot tell that note from one a user
wrote by hand, or from one pointing at a different project, and it does not
try — judging whose note it is means ruling a user's own words about Rhei out
of a file init has just promised not to write. A root that already reads as
spoken for gets no hint, whoever spoke for it. Init leaves what it found where
it is — removing it is the user's call, in their own file. Deciding which of
the root's `AGENTS.md` or `CLAUDE.md` to name, and whether one of them reads
as carrying a note, is the whole of what init reads above the host for.
Otherwise the hint follows the layout, not the write: a re-run that changes
nothing still prints it, because it describes where the project sits rather
than a file init touched.

The first sentence of the note names where the project is relative to the
host: `panta/` in default mode, and "This directory is a Rhei (Panta)
project." with `--here`, where the host is the project itself. The note deliberately names only the worker surface —
`list`, `next`, `complete`, `validate` — and marks `rhei run` as
human-initiated: orchestration spawns agent fleets and spends money, so an
agent must never be instructed to start it. Rewriting the note first strips every trace of a previous
one — marker-delimited regions, orphaned markers, and a marker-less `## Rhei`
section still carrying the note body — so init is idempotent even after a
third-party merge mangled the markers, and removal is one block deletion.
Stripping only ever removes the note's own material: an orphaned begin
marker (its end marker lost) is removed alone, never together with the user
content that follows it. Two boundaries make that exact, because a strip that
guesses wide deletes from a file init was only asked to append to. A begin
marker pairs with an end marker only when no second begin marker stands
between them, so two begin markers before one end leave the first orphaned and
the lines between the two — user content a merge stranded there — are kept
rather than read as the inside of one long region. And a marker-less `## Rhei`
section ends where the note's prose ends: at the blank line closing the
paragraph that carries the note's own sentence, or at the next `## ` heading
or begin marker if one comes first, never at the end of the file — so trailing
user content under no heading of its own survives.
Richer per-agent integration (skills for Claude Code, Cursor, …) stays with
`rhei install-skills` ([§FS-rhei-install-skills](rhei-install-skills.spec.md#fs-rhei-install-skills-rhei-install-skills)); init's final output points
at it rather than duplicating it.

## 5. Discovery report

After writing, init loads the project through the ordinary discovery pass
([§AR-rhei-panta.1](../architecture/rhei-panta.spec.md#1-on-disk-layout)) and reports where it lives and what it contains:

```text
Initialized Panta project "My Project" at panta/ with no rheis yet. Add one
by dropping a `<id>.rhei.md` file or a workspace directory next to
index.panta.md.
```

An adoption (`--here`) over existing plans reports the discovered rheis
(`with 2 rheis: auth, billing`). When discovery fails — for example a plan
file whose stem is not a valid rhei id — init still succeeds (the manifest is
written) and surfaces the load error as a warning, so init doubles as a first
validation.

Init also **names every file it wrote or changed outside the project
directory**, and states the gitignore consequence in default mode:

```text
Also changed in the host directory: .gitignore, AGENTS.md
Note: `panta/` is gitignored — planning state is working material, not
repository content. Delete that entry to version the project.
```

Init runs inside someone else's repository and edits two files there that the
user did not name. Writing them silently is the surprise: a team discovers
weeks later that no plan was ever committed, and the `.gitignore` line that
caused it was never mentioned. Only files actually changed are listed —
re-running init over an up-to-date `.gitignore` and `AGENTS.md` reports
neither, because the entries and the marked block are idempotent (§2).

The omitted-plan-target resolution knows the convention: a `panta/` child
containing `index.panta.md` resolves as the project for commands run in the
host directory or anywhere under it ([§FS-rhei-panta.6](rhei-panta.spec.md#6-project-scope-and-command-behavior)), so after init, bare
`rhei list` works from the whole repository.

## 6. What init does not do

- It does not create rheis or tickets. `rhei new` is the verb for that
  ([§FS-rhei-new](rhei-new.spec.md#fs-rhei-new-rhei-new)) and composes: `rhei init && rhei new "Auth"`.
- It does not move existing plan files into `panta/` — adopting in place is
  `--here`; moving content is left to the user (and note that moving a
  *tracked* plan into the gitignored `panta/` untracks it).
- It does not scaffold from templates — that is `rhei instantiate`
  ([§FS-rhei-templates](rhei-templates.spec.md#fs-rhei-templates-rhei-templates-specification)).
- It does not touch git beyond `.gitignore`: no hooks, no commits.
- It does not write a state machine or a `**States:**` line — the manifest
  stays bare (§2); each rhei keeps the machine it declares, and the built-in
  `rhei` machine covers the rest ([§FS-rhei-states](rhei-states.spec.md#fs-rhei-states-rhei-states-specification)).
