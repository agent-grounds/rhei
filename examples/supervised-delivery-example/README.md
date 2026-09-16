# supervised-delivery — example

A pre-rendered instantiation of the
[`supervised-delivery`](../../crates/rhei-cli/templates/supervised-delivery/)
template, checked in as the template's smoke test. This is the canonical
**subtree supervision** reference among the templates: one root task in a
`supervising` state (`execute_on: child-terminal`) that briefs, routes, and
cancels the ten steps beneath it.

## Inputs used

```yaml
spec_path: docs/functional-spec/rhei-supervision.spec.md
title: Deliver subtree supervision
supervisor_target: claude-code[yolo]:anthropic:claude-opus-4-7
implementer_target: claude-code[yolo]:anthropic:claude-opus-4-7
reviewer_target: codex[xhigh]:openai:gpt-5.5
pm_target: claude-code[yolo]:anthropic:claude-opus-4-7
fixer_target: claude-code[yolo]:anthropic:claude-opus-4-7
coverage_target: codex[xhigh]:openai:gpt-5.5
docs_target: claude-code[yolo]:anthropic:claude-opus-4-7
review_rounds: 2
coverage_rounds: 1
docs_rounds: 1
ci_commands:
  - cargo fmt --all -- --check
  - cargo clippy --workspace --all-targets -- -D warnings -W clippy::all
  - cargo test --workspace --all-targets --no-fail-fast
review_focus:
  - concurrency
  - error handling
supervisor_session: false
```

The same values are checked in at `instantiation-values.yaml`.

`review_rounds: 2` is deliberate: it is the value that exercises the `k > 1`
branch of the unrolled rounds, where `review-2` and `pm-2` chain off `fix-1`
and consume the previous round's `findings` and `resolutions` exports. The
non-empty `ci_commands` and `review_focus` lists exercise the other two
conditional branches in `states.yaml`.

The one branch this example does **not** cover is `supervisor_session: true`,
which adds a `snapshot:` block to the supervising state. That block is only
legal on a session-capable agent — of the built-in profiles, `pi` — so an
example carrying it would not run with the `claude-code` targets every other
example uses. It is covered by an end-to-end test instead
(`tests/e2e/supervised_delivery_tests.rs`).

## What it shows

```text
supervisor prepares
    -> implement
    -> [ code review  ||  product review ] -> fix     x 2 rounds
    -> coverage audit -> fix                          x 1 round
    -> documentation                                  x 1 round
    -> supervisor writes the delivery result
```

Every child state declares a required input at
`runtime/supervise/<task-id>.md`, so nothing runs until the supervisor writes
that step's brief. A dry run therefore shows one ready ticket and ten held
ones:

Each `**Consumes:**` producer also appears directly in the consumer's
`**Prior:**`; the explicit edges make export ordering visible to validation,
blocked-task listings, and graph rendering.

```text
Pass 1: 1 ready, 0 terminal, 11 total.
Ready: Task supervised-delivery-example.deliver: Deliver subtree supervision
10 ticket(s) held by supervisor Task supervised-delivery-example.deliver
would transition: ...  supervising -> supervising (release)
```

## Validate

```bash
rhei validate examples/supervised-delivery-example
rhei run examples/supervised-delivery-example --dry-run --parallel 2
```

Run it for real with `--parallel 2` or more, so the code review and the product
review of a round overlap.

## Regenerate

```bash
rm -rf examples/supervised-delivery-example
rhei instantiate crates/rhei-cli/templates/supervised-delivery \
  --values crates/rhei-cli/templates/supervised-delivery/.example-values.yaml \
  --output examples/supervised-delivery-example
```

After regenerating, restore this README and `instantiation-values.yaml` from
the checked-in copies — both are example-owned files that instantiation does
not produce.
