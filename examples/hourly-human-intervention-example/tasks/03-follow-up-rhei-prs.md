### Task follow-up-rhei-prs: Follow up on RHEI pull requests
**State:** rhei-pr-follow-up

Fetch every open GitHub pull request in `oracle/graalvm-reachability-metadata` labeled `rhei`.

For each PR, inspect review decisions, review comments, issue comments,
requested changes, CI status, mergeability, head branch, changed files, and the
repository that owns the head branch.

Address reviewer comments in an isolated git worktree when the PR needs code,
metadata, test, workflow, or documentation changes. Push fixes to the PR head
branch. If every reviewer has approved and required checks are passing, merge
the PR using the repository's standard merge method.
