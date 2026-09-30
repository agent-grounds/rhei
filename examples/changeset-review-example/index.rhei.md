# Rhei: Changeset Review — PR#42
**States:** changeset-review

## Overview
Two-agent changeset review with validation, fix proposals, smart adjudication,
and smart final fixes.

The change under review (`PR#42`) can be any of:

- a PR URL or PR number
- a branch name
- a commit SHA or range `base..head`
- a path to a `.diff` / `.patch` file

The coordinator resolves the reference to a concrete set of changed files
before splitting.

The instantiated workspace is a review scratchpad. Agents must resolve the
repository root via Git and inspect or edit repository files from that root,
not from this workspace directory.

Flow:

1. The coordinator task resolves `PR#42`, writes an architectural
   overview, splits the change into logical parts, appends one `review-<slug>`
   task per part, and appends one `aggregate` task whose `**Prior:**` waits on
   every part review.
2. Each part is reviewed independently by every configured review target:
   - `claude-code[yolo]:anthropic:claude-opus-4-7`
   - `codex[xhigh]:openai:gpt-5.5`

   Each reviewer must organize findings into these focus subsections:
   - `performance`
   - `security`
   - `concurrency`
3. The smart target (`codex[xhigh]:openai:gpt-5.5`) deduplicates review findings into
   candidate issues.
4. Candidate issues are validated independently by:
   - `claude-code[yolo]:anthropic:claude-opus-4-7`
   - `codex[xhigh]:openai:gpt-5.5`
5. Fix proposals are produced independently by:
   - `claude-code[yolo]:anthropic:claude-opus-4-7`
   - `codex[xhigh]:openai:gpt-5.5`
6. The smart target (`codex[xhigh]:openai:gpt-5.5`) aggregates those proposals into a
   proposal matrix.
7. The smart target (`codex[xhigh]:openai:gpt-5.5`) decides discrepancies and writes the
   final fix plan.
8. A human reviews the final fix plan and explicitly approves the fix phase.
9. The smart target applies the accepted fixes in a `worktree` workspace and performs the `pr` commit step.

## Notes

- The workspace is "living": the coordinator appends review and aggregate
  task files under `tasks/` during the run. `rhei reset` clears state but does
  not delete dynamically appended task files.
- Instantiate the workspace inside the repository under review, ideally under
  `.agent-grounds/scratchpad/`, so `git rev-parse --show-toplevel` from the workspace
  resolves the project root deterministically.
- The bundled settings add `codex[xhigh]`, which passes
  `model_reasoning_effort="xhigh"` to Codex. The default smart target is
  `codex[xhigh]:openai:gpt-5.5`. Claude Code is included as a second default
  reviewer, but Rhei does not currently expose a Claude reasoning-effort flag.
