# Rhei: Two-Pass Review Artifact Followed by Fix

## Overview
This workspace demonstrates a counted review/fix loop that writes one review
artifact across two total passes and updates one fix artifact across two total
passes.

## Notes

- The task starts directly in `review`.
- The `review` state declares `visits: 2`, so the task visits `review` and
  then `review-2`.
- The `fix` state also declares `visits: 2`, so the task visits `fix` and
  then `fix-2`.
- Each exit from `review` appends one section to
  `runtime/reviews/task-<id>-review.md`.
- The first fix pass returns to `review-2`.
- The second fix pass updates `runtime/fixes/task-<id>-fix.md` and then the
  task finishes in `completed`.
