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
