# Fix block

`fix` is the reusable application half of `changeset-review`. It accepts the
review block's `decision` state-file endpoint, prepares a workspace, applies and
verifies the fix, and optionally records a commit or pull request. It contributes
states to the task supplied by the preceding block rather than creating a
second plan task.

```sh
rhei instantiate \
  --mount review=code-review --mount fix=fix \
  --set review.change_ref=HEAD~3 \
  --seam review.done=fix.entry \
  --pass review.decision=fix.decision
```
