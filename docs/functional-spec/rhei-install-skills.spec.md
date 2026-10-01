# FS-rhei-install-skills: `rhei install-skills`

Install rhei skills (plan-writer, plan-worker, state-machine-writer, template-writer) into the configuration directories of major AI coding agents, so any agent session can invoke them without per-project setup. Supports both global (user-level) and project-local installation.

Every skill the binary carries is installed by default. A skill that ships in a release but not in the default set would reach only the users who read the flag documentation and typed its name, which is not a state any shipped skill should be in.

## 1. Usage

```
rhei install-skills [OPTIONS]
rhei install-skills --agent claude-code
rhei install-skills --agent cursor
rhei install-skills --agent all
rhei install-skills --local --agent claude-code
rhei install-skills --uninstall --agent claude-code
```

## 2. Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent <NAME>` | `all` | Target agent: `claude-code`, `cursor`, `windsurf`, `copilot`, `kilocode`, `pi`, `codex`, `antigravity`, or `all` |
| `--local` | | Install into the current project directory instead of global user config |
| `--link` | | Symlink skill files instead of copying (stays up-to-date with rhei releases) |
| `--uninstall` | | Remove previously installed skills |
| `--dry-run` | | Print what would be done without changing anything |
| `--skills <LIST>` | every embedded skill: `rhei-plan-writer,rhei-plan-worker,rhei-state-machine-writer,rhei-template-writer` | Comma-separated list of skills to install |

## 3. Agent Targets

Each agent has a different configuration layout. The command handles each one. The tables below show global (default) and project-local (`--local`) paths.

### 3.1. Claude Code (`claude-code`)

| Mode | Skill files | Registration |
|------|-------------|--------------|
| Global | `~/.claude/skills/rhei-<skill>/` | `~/.claude/CLAUDE.md` |
| Local | `.claude/skills/rhei-<skill>/` | `.claude/CLAUDE.md` (project root) |

**Registration:** Append a section to the target `CLAUDE.md`:

```markdown
# rhei
- **rhei-plan-writer** (`~/.claude/skills/rhei-plan-writer/SKILL.md`) — create and validate Rhei Plans. Trigger: `/rhei-plan-writer`
- **rhei-plan-worker** (`~/.claude/skills/rhei-plan-worker/SKILL.md`) — execute tasks in a Rhei Plan. Trigger: `/rhei-plan-worker <plan>`
- **rhei-state-machine-writer** (`~/.claude/skills/rhei-state-machine-writer/SKILL.md`) — design custom state machines from project specs and teams. Trigger: `/rhei-state-machine-writer`
- **rhei-template-writer** (`~/.claude/skills/rhei-template-writer/SKILL.md`) — create and edit reusable Rhei Templates. Trigger: `/rhei-template-writer`
When the user types `/rhei-plan-writer`, `/rhei-plan-worker`, `/rhei-state-machine-writer`, or `/rhei-template-writer`, invoke the Skill tool with the corresponding skill name before doing anything else.
```

In local mode, the paths in the registration block use relative paths (e.g., `.claude/skills/rhei-plan-writer/SKILL.md`).

### 3.2. Cursor (`cursor`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.cursor/rules/rhei-<skill>.mdc` |
| Local | `.cursor/rules/rhei-<skill>.mdc` (project root) |

**Format:** Cursor uses `.mdc` files with YAML frontmatter:

```markdown
---
description: Create and validate Rhei Plan markdown documents
globs:
  - "**/*.rhei.md"
alwaysApply: false
---

<SKILL.md content>
```

### 3.3. Windsurf (`windsurf`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.windsurfrules` (or `~/.codeium/windsurf/memories/global_rules.md`) |
| Local | `.windsurfrules` (project root) |

**Format:** Plain markdown sections, delimited with `<!-- rhei:start -->` / `<!-- rhei:end -->` markers for clean uninstall.

### 3.4. GitHub Copilot (`copilot`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.github/copilot-instructions.md` |
| Local | `.github/copilot-instructions.md` (project root) |

**Format:** Plain markdown appended between `<!-- rhei:start -->` / `<!-- rhei:end -->` markers.

**Note:** Copilot's instruction file has no skill/trigger system — the content is injected as system context. Skills are presented as "when the user asks to create/execute a Rhei plan, follow these instructions."

### 3.5. Kilocode (`kilocode`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.kilocode/rules/rhei-<skill>.md` |
| Local | `.kilocode/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown with Kilocode's frontmatter if supported, otherwise raw content.

### 3.6. Pi (`pi`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.pi/rules/rhei-<skill>.md` |
| Local | `.pi/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown rule files. Pi loads all `.md` files from its rules directory as system context.

### 3.7. OpenAI Codex (`codex`)

| Mode | Skill files | Registration |
|------|-------------|--------------|
| Global | `~/.agents/skills/rhei-<skill>/SKILL.md` | None |
| Local | `.agents/skills/rhei-<skill>/SKILL.md` (project root) | None |

**Format:** A standard Codex skill directory containing `SKILL.md` and any optional supporting files (`scripts/`, `references/`, `assets/`, `agents/`).

**Note:** Codex discovers skills by scanning `.agents/skills` from the current working directory up to the repository root, plus `$HOME/.agents/skills` for user-level skills. No registration or marker injection file is needed. Custom spawned agents are configured separately under `.codex/agents/*.toml` or `~/.codex/agents/*.toml`; they inherit the parent session's available skills unless `skills.config` is explicitly overridden.

### 3.8. Google Antigravity (`antigravity`)

| Mode | Skill files |
|------|-------------|
| Global | `~/.antigravity/rules/rhei-<skill>.md` |
| Local | `.antigravity/rules/rhei-<skill>.md` (project root) |

**Format:** Plain markdown rule files.

## 4. Behavior

### 4.1. Local installation

With `--local`, skills are installed into the current project directory instead of the user's home directory. The command resolves the project root by walking up from the current directory to find a `.git` directory, `Cargo.toml`, `package.json`, or similar project marker. If no project root is found, it falls back to the current working directory.

Local installation is useful for:

- Sharing skills with collaborators via version control (the default copies files).
- Scoping skills to a specific project without polluting the global config.
- Overriding global skills with project-specific versions.

When `--local` is combined with `--link`, the symlinks use relative paths so the project stays portable. Files installed with `--local` and `--link` should be added to `.gitignore` unless the intent is to commit them.

### 4.2. Detect installed skills

Before writing, remove or replace any existing rhei skill files for the target agent and install the requested set again. Re-running `install-skills` refreshes previously installed skills in place instead of skipping.

### 4.3. Resolve skill source

The skills are compiled into the binary, so `install-skills` works from any
`rhei` — a `cargo install`, an extracted release archive, the npm wrapper —
with no companion asset directory to locate. Resolution never depends on where
the binary sits or on the current working directory.

A filesystem copy takes precedence when one is present, so a checkout can
install the skills it is editing rather than the ones the binary was built
from. In order:

1. `<binary>/../share/rhei/skills/<skill>/` — a packaged asset directory, for
   distro packages that install skills alongside the binary.
2. `<repo>/crates/rhei-cli/skills/<skill>/` — found by walking up from the
   binary, then from the current directory, to a directory holding
   `crates/rhei-cli/skills/`. This is the dev-build path.
3. The embedded copy, materialized into a temporary directory for the duration
   of the command.

An unknown skill name is an error that lists the skills the binary carries.

### 4.4. Symlink vs copy

The default behavior copies skill files into the target directory. `--link` symlinks instead — useful during development so skills stay up-to-date with local changes, but requires the rhei source to remain at a stable path.

Because a symlink into a temporary extraction would dangle the moment the
command exits, `--link` needs one of the filesystem sources in §4.3. When only
the embedded copy is available, `--link` fails with an error naming the two
paths that would have satisfied it and pointing at plain copying instead.

`--link` is the one option in this command with a platform in it. An
unprivileged Windows process cannot create a symbolic link, so there `--link`
refuses the agent it was asked for, names the platform limit, and points at
plain copying; the other agents in the same invocation are unaffected, and the
copying default behaves identically everywhere. [§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity)

### 4.5. Registration

For agents that require explicit registration (Claude Code's `CLAUDE.md`), the command appends a delimited section. It uses markers (`<!-- rhei:start -->` / `<!-- rhei:end -->` or an `# rhei` heading) so uninstall and updates can find and replace the block idempotently.

The generated `# rhei` block is contiguous — the heading, one bullet per skill, then the trigger sentence — so it ends at the **first blank line**, or at the next heading of equal or higher level, whichever comes first. Everything after that boundary is left untouched on both update and uninstall.

Ending the block at the next heading instead would make rhei delete whatever sits between its own block and that heading. These files are shared: a user's `CLAUDE.md` holds other tools' marker blocks and their own prose, and none of it is rhei's to remove.

### 4.6. Dry run

With `--dry-run`, print each action (symlink, copy, append) without executing.

### 4.7. Uninstall

With `--uninstall`, remove symlinks/copied files and delete the registered section from agent config files.

### 4.8. Bundled reference files

A skill directory's `references/` subtree is part of the skill: every file under
it, at any depth, is installed with the `SKILL.md` for every target in §3, and
is resolved from the same source as the skill itself (§4.3).

One of those files is shared. `references/shape.md` ships under
`rhei-plan-writer`, `rhei-state-machine-writer` and `rhei-template-writer`, and
it is an extract of [§FS-rhei-shape](rhei-shape.spec.md#fs-rhei-shape-state-task-subtask-rhei-or-prose) rather than
prose of its own: the three installed copies are byte-identical to each other,
and their extracted block is byte-identical to that page's ([§FS-rhei-shape.7](rhei-shape.spec.md#7-the-one-normative-copy)).
Three real files ship rather than one file and two links, because the skills are
embedded in the binary at build time and a symlink does not survive that — nor
every platform [§REQ-cross-platform.2](../requirements/cross-platform.md#2-parity).

## Example Output

### Global (default)

```
$ rhei install-skills --agent all

claude-code:
  ✓ ~/.claude/skills/rhei-plan-writer → /usr/share/rhei/skills/rhei-plan-writer
  ✓ ~/.claude/skills/rhei-plan-worker → /usr/share/rhei/skills/rhei-plan-worker
  ✓ ~/.claude/CLAUDE.md — registered 2 skills

cursor:
  ✓ ~/.cursor/rules/rhei-plan-writer.mdc — written
  ✓ ~/.cursor/rules/rhei-plan-worker.mdc — written

windsurf:
  ✓ ~/.windsurfrules — appended rhei section

copilot:
  ✓ ~/.github/copilot-instructions.md — appended rhei section

kilocode:
  ✓ ~/.kilocode/rules/rhei-plan-writer.md — written
  ✓ ~/.kilocode/rules/rhei-plan-worker.md — written

pi:
  ✓ ~/.pi/rules/rhei-plan-writer.md — written
  ✓ ~/.pi/rules/rhei-plan-worker.md — written

codex:
  ✓ ~/.agents/skills/rhei-plan-writer — copied
  ✓ ~/.agents/skills/rhei-plan-worker — copied

antigravity:
  ✓ ~/.antigravity/rules/rhei-plan-writer.md — written
  ✓ ~/.antigravity/rules/rhei-plan-worker.md — written

Installed rhei skills for 8 agents.
```

### Project-local

```text
$ rhei install-skills --local --agent claude-code

claude-code (local):
  ✓ .claude/skills/rhei-plan-writer → ../../target/rhei/skills/rhei-plan-writer
  ✓ .claude/skills/rhei-plan-worker → ../../target/rhei/skills/rhei-plan-worker
  ✓ .claude/skills/rhei-state-machine-writer → ../../target/rhei/skills/rhei-state-machine-writer
  ✓ .claude/CLAUDE.md — registered 3 skills

Installed rhei skills locally for 1 agent.
```

## Implementation Notes

- New `InstallSkills` variant in the `Commands` enum in `crates/rhei-cli/src/main.rs`.
- Agent-specific logic should be a match arm per agent, keeping format conversion isolated.
- The `.mdc` conversion for Cursor and marker-delimited injection for Windsurf/Copilot are the only non-trivial transforms — all others are copy/symlink plus optional registration.

## Related Documentation

- [Plan Language Specification](rhei-plan-language.spec.md) - Formal grammar and semantic constraints
- [How Rhei Is Used](rhei-usage.spec.md) - Roles, coordination patterns, and agent workflows
