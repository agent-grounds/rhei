# Hourly Human Intervention Example

This is a pre-rendered instantiation of the
`hourly-human-intervention` template using the inputs checked in at
`instantiation-values.yaml`, which are its defaults.
It includes the default human-intervention sweep plus RHEI-labeled PR
review-comment follow-up.

## Inputs

| Input | Value |
|---|---|
| `repo` | `oracle/graalvm-reachability-metadata` |
| `label` | `human-intervention` |
| `forge_checkout` | `master` |
| `graalvm_checkout` | `graalvm` |
| `graalvm_ce_checkout` | `graalvm/ce` |
| `graalvm_ee_checkout` | `graalvm/ee` |
| `analysis_target` | `codex[workspace]:openai:gpt-5.5` |
| `forge_fix_target` | `codex[workspace]:openai:gpt-5.5` |
| `graalvm_fix_target` | `codex[workspace]:openai:gpt-5.5` |
| `review_target` | `codex[workspace]:openai:gpt-5.5` |
| `github_reviewers` | `[]` |

## Validate

```bash
cargo run -p rhei-cli -- validate examples/hourly-human-intervention-example
```

## Regenerate

```bash
rm -rf examples/hourly-human-intervention-example
cargo run -p rhei-cli -- instantiate hourly-human-intervention \
  --values crates/rhei-cli/templates/hourly-human-intervention/.example-values.yaml \
  --output examples/hourly-human-intervention-example
```

Its four checkout inputs are paths, which render as absolute paths under the
directory the command runs in, so the rendered files differ with every checkout
location. That is why `tests/e2e/template_example_sync_tests.rs` leaves this
example out. Restore this README and `instantiation-values.yaml` after
regenerating.
