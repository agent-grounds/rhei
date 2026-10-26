# Multi-Model Analysis Template

This template instantiates a single-task Rhei workspace that:

1. creates one task with a single `analyze` state that fans out across three
   bundled targets by default
2. stores one note per target under `runtime/analyses/`
3. advances the same task to a `summarize` state after the fanout completes
4. writes the final document to `runtime/final-analysis.md` by default

The instantiated workspace also ships a default `.agent-grounds/rhei/settings.json` with
`defaults.agent_timeout` so `rhei run` can orchestrate the analysis agents
without extra manual setup.

Instantiate it from the repository root:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --output ./multi-model-analysis-demo
```

To override the fanout target set, use a values file so the structured
`agents` input stays readable:

```bash
cargo run -p rhei-cli -- instantiate multi-model-analysis \
  --values ./multi-targets.yaml \
  --set plan_title="Multi-Model Analysis" \
  --set task_title="Analyze the target problem" \
  --set-file task_description=./brief.md \
  --output ./multi-model-analysis-demo
```

Example `multi-targets.yaml`:

```yaml
agents:
  - id: claude
    label: Claude analysis note
    selector: claude-code[yolo]:anthropic:claude-opus-4-7
  - id: gemini
    label: Gemini flash analysis note
    selector: gemini[yolo]:google:gemini-3.1-flash
  - id: codex
    label: Codex analysis note
    selector: codex[workspace]:openai:gpt-5-codex

summary_agent: claude-code[yolo]:anthropic:claude-opus-4-7
```

## Customizing the Gemini CLI

Each state references a coding-agent id (`claude-code`, `gemini`, `codex`) that
Rhei resolves against the merged `agents` registry. The built-in profiles are
usually enough, but if your local Gemini CLI uses a different binary name or
different prompt/model flags, override the `gemini` entry in
`<plan>/.agent-grounds/rhei/settings.json`:

```json
{
  "agents": {
    "gemini": {
      "command": ["your-gemini-cli"],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "stdin_prompt": false,
      "modes": {
        "yolo": ["--approval-mode", "auto_edit"]
      }
    }
  }
}
```

A user-written entry replaces the built-in `gemini` profile wholesale — keep
the fields you need. See [Agents
Specification](../../../../docs/functional-spec/rhei-agents.spec.md) for the
full schema and mode-resolution order.

## Inputs

| Input | Type | Default | What it does |
|---|---|---|---|
| `plan_title` | string | *(required)* | Title of the instantiated plan (index heading). |
| `task_title` | string | *(required)* | Title of the shared analysis task. |
| `task_description` | string | *(required)* | Instructions given to every analysis pass. Use `--set-file` for long briefs. |
| `analysis_output_dir` | string | `runtime/analyses` | Where per-target notes are written. |
| `agents` | object[] | 3 targets (claude/gemini/codex) | The fan-out set. Each entry is `{ id, label, selector }`; one analysis pass runs per entry. `id` is slugified into artifact paths. |
| `summary_agent` | string | `claude-code[yolo]:…opus-4-7` | Agent that writes the final synthesis. |
| `final_document_path` | string | `runtime/final-analysis.md` | Where the synthesis lands. |

Per-task path: a single `analysis` task runs `analyze` (fan-out across
`agents`, one pass per target) → `summarize` (one synthesis pass). Run it with
`rhei run --parallel <n>` to execute the per-target passes concurrently.

## Codex sandbox

The default `agents` run Codex as `codex[workspace]`: the built-in profile's
mode that runs unattended (`approval_policy="never"`) with network on and writes
only under the roots Rhei grants at launch, which are the plan and the Panta
project around it, the checkout the state runs in with its git directories, and
Rhei's own state directory (§FS-rhei-agents.2.2). The built-in's other mode,
`yolo`, writes anywhere (`--sandbox danger-full-access`), but a managed ChatGPT
workspace refuses it, and the worker then falls back to read-only, exits `0` and
writes nothing.

**Migrating.** A plan laid with an earlier Rhei keeps the settings it was laid
with, and with them `danger-full-access`. In a plan laid now, a Codex state
cannot write outside those roots: not a build cache such as `~/.cargo`,
`~/.gradle` or `~/.npm`, and not another repository. This template ships no
Codex profile, so grant a directory with one: an `agents` entry beside
`defaults` in the plan's `.agent-grounds/rhei/settings.json`, or in the Panta
project's copy if the settings were hoisted, that keeps the built-in's family:

```json
"agents": {
  "codex": {
    "family": "codex",
    "modes": {
      "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check",
               "-c", "approval_policy=\"never\""],
      "workspace": ["--sandbox", "workspace-write", "--skip-git-repo-check",
                    "-c", "approval_policy=\"never\"",
                    "-c", "sandbox_workspace_write.network_access=true",
                    "--add-dir", "/home/me/.cargo"]
    }
  }
}
```

`modes` is replaced whole, so `yolo` is restated to keep it; `writable_roots` is
still inherited from the built-in. The path must be absolute: Rhei passes mode
tokens through untouched, so `~` and `$HOME` are not expanded. If settings of
your own replace `codex` by id, give that entry the same `family` and
`workspace` mode: without the mode a plan laid from this template is refused at
`rhei validate`, and without the family it is granted no roots unless it writes
`writable_roots` itself. The way back is to select `codex[yolo]`. It works only
on an account without managed requirements: under them, Codex rust-v0.159.3 and
later stop at startup when given `danger-full-access` with
`approval_policy="never"`, and earlier releases fall back to read-only.

## Note

This template mirrors the `target` / `all_targets` spec shape: a single task,
one fan-out analysis state, and one synthesis state. The manifest uses
MiniJinja-compatible structured inputs, so `agents` is a real array of objects
rather than a bundle of hardcoded scalar parameters.

It uses the directory-workspace layout rather than a single-file plan so
`rhei run --parallel <n>` can execute the fanout without being forced back to
sequential mode.

## Example

A pre-rendered example lives at
[`examples/multi-model-analysis-example/`](../../../../examples/multi-model-analysis-example/)
and passes `rhei validate` as shipped.
