# spec-implementation

Take one or more specifications and produce reviewed, fix-cycled,
e2e-covered implementations of them — without you babysitting the loop.

## What it does

You point this template at either a single spec file (`spec_path`) or a
git reference (`spec_ref` — PR, branch, commit range, or `.diff` file)
whose changed `*.spec.md` files become the work set. It instantiates a
workspace with a coordinator task that resolves the input and fans out:

1. **Per-spec pipeline** (one task per spec, run in parallel):
   - **Implementation** by `implementation_target`.
   - **Completeness audit** — fan-out across reviewers. "Did we implement
     everything the spec calls out?"
   - **Quality review/fix loop** — fan-out review, smart aggregation,
     smart fix. Repeats N times. "Is what we implemented actually correct?"
2. **Shared E2E coverage loop** (runs once, after every per-spec task
   completes):
   - Two agents ping-pong: one writes tests against the mock agent, the
     other independently runs and audits them. Repeats M times. "Do tests
     actually exercise the specs through the public interface — without us
     paying for real model calls on every commit?"

Each stage writes its artifacts under `runtime/...` in the workspace so
you can inspect what each agent saw and decided.

## When to use it

- You have a written spec (or several) and you want implementations you
  can trust without running the whole review loop by hand.
- For multi-spec mode, the changed specs are related enough that one
  shared e2e pass over them makes sense. For unrelated specs, instantiate
  separately per spec.
- You're willing to pay for ~`1 + (1 + 2N) × S + 2M` agent invocations
  where S is the spec count, N is `review_passes`, and M is `e2e_passes`.

If your need is closer to *audit*, *review*, or *spec drafting*, use the
neighboring templates (`spec-implementation-discrepancy-audit`,
`changeset-review`, `spec-review`).

## Input mode (XOR)

Exactly one of these must be set:

| Mode | Input | Example |
|---|---|---|
| Single-spec | `spec_path` | `--set spec_path=docs/functional-spec/rhei-list.spec.md` |
| Multi-spec from diff | `spec_ref` | `--set spec_ref=main..HEAD` or `--set spec_ref=PR#42` |

The coordinator state checks the XOR at the start of the run and stops
with a clear error if both or neither are set. (The check happens at run
time, not at instantiation time — instantiation will still produce a
workspace even if the inputs are wrong; the coordinator catches it.)

For `spec_ref`, accepted forms:

- PR URL or number (`PR#42`, full URL, `org/repo#42`) — uses `gh`.
- Branch name — compared against the project's default branch.
- Commit SHA or range `base..head` — uses `git diff`.
- Path to a `.diff` or `.patch` file on disk.

The coordinator filters to paths matching `*.spec.md` under the project's
spec directory.

## Flow at a glance

```
   coordinate                    (smart_target)
     |
     |  appends:
     |    - tasks/NN-impl-<slug>.md   (one per spec)
     |    - tasks/NN-e2e-aggregate.md (one, depends on every impl task)
     v
   completed                     (coordinator task done)

   --- per impl-<slug> task ---
   implement                     (implementation_target)
     |
     v
   completeness-review           (fan-out: review_targets)
     |
     v
   completeness-aggregate        (smart_target)
     |
     v
   completeness-fix              (implementation_target)
     |
     v
   +-> quality-review            (fan-out: review_targets)
   |     |
   |     v
   |   quality-aggregate         (smart_target)
   |     |
   |     v
   |   quality-fix               (smart_target)
   +-- loop x review_passes
         |
         v
       completed                 (per-spec task done)

   --- e2e-aggregate task, after every impl-<slug> completes ---
   +-> e2e-write                 (e2e_writer)
   |     |
   |     v
   |   e2e-verify                (e2e_verifier)
   +-- loop x e2e_passes
         |
         v
       completed                 (workflow done)
```

The state machine diagram (with per-task paths) lives at the top of
[`states.yaml`](./states.yaml).

## Inputs

The only required choice is one of `spec_path` / `spec_ref`. Everything
else has a sensible default.

| Input | Type | Default | What it does |
|---|---|---|---|
| `spec_path` | string | empty | Single-spec mode. Path to one spec file. |
| `spec_ref` | string | empty | Multi-spec mode. PR / branch / commit range / diff file whose changed `*.spec.md` files become the work set. |
| `spec_title` | string | `Spec Implementation` | Workspace title. |
| `implementation_target` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that implements and closes completeness gaps. |
| `review_targets` | string[] | `[claude-code…, codex…]` | Reviewers — used in both completeness and quality reviews per spec. |
| `smart_target` | string | `codex[xhigh]:openai:gpt-5.5` | Coordinator; aggregator; writes fix plan; applies fixes. |
| `review_passes` | number | `2` | Quality review/fix cycles per spec. |
| `focus_areas` | string[] | `[]` | Optional focus sections each quality reviewer must address. |
| `e2e_writer` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that adds e2e tests in the shared loop. |
| `e2e_verifier` | string | `codex[xhigh]:openai:gpt-5.5` | Agent that re-runs and audits the e2e suite. Should differ from `e2e_writer`. |
| `e2e_passes` | number | `2` | E2E write/verify cycles. |
| `e2e_test_root` | string | empty | Where e2e tests live (e.g., `e2e/`). Empty = let the writer discover. |
| `mock_agent` | string | `mock` | The mock agent selector every standard e2e test must target. |
| `release_only_marker` | string | `release-only` | Tag the CI runner uses to exclude real-agent tests from the default suite. |
| `release_only_test_root` | string | empty | Optional separate directory for release-only real-agent tests. |

### E2E test policy (enforced by the template)

Every standard e2e test added by this workflow MUST target the **mock agent**
(`mock_agent`, default `mock`), which returns canned outputs the test
controls. This keeps the suite fast, deterministic, and offline — no real
model calls on every commit.

A small **release-only subset** may invoke real agent operations to verify
that the agent integration still works end-to-end. These tests are marked
with `release_only_marker` (default `release-only`) so the project's CI
runner can:

- exclude them from the default test command, and
- include them only in release builds.

The verifier (`e2e_verifier`) enforces both halves of the policy: it flags
any standard test that calls real-agent code paths, and it flags growth in
the release-only subset beyond ~one happy-path test per distinct real-agent
integration.

Override the defaults per-instantiation when your project uses different
names:

```bash
--set mock_agent='MockAgent::canned' \
--set release_only_marker='@RealAgent' \
--set release_only_test_root='e2e/real-agent/'
```

### Common configurations

- **Single spec, balanced:** `--set spec_path=docs/.../my-feature.spec.md`.
- **All specs in the current branch:** `--set spec_ref=main..HEAD`.
- **A specific PR:** `--set spec_ref=PR#42`.
- **Faster, lower-confidence pass:** add `--set review_passes=1 --set e2e_passes=1`.
- **Heavier audit:** `--set review_passes=3` and add a third reviewer.

## Quick start

```bash
# Single spec
rhei instantiate spec-implementation \
  --set spec_path=docs/functional-spec/my-feature.spec.md \
  --output .agent-grounds/scratchpad/spec-implementation/

# All specs changed on the current branch
rhei instantiate spec-implementation \
  --set spec_ref=main..HEAD \
  --output .agent-grounds/scratchpad/spec-implementation/

rhei run .agent-grounds/scratchpad/spec-implementation/
```

For non-scalar inputs (`review_targets`, `focus_areas`), use a values file:

```bash
rhei instantiate spec-implementation \
  --values my-values.yaml \
  --output .agent-grounds/scratchpad/spec-implementation/
```

## Where things land

Each state writes its artifacts under `runtime/...` in the workspace:

| Directory | What's in it |
|---|---|
| `runtime/manifests/` | Coordinator-written spec assignments and per-task spec paths. |
| `runtime/implement/` | Per-spec implementation notes (what was implemented, what was deferred). |
| `runtime/completeness/` | Per-reviewer gap inventories, merged gap list, fix log — per spec. |
| `runtime/quality/` | Per-reviewer findings, fix plan, and fix log — per spec, per pass. |
| `runtime/e2e/` | Shared write report + verify report — per pass. |

The fix-and-edit work itself happens in the repository checkout (resolved
via `git rev-parse --show-toplevel`), not in the workspace. The
coordinator also appends per-spec task files and the e2e-aggregate task
file under `tasks/` during the run.

## What's bundled

- `template.yaml` — input manifest.
- `states.yaml` — the `spec-implementation` state machine (diagram in the
  top comment).
- `index.rhei.md` — workspace index.
- `tasks/01-coordinate.md` — the initial coordinator task skeleton.
- `settings.json` — declares the `codex` profile that
  [Codex sandbox](#codex-sandbox) describes (the default `smart_target`
  uses its `xhigh`) and sets `agent_timeout: 2h` since implementation
  states can run long.

## Codex sandbox

This template's `settings.json` declares its own `codex` profile. Every mode in
it runs unattended (`approval_policy="never"`) with network on; they differ in
what they may write:

| Mode | Writes |
|---|---|
| `workspace` | only under the roots Rhei grants at launch |
| `high`, `xhigh` | the same as `workspace`, at that reasoning effort |
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

## Example

A pre-rendered example lives at
[`examples/spec-implementation-example/`](../../../../examples/spec-implementation-example/)
and passes `rhei validate` as shipped.
