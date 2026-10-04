# Rhei: Hourly Human Intervention Sweep

## Overview
Run this workspace once per hour to drain `{{label}}` work from
`{{repo}}`.

The sweep has three entry tasks:

- open GitHub issues carrying `{{label}}`
- open GitHub pull requests carrying `{{label}}`
- open GitHub pull requests carrying `rhei`

Each entry task fetches the full queue, classifies items into root-cause
classes, and appends one child task per class. CI failure items are split into
one child task per issue or PR so each can be restarted, rechecked, and either
completed as transient/infra or routed into the fix pipeline. Each non-CI class
task then runs the shared fix state machine:

1. deep cause analysis
2. routing decision
3. one of:
   - CI failure triage, restart/rerun, and completion or routing
   - human GitHub issue/comment handoff
   - Forge fix, review, fix, review, publish/add reviewers
   - GraalVM proposal, human review, fix, review, fix, review, publish/add reviewers

The `rhei` PR follow-up task works independently from `{{label}}` triage. It
finds open PRs labeled `rhei`, addresses reviewer comments in isolated
worktrees, and may merge a PR only when reviewers approve and repository gates
are green.

Use `{{forge_checkout}}` for Forge and reachability-metadata fixes. Use
`{{graalvm_ce_checkout}}` and `{{graalvm_ee_checkout}}` for GraalVM code
changes; `{{graalvm_checkout}}` is only the coordination directory containing
those checkouts. Refresh the local GraalVM from
`{{graalvm_ee_checkout}}/vm-enterprise` with `mxb ee` before validations that
depend on the GraalVM build.
