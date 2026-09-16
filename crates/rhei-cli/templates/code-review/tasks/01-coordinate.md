### Task coordinate: Coordinate review of {{change_ref}}
**State:** split

Resolve and review `{{change_ref}}`, validate the findings, propose fixes, and
record the approved decision before completing the review block. Resolve the
change to a concrete set of files, write an architectural overview, then split
it into logical parts. For each part, append a `review-<slug>` task to `tasks/`
with `**State:** review` and `**Prior:** Task coordinate`. Append one
`aggregate` task with `**State:** aggregate-reviews` and `**Prior:**` listing
every review task. When the overview and parts manifest are written and all
follow-up tasks are appended, transition to `completed`.
