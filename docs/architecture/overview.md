# Rhei Overview

Rhei is a structured markdown plan system for hierarchical task management. It enables parsing, validation, execution, and rendering of Rhei plans with support for formal state transitions.

## Purpose

Rhei serves three primary use cases:

1. **GitHub/Ticket Integration** — Reflect a hierarchy of tickets with dependencies and states
2. **AI Agent State Management** — Enable coding agents to maintain state and track progress with minimal context
3. **Human Oversight** — Allow humans to oversee, review, and manage automated work

## Quick Start

### CLI Installation

Build and run the CLI from source:

```bash
cargo build -p rhei-cli --release
```

### Basic Usage

Validate a plan file:

```bash
rhei validate path/to/plan.rhei.md
```

Watch for changes:

```bash
rhei validate --watch path/to/plan.rhei.md
```

Render a plan as JSON for other tools:

```bash
rhei render path/to/plan.rhei.md --format json --pretty
```

See the [README](../../README.md) for complete CLI options.

---

## Core Concepts

### Plans

A **Rhei plan** is a structured markdown document representing a project or workflow. Each plan contains:

- A title (`# Rhei: <title>`)
- Optional content sections (overview, requirements, etc.)
- A `## Tasks` section with hierarchical task definitions

### Tasks and Child Tasks

**Tasks** are the primary work units within a plan. Each task has:

- A unique identifier (numeric or named)
- A mandatory state
- Optional dependencies on other tasks
- Optional child task nodes for detailed breakdown

**Child task nodes** are full task nodes declared under a parent heading
(`####` under `###`, `#####` under `####`, and so on). A child's id extends
its parent's id by one segment joined with a dot — for example the children
of `Task 2` are `Task 2.1`, `Task 2.2`, and a child of `Task api.cache` is
`Task api.cache.fix`. Tree depth is bounded by the plan's
`structure.maxLevels` (default `2`).

### States and Transitions

Tasks progress through defined **states** (for example `draft` → `pending` → `agent-review` → `completed`). The state machine can be:

- Simple: A flat list of valid states for validation
- Formal: Full transition rules with callbacks for automation

### What an Agent Is Given

When `rhei run` starts an agent on a task, its agent, model, MCP servers and
skills come from the machine and project settings, the state, the task and the
run's flags. For one value the most specific place wins. For a list such as
MCP servers, the entries add up. [What an Agent Is Given](../what-an-agent-is-given.md)
gives the order for each setting and says what happens when a server cannot
be reached.

---

## Specification Documents

### Language and Format

| Document | Description |
|----------|-------------|
| [Rhei Language Reference](../functional-spec/rhei-language-reference.spec.md) | Canonical entry point for the complete user-authored Rhei language surface |
| [Plan Language Specification](../functional-spec/rhei-plan-language.spec.md) | Formal EBNF grammar, token types, and semantic constraints for Rhei plan documents |
| [States Specification](../functional-spec/rhei-states.spec.md) | State machine format and default states |
| [Transitions Specification](../functional-spec/rhei-transitions.spec.md) | State transition system, callbacks, and YAML schema |
| [Run Specification](../functional-spec/rhei-run.spec.md) | Orchestrated execution loop |
| [Agent-Orchestrator Workflow Architecture](agent-orchestrator-workflow.spec.md) | Component workflow for plan creation, validation, and execution |
| [Canonical Language Reference Architecture](language-reference.spec.md) | Architectural rule that the language reference must exist as the entry point |
| [Source File Size Architecture](source-file-size.spec.md) | Source file size limits and exception register |

### Reference Files

| File | Description |
|------|-------------|
| [states.yaml](../functional-spec/states.yaml) | Default states definition used for validation |
| [release-automation.rhei.md](../../examples/release-automation.rhei.md) | A checked-in example plan |

### Examples

The [`examples/`](../../examples/) directory contains working plan documents:

| Example | Features Demonstrated |
|---------|----------------------|
| [`release-automation.rhei.md`](../../examples/release-automation.rhei.md) | Mixed task IDs, dependencies, code blocks |
| [`human-review-loop.rhei.md`](../../examples/human-review-loop.rhei.md) | Review states, dependency chains |
| [`escaped-state-values.rhei.md`](../../examples/escaped-state-values.rhei.md) | States with spaces, custom states files |

See [`examples/README.md`](../../examples/README.md) for verification commands.

---

## Architecture

Rhei is structured as a Rust workspace with focused crates:

```
rhei/
├── crates/
│   ├── rhei-core/       # Lexer, parser, AST types
│   ├── rhei-validator/  # Semantic validation
│   ├── rhei-output/     # JSON, markdown, progress rendering
│   ├── rhei-cli/        # Command-line interface
│   └── rhei-napi/       # Node.js bindings
└── docs/
```

### Crate Responsibilities

Only crates with an audience outside this repository are workspace crates; the
CLI's own subsystems are modules inside `rhei-cli`. [§FS-rhei-distribution.1](../functional-spec/rhei-distribution.spec.md#1-release-targets)

| Source crate | Published package | Role |
|-------------|-------------------|------|
| `rhei-core` | `rhei-plan` | Tokenizes markdown, parses into AST, defines data structures |
| `rhei-cli` | `rhei-cli` | Provides the `rhei` command and its `rh` alias |
| `rhei-agent-core` | unpublished | Re-export facade over `rhei-plan`; a release target once it has an API of its own |
| `rhei-napi` | unpublished (`rhei-api` on npm/PyPI) | Native implementation backing JavaScript API packages via N-API |

`rhei-cli` modules that were previously separate crates, and keep their names
so the CLI body still spells them the same way:

| Module | Role |
|--------|------|
| `rhei_validator` | Validates dependencies, states, cycles, and child task id numbering |
| `rhei_output` | Renders AST to JSON, GitHub markdown, terminal progress |
| `rhei_tui` | Terminal UI event surface and frontend |
| `rhei_viz_model` | Flow-visualization data model and self-contained HTML renderer |
| `rhei_viz` | Builds the visualization model from a plan and its state machine |

The Rust import name for the plan model remains `rhei_core`. Public language
bindings use the package name `rhei-api` on npm and PyPI; the N-API
implementation crate is not published as a standalone crates.io package.

### Processing Pipeline

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Markdown   │────▶│   Lexer     │────▶│   Parser    │────▶│    AST      │
│   Input     │     │  (tokens)   │     │  (rhei-core)│     │             │
└─────────────┘     └─────────────┘     └─────────────┘     └──────┬──────┘
                                                                   │
                    ┌─────────────┐     ┌─────────────┐            │
                    │   Output    │◀────│  Validator  │◀───────────┘
                    │ (render)    │     │ (semantic)  │
                    └─────────────┘     └─────────────┘
```

---

## Library Usage

For programmatic integration in Rust:

```rust
use rhei_core::parse;
use rhei_validator::{StateMachine, validate_with_machine};
use rhei_output::{render_json, render_github, render_progress};

// 1. Parse markdown into AST
let saga = parse(markdown_content)?;

// 2. Load state machine
let machine = StateMachine::from_yaml_file("states.yaml")?;

// 3. Validate
let errors = validate_with_machine(&saga, &machine);

// 4. Render
let json = render_json(&saga, true)?;  // pretty print
let github = render_github(&saga, options)?;
let progress = render_progress(&saga, options)?;
```

---

## Formal State Transitions

For workflows requiring automation, Rhei supports formal state transitions with callbacks. This enables:

- Declarative transition rules in YAML
- Pre/post transition callbacks (`on_leave`, `on_enter`)
- Conditional transitions and timeouts
- Multi-platform execution (CLI/bash, Node.js, Python, Java)

See the [Transitions Specification](../functional-spec/rhei-transitions.spec.md) for:

- [`TransitionContext`](../functional-spec/rhei-transitions.spec.md#1-transitioncontext-data-structure) — Data passed to callbacks
- [YAML State Machine Format](../functional-spec/rhei-transitions.spec.md#4-yaml-state-machine-format-specification) — Configuration schema
- [Transition Callback Examples](../functional-spec/rhei-callbacks.spec.md) — CLI, JavaScript, Python, and Java examples

---

## Document Conventions

### File Extensions

- `.rhei.md` — Rhei plan documents
- `.yaml` — States and state machine definitions

### State Values

Single-word states are written directly:

```markdown
**State:** pending
```

Multi-word states require backtick escaping:

```markdown
**State:** `in progress`
```

### Task References

Dependencies use the `Task <id>` format:

```markdown
**Prior:** Task 1, Task 2
```

---

## Related Resources

- [README](../../README.md) — Project overview and CLI reference
- [AGENTS.md](../../AGENTS.md) — CI verification commands for contributors
- [Cargo.toml](../../Cargo.toml) — Workspace configuration
