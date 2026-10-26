# Agora Template

This template turns a discussion, transcript, issue, or decision brief into a
structured multi-agent deliberation. One splitter agent breaks the input into
separate points. Configured target agents independently propose a solution for
each point, an aggregator tags agreements and disagreements, the targets discuss
those disagreements, and an aggregator chooses a point decision. A final
aggregator then proposes the overall solution, and a presentation agent writes
the simplest human-facing summary with the reasons for the choice.

The custom workflow is defined in [`states.yaml`](states.yaml), including the
state-machine diagram.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `plan_title` | string | `Agora` | Title of the instantiated workspace. |
| `discussion_title` | string | required | Short title for the discussion being resolved. |
| `discussion` | string | required | Full discussion/transcript/brief. Use `--set-file` for long input. |
| `splitter_agent` | string | `codex[workspace]:openai:gpt-5.5` | Agent that turns the input into point tasks. |
| `target_agents` | object[] | claude/codex/gemini | Agents that propose and discuss solutions for every point. Each entry is `{ id, label, selector }`; the array must not be empty, and both `id` and `selector` values must be unique. |
| `point_aggregator_agent` | string | `codex[workspace]:openai:gpt-5.5` | Agent that tags disagreements and resolves each point. |
| `final_aggregator_agent` | string | `codex[workspace]:openai:gpt-5.5` | Agent that synthesizes the final cross-point solution. |
| `presentation_agent` | string | `claude-code[yolo]:anthropic:claude-opus-4-7` | Agent that writes the simplest human-facing summary. |
| `max_points` | positive number | `8` | Soft cap for how many point tasks the splitter should create. |
| `output_dir` | string | `runtime/agora` | Workspace-relative artifact directory. |

## Task Paths

| Task kind | State path |
|---|---|
| Splitter task | `split-points -> completed` |
| Generated point task | `propose-solutions -> aggregate-disagreements -> discuss-point -> resolve-point -> completed` |
| Final aggregation task | `final-solution -> present-to-human -> human-review -> completed` |

## Flow

1. `Task split` reads the discussion, writes a point manifest, creates one
   `point-<slug>` task per separate point, and creates `Task final-solution`
   with priors on every point task.
2. Each point task fans out through `propose-solutions`, running every configured
   `target_agents` entry and writing one proposal per target.
3. `aggregate-disagreements` reads the proposals, groups candidate solutions,
   names agreements, tags disagreements, and writes a discussion prompt.
4. `discuss-point` fans out to the same target agents so they respond to the
   disagreement map rather than restating their first proposal.
5. `resolve-point` chooses the best solution for that point and records why.
6. `final-solution` reads all point decisions and writes the overall
   recommendation.
7. `present-to-human` writes `runtime/agora/human-summary.md`, then the
   final task enters the `human-review` gate. A human accepts by transitioning to
   `completed` or abandons by transitioning to `cancelled`.

## Instantiate

```bash
cargo run -p rhei-cli -- instantiate agora \
  --set discussion_title="Resolve API design discussion" \
  --set-file discussion=./discussion.txt \
  --output ./agora-demo
```

For custom target agents, use a values file:

```yaml
target_agents:
  - id: claude
    label: Claude proposal
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: codex
    label: Codex proposal
    selector: codex[workspace]:openai:gpt-5.5
  - id: gemini
    label: Gemini proposal
    selector: gemini[yolo]:google:gemini-3.1-pro-preview
```

Then instantiate with:

```bash
cargo run -p rhei-cli -- instantiate agora \
  --values ./agora-values.yaml \
  --set-file discussion=./discussion.txt \
  --output ./agora-demo
```

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
[`examples/agora-example/`](../../../../examples/agora-example/)
and passes `rhei validate` as shipped.
