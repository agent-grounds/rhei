# REQ-test-isolation: A test writes only inside its own directory

Running this project's tests changes nothing that a person or a gate can see
afterwards. Every automated test writes beneath one private directory of its
own, outside the checkout, named so that no other test can name the same one
([§REQ-cross-platform.6](cross-platform.md#6-a-tests-own-directory-is-its-own-by-construction)). This requirement holds for every feature from the moment it
is specified: an artifact a test leaves behind is a defect of the harness, never
a cost of testing. [§GOAL-rhei-outcomes](../functional-spec/goals.md#goal-rhei-outcomes-goals)

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
