# Code review block

`code-review` is the reusable review half of `changeset-review`. It begins at
`split`, retains the review/validation/proposal fan-out states, gates the final
decision at `human-review`, and exposes that decision as the `decision`
state-file output.

```sh
rhei instantiate --mount review=code-review \
  --set review.change_ref=HEAD~3
```
