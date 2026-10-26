### Task fetch-prs: Fetch and classify human-intervention pull requests
**State:** fetch

Fetch every open GitHub pull request in `oracle/graalvm-reachability-metadata` labeled `human-intervention`.

Use GitHub as the source of truth. Include the PR number, title, URL, labels,
assignees, author, body, review state, CI state, head branch, changed files,
recent comments, review comments, and any Forge human-intervention comment or
log reference.

Classify pull requests into actionable root-cause classes. Prefer classes that
share one investigation and one fix path, such as:

- CI failure PRs from GitHub Actions or status checks; create one child
  `ci-failure-triage` task per PR so each can be restarted and rechecked
- PR needs metadata, test, or Forge workflow changes in `/home/vjovanov/ag/rhei/main/master`
- PR exposes a Forge automation defect in `/home/vjovanov/ag/rhei/main/master/forge`
- PR exposes Native Image or GraalVM behavior in `/home/vjovanov/ag/rhei/main/graalvm/ce` or `/home/vjovanov/ag/rhei/main/graalvm/ee`
- PR needs human product/reviewer input on GitHub before automation continues

Append one child task under this task for each non-empty non-CI class, using
state `deep-analysis` and `**Prior:** Task fetch-prs`. Append one child task
per CI failure PR using state `ci-failure-triage` and `**Prior:** Task
fetch-prs`.
