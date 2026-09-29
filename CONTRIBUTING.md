# Contributing to Rhei

## Install the hooks first

The repository's gates run locally as [pre-commit](https://pre-commit.com) hooks,
so they can tell you what is wrong while the branch is still in front of you
rather than after CI has gone red:

```bash
pre-commit install --install-hooks
```

That installs the pre-commit, pre-push and commit-msg hooks the repository
declares. Never get past a hook with `--no-verify`.

## The changelog rule

Every pull request adds a bullet of its own under `## Unreleased` in
[`docs/changelog.md`](docs/changelog.md), describing the change in the terms
someone reading release notes would want.

**Do not write the pull request number.** You cannot know it: it does not exist
until the pull request is opened, which is after the push that would have to
check it. The release stamps `(PR #N)` onto your bullet when it cuts the version.
Write `(PR #TBD)` if you would rather leave a visible placeholder — that is
accepted too, and the release replaces it.

The pre-push hook refuses a push whose `## Unreleased` section holds no new or
changed bullet against your branch's base, and says what to add. Rewrapping or
renumbering a bullet somebody else wrote is not a bullet of your own.

For a branch that is not becoming a pull request — a spike, a scratch branch —
push it past the check explicitly:

```bash
SKIP=changelog-pr-entry git push
```

That skip is local, and CI does not honour it, so the pull request is still
checked.

## The rest of the gates

[`AGENTS.md`](AGENTS.md) lists the commands CI runs and the grounding rules the
repository is held to — every spec, decision and test carries a stable `§` id,
and `grund check .` verifies that the citations resolve.
