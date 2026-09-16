# Code review block

`code-review` is the reusable review half of `changeset-review`. It begins at
`split`, retains the review/validation/proposal fan-out states, gates the final
decision at `human-review`, and exposes the decision written by `decide` as
the `decision` state-file output. The approval gate requires that file before
entry. State prompt files retain the review duties and resolve mounted state
names and artifact paths from the compiled machine.

```sh
rhei instantiate --mount review=code-review \
  --set review.change_ref=HEAD~3
```
