# REQ-test-isolation: A test writes only inside its own directory

Running this project's tests changes nothing that a person or a gate can see
afterwards. Every automated test writes beneath one private directory of its
own, outside the checkout, named so that no other test can name the same one
([§REQ-cross-platform.6](cross-platform.md#6-a-tests-own-directory-is-its-own-by-construction)). This requirement holds for every feature from the moment it
is specified: an artifact a test leaves behind is a defect of the harness, never
a cost of testing. Reading is held to the same line as writing: what a test
reads is its own too, so its verdict never turns on what another test or the
machine put somewhere shared. [§GOAL-rhei-outcomes](../functional-spec/goals.md#goal-rhei-outcomes-goals)

## 1. The Checkout Is Not A Test's Directory

After the suite has run, the working tree is what it was before: `git status`
reports the same thing it reported before, and no entry the suite created stands
at the checkout root.

That is not tidiness. This repository's own gates read the working tree — the
pre-commit and pre-push hooks of [§AR-ci-release.2](../architecture/ci-release.spec.md#2-local-hooks), and any release or CI step
that asserts a clean checkout — so anything the suite leaves behind reads to
every one of them as a change the author made. An artifact under a name nobody
wrote also costs whoever finds it the work of proving that their own command was
not what wrote it, which is a cost paid by someone other than whoever introduced
it.

A test that must exercise a path inside the repository writes beneath
`scratchpad/`, the one ignored directory kept for that, and never at the
checkout root.

## 2. Where A Spawned Process Writes Is Chosen, Never Derived From Where It Stands

A test that spawns the built binary decides two things, and decides them
independently: the working directory the binary stands in, and the directory its
state goes under. Both are pinned rather than inherited, because a spawned `rhei`
takes its state locations from `HOME` and `XDG_STATE_HOME` and the run registry
those name is machine-wide ([§FS-rhei-run-headless.2](../functional-spec/rhei-run-headless.spec.md#2-the-run-descriptor)) — a test that leaves
either unset publishes into the developer's own state.

A helper that computes one of the two from the other makes them impossible to
vary. Every caller that wants a real directory as the working directory then
gets that same directory as the state home, and gets it silently: a helper that
creates the state directory before the spawn raises no error to notice, and the
conflation stays invisible for as long as every caller happens to pass a
temporary directory. The first caller that passes the checkout — because it
wanted a working directory and said nothing about state — writes into it ([§REQ-test-isolation.1](test-isolation.spec.md#1-the-checkout-is-not-a-tests-directory)).

So a spawn takes the two separately, and a helper offers no way to spell one in
terms of the other. Removing the way is what holds this: a call site corrected
in place leaves the next caller the same mistake to make.

## 3. Which Repository A Spawned Command Acts On Is Chosen, Never Inherited

Every `git` and every `rhei` the suite spawns starts with git's repository
variables gone from the child's environment. The rule is the whole of it rather
than a list: every `GIT_*` the child would otherwise see is removed — whether
this harness process inherited it or a caller had already put it on the command
— and what survives is only what a test sets on that spawn afterwards. One name
is set that way today, `GIT_CEILING_DIRECTORIES`, which bounds a fixture's
ancestor discovery to its own directory. A list is what the next variable gets
left off: `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_PREFIX`,
`GIT_COMMON_DIR` and `GIT_CONFIG_PARAMETERS` are the names observed to do
damage, not the definition of what is taken off.

This is [§REQ-test-isolation.2](test-isolation.spec.md#2-where-a-spawned-process-writes-is-chosen-never-derived-from-where-it-stands) read against a second pair of locations. There the
choice was where a spawned process writes; here it is which repository it acts
on, and git takes that from the environment: with `GIT_DIR` set and
`GIT_WORK_TREE` unset, `git` reads the repository from `GIT_DIR` and only the
work tree from the current directory, so `-C <fixture>` names a work tree and
settles nothing about which repository is written. `GIT_CONFIG_PARAMETERS`
carries configuration through the same inheritance, which is why `-c
core.hooksPath=…` on an outer command cannot contain this either.

The configuration that makes it matter is the one [§REQ-test-isolation.1](test-isolation.spec.md#1-the-checkout-is-not-a-tests-directory) is
written for. Git puts these variables into the environment of every hook it runs
and of everything that hook spawns, and this repository's pre-commit and
pre-push hooks run the whole suite ([§AR-ci-release.2](../architecture/ci-release.spec.md#2-local-hooks)) — so under the gate, and
nowhere else, every fixture acts on the repository being committed to. Its index
is rewritten, its `HEAD` gains a commit the fixture authored, and under a linked
worktree `GIT_DIR` names the *shared* configuration, where a fixture's `git
init` leaves `core.bare` and a `[user]` section that make every later command in
that checkout fail. So the measure is a repository no test named: after the
suite has run it has the `HEAD` it had, the index entries it had, and no
configuration a fixture wrote. A suite run from a hook behaves exactly as a
plain `cargo test` does, or [§REQ-test-isolation.1](test-isolation.spec.md#1-the-checkout-is-not-a-tests-directory) holds everywhere except
where it was needed.

A fixture whose git setup does not succeed fails the test that owns it, and says
which command failed. Only one thing is a skip: `git` not being on the machine
at all, which is the program failing to start. A `git` that started and refused
is a failure of the test, because the test has not exercised what it claims to
— and the refusal is what the inheritance above produces, so a test that skips
on it reports `ok` under the gate and nowhere else. The alternative was both of
those: three tests that quietly tested nothing, and processes contending for one
repository's index until the gate was killed hours later.

Removing the way is what holds this, as in [§REQ-test-isolation.2](test-isolation.spec.md#2-where-a-spawned-process-writes-is-chosen-never-derived-from-where-it-stands). One shared
helper names `git` as a program and applies the removal, every `rhei`
constructor applies the same removal, and no other place in the suite spells
either. A call site corrected in place leaves the next caller the same mistake
to make — which is not hypothetical here: one call site had been corrected by
hand and named two of the six variables that mattered, and the rest of the suite
inherited all of them.

## 4. What A Test Resolves Is What Its Fixture Names, Never What Lies Above It

A test's own directory is the whole of what it reads as well as what it writes.
Where `TMPDIR` points is the developer's choice, and it may lie beneath a rhei
home, beneath the user tier `~/.agent-grounds/rhei/`, or inside a git work tree
of any repository. So what the test gates, and where a process it spawns writes,
are settled by the fixture alone: nothing above the test's directory can stand
in for the thing under test, and nothing above it can become the place a spawned
process writes.

Two discoveries walk upwards on purpose, and the suite meets both:

- **A template asked for by bare name** resolves through every ancestor rhei
  home and the user tier before the built-ins ([§FS-rhei-templates.1](../functional-spec/rhei-templates.spec.md#1-template-discovery)), and an
  empty template home planted in the fixture does not stop that walk. A test
  about a particular template - the shipped one, above all - names it by its
  path, so a same-named template above `TMPDIR` is never the one it gates.
- **A git work tree** is found from wherever a process stands. A spawned agent
  that is told its root learns it from what the run hands it, never from its
  working directory, because inside an enclosing work tree that directory is
  the repository's root rather than the fixture
  ([§REQ-test-isolation.2](test-isolation.spec.md#2-where-a-spawned-process-writes-is-chosen-never-derived-from-where-it-stands)). Bounding discovery with `GIT_CEILING_DIRECTORIES`
  ([§REQ-test-isolation.3](test-isolation.spec.md#3-which-repository-a-spawned-command-acts-on-is-chosen-never-inherited)) hides that symptom but leaves the agent rootless.

The measure is the outcome: a test passes or fails the same way whatever lies
above its directory. A test exposed to one of these discoveries carries the
proof itself, by planting inside its own directory - above where its commands
run - the thing that would otherwise stand in: a same-named template in a rhei
home, or a git work tree enclosing the project.

## 5. A Test's Home Is Its Own, And Never Another's

A test's verdict depends on the code it exercises and on nothing that another
test, or the machine it runs on, controls. The case that made this a rule is
the home directory. Settings, templates, tooling and snapshot records all have
a user tier read from under the home, and the home is named by `HOME` — one
variable for the whole test process, which every test in a binary shares.

So no test changes the process's `HOME` to give itself a home. A test that
needs a user tier gets a home of its own by a means no other test can see, and
a test that names no home sees an empty one: neither a sibling's, nor the real
user's. Either of those turns one test's verdict into a question of which
sibling the scheduler ran beside it, or of what the developer happens to keep
in `~/.config/rhei` — and the failure then names a file and a key that the
change under test never touched, so whoever meets it first has to prove it is
not theirs. A lock taken by the tests that move `HOME` does not hold this: it
orders only the tests that take it, and a test that reads the user tier without
knowing it does is exactly the one that never would.

The measure is the process's `HOME` itself. Whatever directory it names — one
holding settings that are invalid on purpose, or the developer's own — every
test in the binary reaches the same verdict, and no test's run leaves it naming
anything other than what it named before. What this point governs is the user
tier read through the home. Where a test's account state goes is not that: with
`XDG_STATE_HOME` unset, the state directory still falls back to `HOME`, so a
test that keeps state needs `HOME` or `XDG_STATE_HOME` to name somewhere, and
this point says nothing about it; [§REQ-test-isolation.6](test-isolation.spec.md#6-a-tests-state-directory-is-its-own-and-never-anothers) does. This governs the tests only:
the built `rhei` still takes its user tier from `HOME`, as
[§REQ-test-isolation.2](test-isolation.spec.md#2-where-a-spawned-process-writes-is-chosen-never-derived-from-where-it-stands) relies on when a test pins it for a spawned binary.

## 6. A Test's State Directory Is Its Own, And Never Another's

The state directory is the account's: the run registry and every root guard's
lock live beneath it ([§FS-rhei-recover.4](../functional-spec/rhei-recover.spec.md#4-pending-root-interlock)), and it is named by `XDG_STATE_HOME`
— again one variable for the whole test process. Nearly every test takes a root
guard without knowing it does, because reading a plan or a source file takes
one, so a test that moves that variable to keep its registry private moves the
lock of every guard taken beside it. The sibling then locks under a directory
it never named: one being created, one being deleted, or one where a test put a
regular file on purpose to make the registry unwritable. It fails with "cannot
create root guard lock" on a path that belongs to neither the code under test
nor the test that met it.

So where a test's root guard locks is fixed for the test, by a means no other
test can move: a sibling repointing `XDG_STATE_HOME`, removing it, or blocking
`<state>/rhei` with a file changes nothing about where a guard taken beside it
resolves. A lock taken by the tests that move the variable does not hold this,
for the reason [§REQ-test-isolation.5](test-isolation.spec.md#5-a-tests-home-is-its-own-and-never-anothers) gives: it orders only the tests that take
it, and the tests that take a guard are almost all the others.

The measure is the guard-taking test's verdict. While another test holds
`XDG_STATE_HOME` pointed at a directory whose `rhei` is a regular file, a forced
transition and its replay still commit, on every platform the gate runs. This
governs the tests only: the built `rhei` still resolves its state directory, and
every lock beneath it, as [§FS-rhei-recover.4](../functional-spec/rhei-recover.spec.md#4-pending-root-interlock) says.
