# Product Management Template

This template creates a directory workspace for a repeated product-management loop. Multiple PM agents independently propose product entries, a stronger agent merges and validates every entry into a bounded implementation slice, and a cheaper implementation agent applies the accepted slice. The default loop runs twice.

## Inputs

| Name | Type | Default | Description |
|---|---|---|---|
| `plan_title` | string | `Product Management Run` | Title for the instantiated workspace. |
| `product_name` | string | required | Product, project, or feature area being managed. |
| `product_brief` | string | required | Product context, customer problem, or strategic goal for every agent. |
| `implementation_scope` | string | `docs/functional-spec, docs/decisions, README.md` | Paths or artifacts the implementation agent may change. |
| `pm_targets` | array<object> | Claude and Codex targets | Agents that independently produce PM entries each pass. Configure this array to add or replace targets. |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that aggregates, validates, prioritizes, and slices work. |
| `implementation_target` | string | `codex[medium]:openai:gpt-5.4-mini` | Cheaper agent that implements the accepted slice. |
| `loop_passes` | number | `2` | Number of PM aggregate implementation cycles. |
| `focus_areas` | array<string> | empty | Optional focus areas every PM entry set should address. |
| `validation_criteria` | array<string> | user value, evidence, bounded scope, conflicts | Acceptance criteria for validated entries. |
| `max_entries_per_pass` | number | `3` | Maximum accepted entries implemented in one pass. |

## Task Paths

| Task kind | State path | Purpose |
|---|---|---|
| Product loop | `(product-run -> aggregate-validate -> implement) x loop_passes -> completed` | Fan out PM entries, validate and slice them, then apply the accepted product changes. |

## Flow

1. Instantiate the template with a product name and product brief.
2. The `product-run` state fans out to every configured PM target. Each target writes structured entries for the current pass.
3. The `aggregate-validate` state reads every entry file, deduplicates proposals, validates each entry, rejects or defers weak entries, and writes a bounded implementation slice.
4. The `implement` state applies the accepted slice within the declared implementation scope and writes a pass report.
5. If the loop budget remains, the task returns to `product-run` so the next PM pass can inspect the updated product state. Otherwise the task completes.

The state machine and loop diagram live in [states.yaml](states.yaml).

## Instantiate

```bash
rhei instantiate product-management \
  --set product_name="Rhei" \
  --set-file product_brief=./product-brief.md \
  --output .agent-grounds/scratchpad/product-management
```

For structured inputs such as custom targets, use a values file:

```bash
rhei instantiate product-management \
  --values product-management-values.yaml \
  --output .agent-grounds/scratchpad/product-management
```

See the pre-rendered example at `examples/product-management-example/`.

## Codex sandbox

This template's `settings.json` declares its own `codex` profile. Every mode in
it runs unattended (`approval_policy="never"`) with network on; they differ in
what they may write:

| Mode | Writes |
|---|---|
| `workspace` | only under the roots Rhei grants at launch |
| `medium`, `high`, `xhigh` | the same as `workspace`, at that reasoning effort |
| `yolo` | anywhere: `--sandbox danger-full-access` |

The roots are the plan and the Panta project around it, the checkout the state
runs in with its git directories, and Rhei's own state directory
(§FS-rhei-agents.2.2). No default selects `yolo`: a managed ChatGPT workspace
refuses `danger-full-access`, and the worker then falls back to read-only, exits
`0` and writes nothing.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. Grant a directory by
appending it to the mode in the plan's `.agent-grounds/rhei/settings.json`, or
in the Panta project's copy if the settings were hoisted:

```json
"xhigh": ["--sandbox", "workspace-write", "--skip-git-repo-check",
          "-c", "approval_policy=\"never\"",
          "-c", "sandbox_workspace_write.network_access=true",
          "-c", "model_reasoning_effort=\"xhigh\"",
          "--add-dir", "/home/me/.cargo"]
```

The path must be absolute: Rhei passes mode tokens through untouched, so `~` and
`$HOME` are not expanded. The way back is to select `codex[yolo]`. It works only
on an account without managed requirements: where they refuse
`danger-full-access`, Codex rust-v0.159.3 stops at startup if they still allow
`approval_policy="never"` and otherwise falls back on both settings and runs
read-only, as earlier releases do; Rhei names each fallback. Where `never` is
refused, every Codex invocation also reports an `approval_policy` fallback,
harmless under `on-request`, since writes inside the granted roots need no
approval there.
