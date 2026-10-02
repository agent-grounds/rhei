# Rhei vs. Other Agent Task-Planning Systems

This document compares Rhei to other task-planning and agent-coordination systems
in the 2024–2026 landscape. It is informational, not normative — the language
specification is in [`rhei-plan-language.spec.md`](rhei-plan-language.spec.md).

## Feature matrix

| Dimension | **Rhei** | **beads** (Steve Yegge) | **beans** (hmans) | **opencode** todowrite | **Claude Code** TodoWrite | **Cline** Deep Planning | **Cursor** Plan Mode | **Roo** Boomerang | **Devin** | **Augment** Tasklist |
|---|---|---|---|---|---|---|---|---|---|---|
| **Storage** | Markdown (`.rhei.md` or `tasks/*.md` workspace) | Dolt SQL DB in `.beads/` | Markdown in `.beans/` + `.beans.yml` | SQLite (`TodoTable`) per-session | In-memory per-session | Markdown (`implementation_plan.md`) + side state | Internal (chat-rendered) | Internal task store | Backend DB (DAG) | Backend DB (typed objects) |
| **Source of truth** | The markdown file | The Dolt DB | Markdown files | DB row | Conversation | Markdown + Focus Chain | Cursor backend | Roo store | Cognition backend | Augment backend |
| **Human-editable in any text editor** | Yes (primary mode) | No (CLI/MCP only; JSONL export) | Yes | No | No | Yes | Partial (pre-run edit) | No | No | No |
| **Hierarchy** | Yes — heading depth + dotted ids, configurable `maxLevels` 1–4 | Yes — typed `parent-child` edges + dotted ids (3 levels) | Yes — milestone/epic/feature/task via parent ref | None | None | Headings only | Parent + sub-tasks | Subtasks in own context | DAG nodes | `parent_task` field |
| **Explicit dependencies** | Yes — `**Prior:**` field, DAG-validated | Yes — typed edges (`blocks`, `waits-for`, `conditional-blocks`, …) | Yes — blocking/blocked-by | No | No | Prose only | No formal DAG | No | Yes — DAG edges | Implicit via parent |
| **Custom / pluggable state machine** | Yes — YAML state machine, `node_policy`, profiles, artifact contracts, callbacks | No — fixed `open`/`in_progress`/`closed` (+ `blocked`, `deferred`) | Configurable statuses (data, no semantics) | No (4 states, prompt-enforced) | No (3 states) | No | No | No (mode-switching is the FSM) | No (internal) | No (`todo`/`in_progress`/`finished`/`cancelled`) |
| **Counted state visits / review loops** | Yes — `visits: n` + `state-2`, `state-3` suffixes | No | No | No | No | No | No | No | No | No |
| **Ready-work selection** | `rhei next` — terminal-state `Prior:` + node-policy filter | `bd ready` — transitive closed-deps query | Implied (no dedicated command yet) | LLM picks from list | LLM picks | LLM picks | User picks | Orchestrator picks subtask | Scheduler over DAG | LLM picks |
| **Concurrency model** | Directory Workspace: per-task files, global graph merge; atomic CAS via `rhei transition` | Hash-IDs, Dolt cell-merge, atomic `--claim` | Worktree-based parallel agents | None — per-session | None | None | None | Sequential (parent suspends) | Internal scheduling | Coordinator + Specialists |
| **Multi-agent coordination substrate** | Filesystem + git | Dolt remotes + atomic claim | Git worktrees + sessions | n/a | n/a | n/a | n/a | Single active path | Hidden | Backend Coordinator |
| **Persistence across sessions** | Yes (file in repo) | Yes (DB in repo) | Yes (files in repo) | Yes (SQLite) | No | Yes (markdown) | Yes (queue) | Yes (store) | Yes | Yes |
| **External-tracker bridge** | None built-in | `external_ref` to GH/Jira/ADO | None | None | None | None | None | None | None | None |
| **Agent interface** | CLI (`rhei next`/`transition`/`complete`/`run`), markdown reads | CLI + MCP, `--json` everywhere | CLI + GraphQL/WebSocket + TUI + plugins | Single `todowrite` tool | Single `TodoWrite` tool | Slash command + chat | Shift+Tab mode | Mode switch | GUI + API | Tool API |
| **Validation (cycles, ids, links, schema)** | Yes — full validator (`rhei validate`) | DAG enforced; less surface | YAML/markdown schema | Schema only | None | None | None | None | n/a | Schema |
| **Artifact contracts (required input/output files per state)** | Yes | No | No | No | No | No | No | No | No | No |
| **Parameterized templates (capture & instantiate recurring workflows)** | Yes — `rhei instantiate` with typed inputs, MiniJinja rendering, bundled state machine + settings | No (issue presets only) | No | No | No | No | No | No | No | No |

## Where Rhei sits

The surveyed systems fall into three rough archetypes:

1. **DB-backed agent trackers** (beads, Devin, Augment) — rich queries, atomic
   claims, hidden internals. Not human-editable as text.
2. **LLM scratchpads** (Claude Code TodoWrite, opencode todowrite, Cursor / Cline
   checklists) — flat, ephemeral, no dependencies, no concurrency. The model owns
   the list.
3. **Markdown-as-plan conventions** (Cline `implementation_plan.md`, AGENTS.md,
   ad-hoc `plan.md`) — readable, but no shared schema, no validator, no
   scheduler.

**Rhei is the only surveyed system that combines all of:** markdown-as-source-of-truth,
explicit `**Prior:**` DAG, configurable hierarchy, swappable YAML state machine
with counted visits and artifact contracts, *and* a directory-workspace
concurrency model for multi-agent coordination over git.

The closest neighbor on the markdown axis is Cline's deep-planning artifact, but
it has no formal state machine, no validator, and no concurrency story. The
closest neighbor on the DAG/state axis is beads, but it abandons human-editable
markdown to get there.

## Notable Rhei-only features

- **Counted-visit suffixes** (`review-2`, `` `human review-3` ``) for bounded
  review loops in the markdown itself.
- **Node-policy resolution** (`overrides → by_type → default`) so different node
  kinds get different allowed states.
- **State artifact contracts** as part of the state machine — required input
  files on entry, required outputs on exit, enforced at runtime.
- **Hierarchical id integrity** (heading depth must equal id segment count) as a
  normative validator rule.
- **Reserved root kind `rhei`** with a dedicated `node_policy.root` profile.

## Closest functional pairings

- Rhei single-file ≈ Cline `implementation_plan.md` + a state machine + a
  validator.
- Rhei Directory Workspace ≈ beads' multi-agent story but in markdown instead of
  Dolt.
- `rhei next` / `rhei transition` ≈ `bd ready` / `bd update --claim` + Roo's
  mode-bound execution.

## Sources

- [steveyegge/beads](https://github.com/steveyegge/beads) — Steve Yegge's
  Dolt-backed agent issue tracker
- [hmans/beans](https://github.com/hmans/beans) — markdown-native alternative to
  beads
- [opencode `todo.ts`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/tool/todo.ts),
  [opencode Modes](https://open-code.ai/en/docs/modes)
- [Claude Code Todo Tracking](https://code.claude.com/docs/en/agent-sdk/todo-tracking)
- [Cline Deep Planning](https://docs.cline.bot/features/slash-commands/deep-planning)
- [Cursor Plan Mode](https://cursor.com/docs/agent/plan-mode)
- [Roo Boomerang Tasks](https://docs.roocode.com/features/boomerang-tasks)
- [Devin 2.0](https://cognition.ai/blog/devin-2)
- [Augment Tasklist](https://www.augmentcode.com/blog/how-we-built-tasklist),
  [Augment Intent](https://www.augmentcode.com/blog/intent-a-workspace-for-agent-orchestration)
- [AGENTS.md](https://agents.md/)
