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

## The changelog

A pull request never touches [`docs/changelog.md`](docs/changelog.md). The
release writes it from the titles of the pull requests merged since the previous
release, so give a pull request the title you want in the notes
([§FS-rhei-distribution.5.1](docs/functional-spec/rhei-distribution.spec.md#51-what-a-releases-notes-list)).

## The rest of the gates

[`AGENTS.md`](AGENTS.md) lists the commands CI runs and the grounding rules the
repository is held to — every spec, decision and test carries a stable `§` id,
and `grund check .` verifies that the citations resolve.
