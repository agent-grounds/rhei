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

A pull request adds no changelog bullet and leaves
[`docs/changelog.md`](docs/changelog.md) alone. The issue it closes already says
what changed, so nothing checks a branch for a bullet.

The changelog is written before a release instead, by whoever cuts it, in one
pull request of its own that lands before either release helper runs
([§FS-rhei-distribution.5.1](docs/functional-spec/rhei-distribution.spec.md#51-what-a-releases-notes-list)):

- The list is every pull request merged since the previous release tag that
  changed more than docs and CI. Docs and CI are the paths under `docs/` and
  `.github/`, every `*.md` file, and the root `LICENSE` and `lychee.toml`.
- Each pull request on the list gets a bullet under `## Unreleased`, or is named
  in one.
- A bullet's words come from the issues its pull request closed, or from the
  pull request's own description where it closed none.
- Every bullet ends in its own pull request's number, written `(PR #N)`.
- A pull request whose bullet is already pending is skipped.

From a checkout with the release tags fetched (`git fetch --tags`), this prints
that list, each pull request with the issues it closed:

```bash
tag="$(git tag --list 'v*.*.*' --sort=-v:refname | head -n1)"
gh pr list --state merged --base main --limit 200 \
  --search "merged:>$(git log -1 --format=%cI "$tag")" \
  --json number,title,body,files \
  --jq '.[] | select(any(.files[].path; test("^(docs/|\\.github/)|\\.md$|^(LICENSE|lychee\\.toml)$") | not))
        | "#\(.number) \(.title) [closes: \([.body | scan("(?i)\\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\\s+#([0-9]+)")[] | "#\(.)"] | join(" "))]"'
```

Until the section is written, the scheduled `Auto bump` holds: its run ends
green with a notice naming the changes that wait for their bullets
([§FS-rhei-distribution.5.3](docs/functional-spec/rhei-distribution.spec.md#53-when-the-scheduled-release-waits)).
A release started by hand refuses an empty section instead.

## The rest of the gates

[`AGENTS.md`](AGENTS.md) lists the commands CI runs and the grounding rules the
repository is held to — every spec, decision and test carries a stable `§` id,
and `grund check .` verifies that the citations resolve.
