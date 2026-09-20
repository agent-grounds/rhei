# FS-rhei-agents: Rhei Agents Specification

This document specifies how Rhei integrates with coding agents — the CLI tools that execute work on tasks. It covers agent configuration, resolution order, invocation profiles, prompt composition, parallel execution, timeout handling, and log capture.

For the state machine format see [States Specification](rhei-states.spec.md). For transition callbacks see [Transitions Specification](rhei-transitions.spec.md).
For token and cost accounting see [Cost Accounting Specification](rhei-cost-accounting.spec.md).

## Overview

Rhei can spawn coding agents directly from `rhei run`. Instead of requiring hand-written `workflow.sh` callback scripts, the run command resolves an agent for each task, composes a prompt from the state machine instructions, and spawns the agent as a subprocess. The spawned agent does the work for the current state, writes any required artifacts, and exits. `rhei run` remains the transition authority: after the subprocess exits, the engine evaluates the declared forward transitions and performs the state change itself. Callbacks still fire on transitions — agents and callbacks are complementary.

## 1. Agent Configuration

Rhei settings separate **model identity** from **agent transport** and
**tooling**:

- A **model profile** is the semantic identity used by state machines, callbacks,
  templates, logs, and multi-model execution.
- An **agent profile** is the CLI transport used when `rhei run` spawns an
  autonomous coding agent.
- A model profile names the concrete provider/model pair and may define
  agent-specific launch overrides for autonomous execution.
- An **MCP server profile** describes a Model Context Protocol server the agent
  can connect to for tool access during a state.
- A **skill profile** describes a reusable agent-side prompt/resource bundle
  enabled for a state.

This separation keeps callback-only workflows model-centric while still letting
`rhei run` resolve the exact subprocess invocation, tool surface, and skill
bundle when agent mode is enabled.

### 1.1. Global and Project Settings

Files:

- Global: `~/.config/rhei/settings.json`
- Project: `<plan-root>/.agent-grounds/rhei/settings.json`, and failing that
  `<plan-root>/.agents/rhei/settings.json`, in the workspace or plan directory

Both files use the same schema. Project settings compose with global settings by
key rather than replacing the whole file.

The effective agent/model roster and the exact files selected for this merge
are inspectable with `rhei roster`; its output contract is §1.1.7.

**The project settings file is resolved by first match, and the two project
paths are never merged.** `.agent-grounds/rhei/settings.json` is read when it
exists; only when it does not is `.agents/rhei/settings.json` read. Merging the
two would invent a fourth precedence tier between global and project that
nothing else in this specification has, and would leave the deprecation with no
end. Whichever file is chosen then composes with global settings exactly as
above.

`.agents/rhei/` is the deprecated home for rhei's project-local material and
reading from it prints a deprecation warning naming both paths; the warning's
contract — stderr, once per distinct path per process, silent when only
`.agent-grounds/rhei/` is present — is §FS-rhei-templates.1.3, and the reason
the directory moved is §FS-rhei-templates.1.1. Rhei writes only
`.agent-grounds/rhei/settings.json`.

**A message that names the project settings file names the one being read.**
Where rhei tells an author to define an agent, a model, or a default in project
settings, the path it prints is the file the merge resolved — the deprecated one
when that is what the project has. Naming the write path there would tell a
project still on the old home to create a file that shadows the one rhei is
reading, and following that advice drops the registry the same error just
listed. A message with no plan root in scope names both paths rather than
choosing one.

```json
{
  "defaults": {
    "model": "impl-fast",
    "agent": null,
    "agent_mode": "yolo",
    "agent_timeout": "30m",
    "program_timeout": "10m",
    "mcp_servers": [],
    "skills": []
  },
  "agents": {
    "claude-code": {
      "command": ["claude"],
      "prompt_flag": "-p",
      "model_flag": "--model",
      "mcp_config_flag": "--mcp-config",
      "skill_flag": "--skill",
      "stdin_prompt": false,
      "modes": {
        "yolo":   ["--permission-mode", "bypassPermissions"],
        "safe":   ["--permission-mode", "default"],
        "review": ["--permission-mode", "plan"]
      }
    },
    "codex": {
      "command": ["codex", "exec"],
      "model_flag": "--model",
      "mcp_flag": "--mcp",
      "stdin_prompt": true,
      "modes": {
        "yolo": ["--sandbox", "danger-full-access", "--skip-git-repo-check", "-c", "approval_policy=\"never\""],
        "safe": ["--sandbox", "workspace-write"]
      }
    },
    "pi": {
      "command": ["pi"],
      "prompt_flag": "-p",
      "model_flag": "--model",
      "skill_flag": "--skill",
      "stdin_prompt": false
    }
  },
  "models": {
    "impl-fast": {
      "provider": "anthropic",
      "model": "claude-sonnet-4-6",
      "default_agent": "claude-code",
      "agents": {
        "claude-code": {
          "args": ["--permission-mode", "default"],
          "autonomous_args": ["--permission-mode", "bypassPermissions"]
        }
      }
    },
    "review-deep": {
      "provider": "openai",
      "model": "o3",
      "default_agent": "codex",
      "agents": {
        "codex": {
          "args": ["--sandbox", "workspace-write"],
          "autonomous_args": [
            "--sandbox",
            "danger-full-access",
            "--skip-git-repo-check"
          ]
        }
      }
    }
  },
  "mcp_servers": {
    "linear": {
      "command": ["npx", "-y", "@modelcontextprotocol/server-linear"],
      "env": { "LINEAR_WORKSPACE": "${LINEAR_WORKSPACE}" }
    },
    "postgres": {
      "command": ["mcp-postgres", "--readonly"],
      "env": { "DATABASE_URL": "${DATABASE_URL}" }
    },
    "grafana": {
      "url": "https://grafana.internal/mcp",
      "transport": "sse"
    }
  },
  "skills": {
    "security-review": { "path": "~/.claude/skills/security-review" },
    "test-authoring":  { "path": ".rhei/skills/test-authoring" }
  },
  "snapshots": {
    "cache_dir": ".rhei/cache/snapshots",
    "redactor": null
  }
}
```

#### 1.1.1. `defaults`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string or null | No | Default model profile id |
| `agent` | string or null | No | Default agent id resolved against the `agents` registry. Inline agent objects are not accepted — define custom agents in `agents.<id>` and reference them here by id. |
| `agent_mode` | string or null | No | Default agent mode (named flag set) applied when a state does not set `agent_mode`. |
| `agent_timeout` | string or null | No | Default autonomous agent timeout |
| `program_timeout` | string or null | No | Default program timeout |
| `attempts` | integer or null | No | Default number of spawns one state visit may have before the run halts. See [Attempt Budget](#323-attempt-budget). |
| `budget_threshold` | object or null | No | Default positive `{currency, amount_micro}` containment threshold for neural states. State-level `budget_threshold` wins. §FS-rhei-budgets.2.1 |
| `mcp_servers` | array | No | Default MCP server entries applied to every agent state. Entries are ids or inline definitions. See [MCP Servers](#114-mcp_servers). |
| `skills` | array | No | Default skill entries applied to every agent state. Entries are ids or inline definitions. See [Skills](#115-skills). |

#### 1.1.2. `agents`

`agents` is a registry of agent transport profiles keyed by agent id. States
and defaults reference agents by id — inline agent definitions are not
permitted on a state or on `defaults.agent`. The registry is the only place an
agent's `command`, flags, and modes are declared.

It is not a qualification registry. An agent entry describes how to launch a
transport but cannot prove or opt into a provider-spend bound; exact
qualification is independently verified under §FS-rhei-budgets.6.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string array | Yes | Base command and fixed arguments |
| `prompt_flag` | string | No | Flag that carries the prompt (e.g., `--prompt`, `-p`) — or, with `stdin_prompt`, the flag that puts the agent in non-interactive mode, emitted with no value. Omit when the agent needs neither. |
| `model_flag` | string | No | Flag to pass the concrete provider model name. Omit if the agent doesn't support model selection. |
| `stdin_prompt` | boolean | No | When `true`, the prompt is piped to stdin instead of being passed as the `prompt_flag` value, `prompt_flag` is still emitted if present, a `--` is appended last, and stdin is closed unless `intervene_stdin` is also true. Default: `false`. |
| `intervene_stdin` | boolean | No | When `true`, `rhei run --dashboard` keeps the child stdin pipe open after the initial prompt so live dashboard interventions can be written to it. Use only for agents that start work without waiting for stdin EOF. Default: `false`. |
| `timeout` | string | No | Default timeout for this agent (e.g., `30m`). Overridden by state-level `agent_timeout`. |
| `mcp_flag` | string | No | Flag used to attach one MCP server per occurrence. `rhei run` emits the flag once per resolved server with a launch spec as its value. Mutually exclusive with `mcp_config_flag`. |
| `mcp_config_flag` | string | No | Flag used to attach a generated MCP config file. `rhei run` writes the resolved set to a temporary JSON file and passes it with this flag once. Mutually exclusive with `mcp_flag`. |
| `skill_flag` | string | No | Flag used to enable one skill per occurrence. `rhei run` emits the flag once per resolved skill id. Omit to declare the agent does not support skills. |
| `deny_read` | object | No | Optional read-denial adapter. In v1 its only field is required `path_flag`: a non-empty string repeated once per resolved excluded absolute path before the profile separator. |
| `modes` | object | No | Named flag sets, keyed by mode name. Values are ordered string arrays appended to the command at spawn time. See [Modes](#22-modes). |
| `effort` | object | No | Native reasoning-effort mapping. Contains `values`, `args`, and optional `conflicts` as defined below. Omission means this profile ignores valid state effort. |
| `session` | object | No | Optional `CustomAgentProfile.session` block describing snapshot resume, fork, interactive continuation, and transcript layout capabilities. The authoritative schema is [Snapshots Specification — CustomAgentProfile.session](rhei-snapshots.spec.md#91-customagentprofilesession). |

An `effort` mapping has this shape:

```json
"effort": {
  "values": { "low": "low", "high": "high" },
  "args": ["--effort", "{value}"],
  "conflicts": [["--effort", "{value}"], ["--effort={value}"]]
}
```

`values` is a non-empty object whose keys are canonical state values and whose
string values are native spellings. `args` is a non-empty token array
containing exactly one `{value}` placeholder. `conflicts`, when present, is an
array of non-empty token patterns, each containing exactly one `{value}`.
`args` is always an implicit conflict pattern, whether or not it is repeated in
`conflicts`. Settings loading rejects unknown canonical keys, empty native
values, malformed placeholders, and malformed conflict patterns.

The built-in registry declares these mappings:

| Agent | Canonical values | Native arguments |
|-------|------------------|------------------|
| `claude-code` | `low`, `medium`, `high`, `xhigh`, `max` | `--effort {value}` |
| `codex` | `minimal`, `low`, `medium`, `high`, `xhigh` | `-c model_reasoning_effort=\"{value}\"` |
| `kilocode` | `minimal`, `low`, `high`, `max` | `--variant {value}` |
| `pi` | `off`, `minimal`, `low`, `medium`, `high`, `xhigh` | `--thinking {value}` |
| `gemini`, `cursor` | none | valid state effort is ignored |

**Read-denial path arguments.** Each `deny_read.path_flag` value is an absolute
path. A recursive directory value ends in `/` on every platform; an exact-file
value has no trailing separator. Preserve this distinction for both logical
and canonical targets, including targets absent at spawn. Wrappers must decode
the trailing `/` before normalizing the path and enforce it for the process
tree: creating a directory later does not turn an exact-file denial into a
recursive denial, and a recursive denial covers descendants created later.
For example, `--deny-read /checkout/private` denies only that exact target;
`--deny-read /checkout/private/` also denies `/checkout/private/child.txt`.
These pairs precede the profile separator, using the existing `path_flag`
schema. A wrapper must not infer recursion from the current filesystem type.

**A prompt flag set alongside `stdin_prompt` is emitted, with no value.** The
two fields answer different questions — which flag makes the agent
non-interactive, and where the prompt text travels — so declaring both is not a
contradiction to be resolved by dropping one. `claude -p` is a boolean
`--print`, and a profile that names it means it. Dropping the flag instead
leaves the agent's mode to whatever else happens to be on the command line: a
setting the author wrote and rhei discarded without a word, which is the same
defect as a separator swallowing a tooling flag. An agent whose prompt flag
requires a value does not set `stdin_prompt`.

Built-in agent ids (see [Known Agent Profiles](#2-known-agent-profiles)) are
preloaded as the default agents registry. A user entry with the same id in
global or project settings replaces the built-in entry wholesale — `command`,
flags, and `modes` are taken from the user entry without field-level merging.

**Enabling live intervention.** No built-in agent enables `intervene_stdin`: the
known coding agents read their prompt and then run autonomously without consuming
more stdin, so the Flow composer ([§FS-rhei-viz](rhei-viz.spec.md#fs-rhei-viz-flow-visualization) §5) and `rhei intervene` are hidden
for them. To make a live agent messageable, register an agent whose transport
keeps reading stdin after it starts, and set both `stdin_prompt` and
`intervene_stdin`:

```json
// .agent-grounds/rhei/settings.json — an agent reachable for live intervention
{
  "agents": {
    "my-interactive-agent": {
      "command": ["my-agent", "--stream-stdin"],
      "model_flag": "--model",
      "stdin_prompt": true,      // deliver the initial prompt on stdin
      "intervene_stdin": true    // and keep stdin open for follow-up messages
    }
  }
}
```

`stdin_prompt` writes the initial prompt to the child's stdin; `intervene_stdin`
then keeps that pipe open for the process lifetime instead of closing it (the
EOF most headless agents wait on). For plain stdin transports, each delivered
message is written as one newline-terminated line. Set `intervene_stdin`
**only** for an agent that begins work without waiting for stdin EOF and
continues to read stdin afterwards; on an EOF-driven or one-shot agent the pipe
stays open but the agent never reads it, so messages would be silently buffered.
Run with `rhei run <plan> --dashboard --agent my-interactive-agent`; the
composer then appears on that agent's live node, and
`rhei intervene --task <id> -m "…"` reaches the same channel.

Delivering the prompt is the last step of a spawn, so a prompt Rhei cannot hand
over — a stdin write that fails, or a generated file such as `mcp_config_flag`'s
that cannot be written — is a warning naming the task and the path rather than a
failed spawn, and the run continues with the prompt delivered as far as the
transport allowed: the child is already running by then, and an agent handed
nothing does whatever it does with nothing, which is otherwise a silence the run
has no way to account for.

`claude-code` is the built-in exception to the plain-line format. When its
resolved profile has `intervene_stdin: true`, Rhei invokes Claude Code with
`-p --input-format stream-json --output-format stream-json --verbose`, writes
the initial prompt as a stream-json user message on stdin, and writes every
intervention as another stream-json user message on the same held-open pipe. It
must not pass the prompt as the `-p` argument in this mode, because that is a
one-shot invocation that does not consume follow-up stdin.

#### 1.1.3. `models`

`models` is a registry of named model profiles keyed by model id.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `provider` | string | Yes | Provider identifier such as `openai` or `anthropic` |
| `model` | string | Yes | Concrete provider model name such as `o3` or `claude-sonnet-4-6` |
| `default_agent` | string | No | Preferred agent id when `rhei run` needs to spawn this model autonomously |
| `agents` | object | No | Per-agent launch overrides for this model, keyed by agent id |
| `prices` | object or `null` | No | Static accounting rates for this profile, or `null` to clear inherited rates |

When present and non-null, `models.<id>.prices` is one whole rate object:

```json
{
  "currency": "USD",
  "effective_at": "2026-09-18T00:00:00Z",
  "input_total_micro": 4000000,
  "input_cached_read_micro": 400000,
  "input_cache_write_micro": 5000000,
  "output_total_micro": 20000000,
  "note": "priority tier, standard context",
  "service_tier": "priority"
}
```

`currency` and `effective_at` are required non-empty strings. All four rate
fields are required non-negative `u64` integers in micro-units of that currency
per one million tokens; zero is an explicit free rate. Fractions, negative
values, and values greater than `u64::MAX` are invalid. The unit is always
`1m_tokens` and is not authored. Validation diagnostics identify the full
`models.<id>.prices.<field>` path, including for a missing required field.

Other properties are explanatory metadata. Their JSON values are preserved in
the generated price-book entry but do not affect matching or arithmetic. The
generated entry keys `provider`, `model`, `unit`, `source`, and
`model_profiles` are reserved and cannot be used as metadata; a diagnostic
identifies the colliding path. Profile rates are static configuration: selecting
one performs no tariff selection and no network fetch.

Each `models.<id>.agents.<agent-id>` binding has this shape:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `args` | string array | No | Additional agent arguments for normal/inherited invocation |
| `autonomous_args` | string array | No | Arguments preferred by `rhei run` when launching autonomously |
| `timeout` | string | No | Timeout default specific to this model-agent binding |

#### 1.1.4. `mcp_servers`

`mcp_servers` is a registry of named MCP server profiles keyed by server id.
Entries describe how to launch or connect to a Model Context Protocol server
so it can be attached to an agent subprocess.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `command` | string array | One of `command` / `url` | Command and arguments to launch a local MCP server |
| `url` | string | One of `command` / `url` | URL of a remote MCP server. Requires `transport`. |
| `transport` | string | No | Transport for remote servers. Supported values: `sse`, `websocket`. Ignored for `command`-based servers. |
| `env` | object | No | Environment variables for the server process. Values may reference host environment with `${VAR}` syntax. Only meaningful for `command`-based servers. |
| `working_directory` | string | No | Working directory for the server process. Only meaningful for `command`-based servers. |
| `startup_timeout` | string | No | Maximum time to wait for the server to complete its MCP handshake after launch. Duration format (`30s`, `10s`, …). Default: `10s`. |

`command` and `url` are mutually exclusive. An entry must declare exactly one.

#### 1.1.5. `skills`

`skills` is a registry of named skill profiles keyed by skill id. Each entry
identifies an agent-side skill bundle that should be enabled for states that
list it.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Filesystem path to the skill bundle. Leading `~` expands to the user's home directory. |
| `description` | string | No | Human-readable description of the skill's purpose. |

Skills are agent-specific capabilities. A resolved skill is wired to the agent
only when the resolved agent profile declares a `skill_flag`; otherwise the
skill is skipped with a warning (see [Missing Tooling](#6-missing-tooling)). This
keeps state machines portable across agents that do not implement skills.

#### 1.1.6. `snapshots`

`snapshots` is an optional top-level settings block for session snapshot
storage, redaction, cache TTLs, and experimental adapter gates. This spec only
declares where the block lives in settings; field definitions and defaults are
authoritative in [Snapshot Operations Specification — Configuration](rhei-snapshot-operations.spec.md#4-configuration).

#### 1.1.7. Inspecting the effective roster

`rhei roster [RHEI_PLAN]` is a read-only view of the settings registry that
execution would use for the selected project. It reports the complete effective
`agents` registry, including every mode, the complete effective `models`
registry and its per-agent bindings, and the canonical nested `defaults`. It
does not report deprecated top-level default keys, state or task selectors,
run-time overrides, MCP/skill/snapshot registries, template source settings, or
settings copied into rendered plans.

The optional target uses the shared explicit and omitted target discovery in
[Panta project scope](rhei-panta.spec.md#6-project-scope-and-command-behavior).
A member rhei widens to its project root before settings are selected, including
a member of a Panta project. With no target, discovery must find an enclosing
project or workspace, or the lone rhei in the invocation directory; the current
directory alone is not treated as a project. Failure uses the ordinary
actionable plan/project discovery diagnostic. The command has no `--rhei`
narrowing because all members of a project use the same merged settings.
Finding the settings root does not parse or validate task bodies, so malformed
task Markdown does not prevent registry inspection.

`--json` writes one pretty-printed JSON object to stdout. Schema version 1 has
exactly these stable top-level fields:

| Field | Value |
|-------|-------|
| `schema_version` | Integer `1` |
| `project_root` | Resolved path of the root whose settings were merged |
| `sources` | The built-in, global, and project source metadata below |
| `agents` | Effective agent profiles keyed by agent id, using the fields in §1.1.2 |
| `models` | Effective model profiles and bindings keyed by id, using the fields in §1.1.3 |
| `defaults` | Effective canonical nested defaults, using the fields in §1.1.1 |
| `provenance` | Origins for the independently merged values below |

`sources` always contains all three keys. `sources.built_in` is an object with
the running Rhei `version`. `sources.global` is `null` when no global settings
file was read, otherwise an object containing its resolved `path`.
`sources.project` is `null` when no project settings file was read, otherwise
an object containing its resolved `path` and `home`, whose value is `current`
for `.agent-grounds/rhei/settings.json` or `deprecated` for the fallback
`.agents/rhei/settings.json`. The current home wins when both exist; the two
files are never merged. Reading the deprecated home retains its existing
stderr warning.

Every provenance leaf is one of the source keys `built_in`, `global`, or
`project`:

- `provenance.agents.<id>` is one origin for the whole agent profile because an
  agent entry is replaced wholesale.
- `provenance.models.<id>.<field>` records the origin of each supplied
  `provider`, `model`, and `default_agent` field.
- `provenance.models.<id>.agents.<agent-id>.<field>` records the origin of each
  supplied `args`, `autonomous_args`, and `timeout` binding field.
- `provenance.defaults.<field>` records the origin of each supplied canonical
  default.

The settings loader records these origins in the same decisions that perform
the merge in §1.3. The roster renderer consumes that merged result and must not
re-read settings documents or perform a second merge. Existing execution
callers receive unchanged effective settings. A mixed model or binding reports
the origin of each inherited and overriding field rather than assigning one
origin to the containing object.

An optional field never supplied by any layer is absent from both its value
object and provenance object. An explicit clear remains present: `null` for an
optional scalar, and `[]` for list-valued defaults or binding arguments, with
the clearing layer as its provenance. Empty registry maps remain objects. This
distinguishes omission, inheritance, and clearing without changing the field
names used in settings.

Without `--json`, output is a compact human summary in this order: the selected
sources and canonical defaults; agents, each with its source and mode names;
then models and bindings, with a compact source annotation for every displayed
field. The text view need not repeat full transport commands or binding values;
the JSON view is the complete machine-readable contract.

Selected settings are read and validated before either renderer writes a
payload. An unreadable, malformed, undecodable, or intrinsically invalid
selected file therefore emits no partial stdout. Under `--json`, failure keeps
the single JSON error-with-help object on stderr; text failures and all warnings
also stay on stderr. Both successful renderings treat an early stdout closure
as normal pipeline termination under [§FS-rhei-usage.2.2](rhei-usage.spec.md#22-command-surface).

Inspection has no execution side effects: it writes no file, creates no
runtime tree, performs no transition or migration, and spawns no subprocess.

### 1.2. Per-State Settings

The `target` / `all_targets` fields are the preferred execution selectors on
state definitions in `states.yaml`. The legacy `model` / `all_models`,
optional `agent`, `mcp_servers`, and `skills` fields remain supported for
compatibility. See
[States Specification — Agent Field](rhei-states.spec.md#5-agent-field) and
[States Specification — MCP Servers and Skills](rhei-states.spec.md#7-mcp-servers-and-skills).
State `effort` is also a per-state execution setting, but it is resolved after
the effective agent and never participates in target or mode selection.

### 1.3. Merge Semantics

Built-in agents load first, global settings compose over them, then project
settings compose over the result:

- `defaults` shallow-override by field.
- `agents` merge by agent id. A user entry with the same id as a built-in or a
  global entry **replaces that entry wholesale** — there is no field-level
  merge within a single agent entry. To tweak just one mode of a built-in
  agent, redeclare the whole entry.
- `models` merge by model id.
- `models.<id>.agents` merge by agent id.
- `models.<id>.prices` is one optional object: absence inherits it, an authored
  object replaces it wholesale, and explicit `null` clears it. Its individual
  rate and metadata fields never merge.
- `mcp_servers` merge by server id.
- `skills` merge by skill id.
- `null` explicitly clears an inherited optional field.

`defaults.mcp_servers` and `defaults.skills` are replaced wholesale by the
project list when the project defines them, not concatenated. Use an empty
array (`"mcp_servers": []`) to clear inherited defaults, or repeat entries
explicitly to extend them. This keeps the precedence predictable — every
level's effective tooling default is the one written there.

This lets a project override only a model's concrete provider/model pair or
only a model-agent binding's autonomous arguments without redefining unrelated
global entries. In particular, overriding `provider` or `model` without a
`prices` key retains inherited rates; an author changing identity without
retaining those rates writes `"prices": null`.

### 1.4. Resolution Order

When `rhei run` or a callback needs model context for a task in a given state,
it resolves the model id in this order:

1. **CLI override** — `--model <MODEL>`
2. **State-level** — `model` on the state definition in `states.yaml`
3. **Project defaults** — `.agent-grounds/rhei/settings.json` `defaults.model`
4. **Global defaults** — `~/.config/rhei/settings.json` `defaults.model`

The resolved model id must exist in the merged `models` registry. If no model
is configured at any level, model-specific callback and template fields are
omitted.

The selected model id is retained as the invocation's profile identity through
CLI, state, project/global default, legacy `all_models`, and task `**Model:**`
precedence. An inline `target`, `all_targets`, or task `**Target:**` remains a
literal execution identity and does not implicitly select a profile merely
because its provider/model pair matches one. When a named profile with prices
overrides a literal target's model while retaining the target's provider, the
final provider/model pair must exactly equal the profile's declared pair or the
run is refused before any agent starts.

When `rhei run` is launching an autonomous agent, it resolves the agent id in
this order:

1. **CLI override** — `--agent <AGENT>`
2. **State-level** — `agent` on the state definition in `states.yaml`
3. **Project defaults** — `.agent-grounds/rhei/settings.json` `defaults.agent`
4. **Global defaults** — `~/.config/rhei/settings.json` `defaults.agent`
5. **Model default** — `models.<id>.default_agent`

The resolved id must match an entry in the merged `agents` registry
(built-ins → global → project). An id with no matching entry is a
configuration error:

```
error: agent 'my-agent' is not defined. Add an entry to agents.<id> in
.agent-grounds/rhei/settings.json or ~/.config/rhei/settings.json, or reference one of the
built-in ids (claude-code, codex, cursor, gemini, kilocode).
```

If no agent is configured at any level and the resolved model does not declare
a `default_agent`, `rhei run` fails:

```
error: no agent configured for model 'impl-fast'.
Set defaults.agent, the state's agent, models.impl-fast.default_agent, or pass --agent <AGENT> to rhei run.
```

For an ordinary single target, Rhei first selects the lower-precedence
identity from task `**Target:**` or `**Model:**`, state `target`, or the legacy
state and settings fields. It then overlays a supplied `--agent` on only the
agent segment and a supplied `--model` on only the model segment. The target's
provider and optional mode remain unchanged, including when the mode is
omitted. The effective agent supplies effort mapping and other agent-profile
behavior.

The composed selector goes through the same registry, mode, binding, timeout,
and effort checks as an authored selector. An unknown run-level agent or model,
or a preserved mode unsupported by the effective agent, is a run error that
names the incompatible value before any invocation is scheduled or spawned.

For a state that declares `all_targets`, this resolution is bypassed for the
fields encoded directly in each selector: the agent id, optional mode, optional
provider, and model name come from the selector itself, even when `--agent` or
`--model` is present. Validation must still
verify that the referenced agent exists and that any referenced mode exists on
that agent. For the legacy `all_models` form, agent resolution still runs
independently for each model-specific execution of the state through the normal
order above. When snapshots are enabled, each legacy `model` or `all_models`
execution must resolve an effective target tuple `(agent, mode?, provider,
model)` before snapshot emit or inherit can run; otherwise explicit snapshot
fields are rejected and auto-emit is skipped.

#### 1.4.1. Mode Resolution Order

When the resolved agent declares `modes`, Rhei selects one in this order:

1. **CLI override** — `--agent-mode <MODE>`
2. **State-level** — `agent_mode` on the state definition in `states.yaml`
3. **Merged nested default** — `defaults.agent_mode`, with the project setting
   overriding the global setting
4. **Merged legacy default** — the backward-compatible top-level
   `agent_mode`, with the project setting overriding the global setting; used
   only when the nested default is absent
5. **Registry default** — the first declared mode in the agent entry's
   `modes` map
6. **None** — if the agent entry has no `modes`, no mode flags are appended

An explicit state `target` or `all_targets` selector, or a task
`**Target:**`, carries its own optional mode and bypasses this legacy
resolution order. A state-level mode shadows both settings defaults. Selection
of the effective agent still follows §1.4, including state, merged settings,
and model-default agents.

State effort follows that effective selection without modifying it. A task
`**Model:**` keeps the effort on the existing profile; a task `**Target:**`
uses the replacement profile's mapping. Same-id registry overrides remain
wholesale, so replacing a built-in profile without redeclaring `effort` makes
that effective profile unsupported and the authored value is ignored.

Before execution, validation applies this order without the run-only CLI
override to every applicable static selection. If both an agent and a mode are
effective and the agent's `modes` map is non-empty, the mode must be a key in
that map; otherwise validation refuses the plan and identifies the mode, the
agent, `agents.<id>.modes`, and the effective project and global settings
locations ([§FS-rhei-errors.1.4](rhei-errors.spec.md#14-where-the-refused-name-is-declared)). An explicit selector remains subject to its
own selector validation and does not also validate a shadowed legacy fallback.
No mode is rejected when no agent is effective or when the effective agent
declares no modes.

`--agent-mode` remains a `rhei run` option and is not accepted by
`rhei validate`; run checks that dynamic selection at runtime. `rhei run` also
performs the static validation pass first, so an invalid static selection is
refused before scheduling or spawn.

When `rhei run` composes the tool surface for a state, it resolves the
effective MCP server and skill sets:

1. Start from `defaults.mcp_servers` (and `defaults.skills`) as resolved by the
   merged settings.
2. Union with the state's `mcp_servers` (and `skills`) list, if any. An empty
   list on the state clears the defaults for that state.
3. Deduplicate by id. Within a single effective set, later entries for the same
   id override earlier ones — a state-level override wins over a defaults
   entry.
4. Resolve each id against the merged `mcp_servers` / `skills` registry. Inline
   object entries on the state (or in `defaults`) do not require a registry
   entry; they are used as-is.
5. An id with no registry match and no inline definition is a validation
   error.

The resolved sets are distinct per state and per invocation. Changing the
current state or restarting `rhei run` recomputes them.

### 1.5. Partial Overrides

Each level can override model and agent independently:

```yaml
# states.yaml — state overrides only the model profile
states:
  agent-review:
    model: review-deep
```

```json
// .agent-grounds/rhei/settings.json — project sets the default agent transport
{
  "defaults": {
    "agent": "codex"
  }
}
```

Result for `agent-review`: model=`review-deep` (from state), agent=`codex`
(from project defaults unless `review-deep.default_agent` or a CLI override
supersedes it).

An explicit single target composes the two CLI dimensions independently:

| Run flags | Effective target for `state-agent[yolo]:state-provider:state-model` |
|-----------|------------------------------------------------------------------------|
| neither | `state-agent[yolo]:state-provider:state-model` |
| `--agent override-agent` | `override-agent[yolo]:state-provider:state-model` |
| `--model override-model` | `state-agent[yolo]:state-provider:override-model` |
| both | `override-agent[yolo]:state-provider:override-model` |

The same composition applies after selecting a task `**Target:**` or
`**Model:**`. A selector without `[mode]` remains without a mode; substituting
an agent does not select that agent's default mode. Provider and mode are never
inferred from the run-level agent or model profile.

## 2. Known Agent Profiles

Rhei ships with built-in invocation profiles for known coding agents. Each
profile defines how to spawn the agent, deliver the prompt, pass the concrete
provider model name, and set transport-level defaults. Each built-in also
declares a `yolo` mode carrying the autonomous/dangerous flag set that was
historically the agent's default.

| Agent ID | Binary | Prompt Delivery | Model Flag | MCP Wiring | Skill Wiring | `yolo` Mode Flags |
|----------|--------|-----------------|------------|------------|--------------|-------------------|
| `claude-code` | `claude` | stdin, under a bare `-p`, with typed `--output-format json`; with `intervene_stdin`, stream-json stdin/output | `--model <m>` | `--mcp-config <path>` | `--skill <id>` | `--permission-mode bypassPermissions` |
| `codex` | `codex exec` | `--` (stdin) | `--model <m>` | `--mcp <spec>` (per server) | unsupported | `--sandbox danger-full-access --skip-git-repo-check -c approval_policy="never"` |
| `gemini` | `gemini` | `--prompt <prompt>` | `--model <m>` | unsupported | unsupported | `--approval-mode yolo` |
| `cursor` | `cursor-agent` | `--print <prompt>` | `--model <m>` | unsupported | unsupported | `--force` |
| `kilocode` | `kilo` | positional via `--auto <prompt>` | `--model <m>` | unsupported | unsupported | `--yolo` |
| `pi` | `pi` | `-p <prompt>` | `--model <m>` | unsupported | `--skill <path>` | (no modes — pi has no permission layer; isolate at the sandbox/container level) |

**Prompt delivery is a property of the transport, not of the prompt.** A profile
either passes the prompt as one command-line argument or writes it to the
agent's stdin, and it does the same for a one-line prompt and for a supervisor
brief of two hundred kilobytes. Every supported platform caps what a command
line may carry — Linux at 131072 bytes for any single argument, Windows at 32767
characters for the whole line, macOS at roughly a megabyte in total — so an
agent known to read its prompt from stdin is declared that way rather than left
to fail at spawn once a plan has run long enough to compose a large brief.
`claude-code` and `codex` are the built-ins that read stdin. The rest carry the
prompt in `argv`, are bounded by the platform's cap, and say so when they reach
it ([§FS-rhei-errors.7](rhei-errors.spec.md#7-a-spawn-failure-the-command-lines-size-explains)).

The agent IDs match those used by `rhei install-skills --agent`.

Snapshot session support is tracked separately from prompt/model invocation.
In v1, built-in `cursor` and `kilocode` profiles do not expose a supported
`CustomAgentProfile.session`; auto-emit is skipped and explicit snapshot
operations fail with `unsupported-snapshot-session`. Gemini snapshot support is
provisional and remains unsupported until the snapshot adapter spike resolves
its resume and path layout.

Agents marked `unsupported` for MCP or skills receive a warning at spawn time
when the resolved state's effective set includes entries they cannot consume.
Required MCP entries (see [Missing Tooling](#6-missing-tooling)) escalate the
warning to an error.

A user-written entry for one of these ids in `settings.json` replaces the
built-in entry wholesale (see [Merge Semantics](#13-merge-semantics)).
None of the six built-in profiles declares `deny_read`; each therefore provides
composition-only exclusion. A mode name, model, permission flag, or writable
root never implies read denial.

The `codex` adapter recognizes one provider refusal in addition to its normal
process result. Recognition requires all of the following:

- the resolved agent id is exactly `codex` and its resolved provider is exactly
  `openai`;
- the invocation exited non-zero and was neither timed out nor interrupted;
- after stripping terminal-control decoration and trimming surrounding
  whitespace from each captured stdout and stderr line, exactly one line is
  `You've hit your session limit · resets <h>:<mm><am|pm> (<zone>)`, where
  `<h>` is `1` through `12`, `<mm>` is two digits from `00` through `59`, the
  meridiem is lowercase, and `<zone>` is an installed IANA time-zone name whose
  local time is valid and unambiguous. Terminal decoration means ANSI escape
  sequences; removing it must not otherwise rewrite the line.

Matching is case-sensitive and does not search inside prose. An exit of `0`, a
different transport, absent or malformed reset information, an absent or
invalid zone, a nonexistent or ambiguous local time, or a second matching line
is an ordinary process result. Timeout and interruption classification happen
before this recognition, so their output can never turn them into a provider
limit. The recognized result is the provider-limited ending specified in
§FS-rhei-run.3.3.

A known id or matching command line is not a spend qualification. Every
Rhei-started built-in or custom agent enters §FS-rhei-budgets.4. Only exact
tuples present in the qualification registry may launch; the initial real
qualification work and deliberately unqualified clients are listed in
§FS-rhei-budgets.6.5.

### 2.1. Custom Agents

When the built-in profiles don't fit, declare a new agent in the `agents`
registry — **never inline** on a state or on `defaults.agent`:

```json
{
  "agents": {
    "my-agent": {
      "command": ["my-agent", "--autonomous"],
      "prompt_flag": "--prompt",
      "model_flag": "--model",
      "stdin_prompt": false,
      "modes": {
        "yolo": ["--permissions", "full"],
        "safe": ["--permissions", "read-only"]
      }
    }
  }
}
```

States and defaults then reference the agent by id:

```yaml
states:
  pending:
    agent: my-agent
    agent_mode: yolo
```

```json
{
  "defaults": {
    "agent": "my-agent",
    "agent_mode": "safe"
  }
}
```

Inline agent objects are rejected by the validator. This keeps state
machines portable — the transport surface lives in `settings.json` where a
project or a user can replace it without touching `states.yaml`.

### 2.2. Modes

A mode is a named ordered list of extra CLI flags. When `rhei run` spawns the
agent, the resolved mode's flags are appended right after the base
`command`, before the prompt and model flags. The full flag order is:

```
<command...> <mode flags...> <autonomous_args...> <effort args...>
  <accounting flags...>
  <prompt_flag> <prompt>?
  <model_flag> <model>?
  <snapshot strategy flags...>
  <mcp flags...> <skill flags...>
  --
```

The prompt value is present only when `stdin_prompt` is `false`; the flag itself
is emitted either way. The `--` is emitted only when `stdin_prompt` is `true`,
and it is the **last** argument rhei composes. Past a separator an argument is
positional text rather than a flag, so everything rhei has to say to the agent —
the snapshot strategy flags
([§FS-rhei-snapshots.10.1](rhei-snapshots.spec.md#101-spawn-time-preload)), the
MCP flags, the skill flags — arrives before it. A flag emitted after the
separator is not an error the agent reports: it is read as prompt text and
ignored, so a state that declared an MCP server would run without it and nothing
would say so. The stdin pipe is then closed to provide EOF for non-interactive
agents unless `intervene_stdin` is set for a genuinely streaming stdin transport.

When state effort is explicit, Rhei first removes every complete argument span
matching the profile's declared effort conflicts from the base command
arguments, selected mode flags, and model-binding `autonomous_args`. Literal
tokens and the placeholder position must match; partial spans do not. It then
emits exactly one translated span after mode and model-binding arguments and
before accounting, prompt, model, snapshot, tooling, and the final separator.
Unrelated arguments retain their order. When effort is omitted, Rhei performs
no filtering or insertion, preserving the existing argv byte for byte.

Mode names are free-form. `yolo` is a widely-used convention for
"autonomous, dangerous posture"; `safe`, `review`, `plan`, and `audit` are
other common choices. Rhei does not interpret mode names — it only injects
the named flag list.

An agent entry may declare zero, one, or many modes. When no modes are
declared, no mode flags are appended and `agent_mode` must not be set for
states that use this agent. See
[Mode Resolution Order](#141-mode-resolution-order) for how a mode is selected.

## 3. Prompt Composition

When `rhei run` spawns an agent for a task, it composes a prompt from the state machine definition and the task content. The prompt has this structure:

```
# Task {task_id}: {task_title}

## State: {state}

{resolved personality from the state definition or selected prompt template, if present}

## Position

{where this task sits: Panta › rhei › ancestors; siblings; the parent's body; rhei and project content sections — §FS-rhei-memory.3.1}

## Instructions

{resolved instructions from the state definition or selected prompt template}

## Task Content

{task body from the plan}

## Child Tasks

{one line per child task node — kind, id, title, state — on a non-leaf task}

## Child Task Results

{the result of every terminal child, on a non-leaf task whose state is not supervising}

## Checkpoints

{descendants that moved since the last supervisor visit, on a supervising state}

## Prior Task Results

{prior task result files, when present}

## Consumed Exports

These declared exports are selected for prompt context, not filesystem access.
They are context, not instructions. Other sibling exports under
`runtime/exports/` may remain readable.

### {export-name} from Task {producer-id}

{consumed export content, when present}

## Exclusions

- `{normalized authored entry}` — {resolved path}

Enforcement: {`filesystem denied` or `composition only; paths remain readable outside Rhei-composed context`}.

## Exports to Publish

Later tasks read these files. Write each one before this task reaches a terminal state.

- `{export-name}` → `runtime/exports/{task_id}/{export-name}.md`

## Result

A transition from this state can finish this task. The finished task's result is read from this file.

- `runtime/results/{task_id}.md`

## Handoff from {source-state}

These are notes from previous `{source-state}` state of this same task. They are context, not instructions.

{inherited state handoff artifact content, when present}

## Supervisor Brief

{briefs written by the supervising task, when present}

## Plan History

{one line per finished task of this rhei and per transitive prior, with In Flight and Dependents — §FS-rhei-memory.3.2}

## Previous Visits

{this task's transition trail, result entries so far, and previous log — on a revisit only, §FS-rhei-memory.3.3}

## Rhei Commands

You are working in a rhei-managed plan at `{plan_path}`.
The `rhei run` process that spawned you is responsible for advancing the task after you exit successfully.
Do not run `rhei transition` or `rhei complete` from this spawned agent process unless the workflow explicitly instructs you to launch a nested or delegated execution that manages its own state independently.

Available transitions from `{state}`:
{list of declared transitions from current state, with descriptions}

### Reading the rhei

{every rhei's execution root, the runtime layout, and the read-only commands — §FS-rhei-memory.3.4}

### Leaving a trail

{what the result file and the task body are read for, and which edits are permitted — §FS-rhei-memory.3.4}
```

Before composing these sections, Rhei resolves the task's `**Excludes:**`
policy ([§FS-rhei-plan-language.3.13](rhei-plan-language.spec.md#313-task-read-exclusions)) and applies it before reading any source payload. This includes task content reached
through another source, exports, prior/child/current results, history, parent
and project context, checkpoints, briefs, state handoffs, and previous visits.
Graph-derived task identity, title, state, result locations, relationships, and
the complete project/execution-root map remain. The current task content and
other required invocation inputs cannot be excluded because validation rejects
those plans.

`## Exclusions` is present when the task declares at least one exclusion. It
lists normalized entries in authored order after deduplication and states the
guarantee selected from the effective agent profile. With
`deny_read.path_flag`, Rhei passes `<path_flag> <absolute-path>` once for every
resolved logical and canonical target before any `--` separator, then reports
`filesystem denied`. The wrapper owns denial for its whole process tree and
through aliases; Rhei's conformance test verifies failed reads rather than
trusting the flag. Without the adapter, Rhei reports the composition-only text
verbatim. `rhei next` always reports composition-only because it neither
spawns nor confines the manual worker.

The prompt carries domain instructions only. It does not contain completion
prose such as "create every required output artifact and then exit":
completion is enforced by the state's [Completion Condition](#32-completion-condition),
not by prompt wording. Required artifact paths are already visible to the agent
via resolved `{output.<name>.path}` variables in the state's `instructions`,
and every supported agent exits deterministically after one turn in its native
headless mode.

Prior task results are resolved by the prompt builder from the current task's
`**Prior:**` graph. For each prior task that has a
`runtime/results/<task-id>.md` file, Rhei injects that result under `## Prior
Task Results` in `**Prior:**` order. This is graph-level context and is not
configured in `states.yaml`.

Task exports are resolved from the current task's `**Consumes:**` and
`**Provides:**` metadata ([§FS-rhei-plan-language.3.12](rhei-plan-language.spec.md#312-task-exports)). Each consumed export
is injected under `## Consumed Exports` in `**Consumes:**` order only after the
preflight in §3.3 accepts all of them. Each declared `**Provides:**` entry is
listed under `## Exports to Publish` with the path the agent must write. Like
prior task results, this is graph-level context and is not configured in
`states.yaml`.

The consumed-export introduction must identify `**Consumes:**` as prompt
selection and must not imply isolation: undeclared sibling exports under
`runtime/exports/` remain readable. Scheduling participants concurrently and
briefing them not to inspect sibling exports can reduce accidental
cross-reading, but neither enforces blindness once an export exists
(§FS-rhei-plan-language.3.12).

The `## Result` section is emitted when — and only when — some transition
declared **from this state by name** lands on a `final: true` state, so it
appears exactly on the invocations that can finish the ticket. A wildcard edge
(`from: "*"`, [§FS-rhei-transitions.4.6](rhei-transitions.spec.md#46-wildcard-semantics)) does not count: nearly every machine
declares `* -> cancelled`, and counting it would put the section — and with it
the invitation to write a result — on the first state of every workflow, where
a result written three states early would pre-satisfy the obligation at the
real terminal edge with a stale message. This is the same rule the human-gate
surfaces apply when they list a gate's choices ([§FS-rhei-viz.5.1](rhei-viz.spec.md#51-human-gate-transitions)). Its path is
the one every other surface uses ([§FS-rhei-complete.3](rhei-complete.spec.md#3-result-file)) — or, on a fanned-out
state, this invocation's own fragment
`runtime/results/<task-id>/<state>/<visit_count>/<identity>.md`
([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)), which is also the path the orchestrator validates for that
invocation — rendered relative to the artifact root, or absolute when the
checkout root differs from it, on the same rule `{output.<name>.path}` follows
(§4). Under `orchestrator` authority a
subprocess never calls `rhei complete`, so without this section the one
artifact a terminal state requires would be the only one the agent was never
shown. The section names the fact and the path and stops there: "write it, then
exit" is completion prose, and completion is enforced by the
[Completion Condition](#32-completion-condition).

State handoffs are resolved from `handoff.inherit` on the current state and
render as one `## Handoff from <state>` section per inherited source state. The
handoff text is context only; if it conflicts with current instructions or the
current task body, the current invocation wins. See
[States Specification — State Handoffs](rhei-states.spec.md#32-state-handoffs).

The subtree sections are resolved from the task tree. `## Child Tasks` lists
a non-leaf task's children; `## Child Task Results` injects the result file of
every terminal child when the state is not supervising; `## Checkpoints`
replaces it on a supervising state with the descendants that moved since the
supervisor's last visit, each carrying its result or its source state's
outputs. `## Supervisor Brief` renders the briefs a supervising ancestor wrote
for this task or this state; unlike handoffs, a brief is direction the agent
follows within the state's instructions and artifact contract. On a
supervising state, `## Rhei Commands` additionally permits `rhei transition`
against held descendants. See [§FS-rhei-supervision.5](rhei-supervision.spec.md#5-prompt-composition).

`## Position`, `## Plan History`, `## Previous Visits`, and the two
sub-sections of `## Rhei Commands` reconstitute the project's mid-term
memory for a cold invocation: where the task sits, what finished before it
anywhere in the Panta, what already happened to it, and how to read the
rest. They are composed by the fixed algorithm of [§FS-rhei-memory.4](rhei-memory.spec.md#4-composition-algorithm) and are
graph-level context, not configured in `states.yaml`. See [§FS-rhei-memory](rhei-memory.spec.md#fs-rhei-memory-mid-term-memory).

Reusable prompt templates are expanded from each state's
`prompt_template.values` before runtime template variables. Inline state
`personality` and `instructions` are then appended after the selected
prompt-template text, preserving direct per-state prompt definitions while
allowing shared prompt fragments. Runtime template variables in values and
inline prompt text (`{task_id}`, `{model}`, `{model.provider}`,
`{model.name}`, `{visit_count}`, etc.) are resolved before the prompt is sent,
using the same resolution rules as `rhei next`. See
[Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)
and [Prompt Templates](rhei-states.spec.md#44-prompt-templates).

The prompt is delivered to the agent via its configured prompt delivery mechanism (flag or stdin).

### 3.1. Completion Authority

Every state has a **completion authority** — the role that decides when the
state's work is done and drives the resulting transition. Rhei defines two
authorities; exactly one applies to any given execution of a state.

| Authority | Applies when | Transition driver |
|-----------|--------------|-------------------|
| `worker` | The invoking role is a manual worker (human, `rhei-plan-worker` skill session, or direct `rhei next` / `rhei transition` / `rhei complete` caller). | The worker calls `rhei transition` or `rhei complete`. |
| `orchestrator` | `rhei run` has spawned the agent or program for this state. | `rhei run` evaluates the state's [Completion Condition](#32-completion-condition), then selects and executes the matching forward transition. |

Completion authority is determined by the execution mode, not declared on the
state. The same state definition is legal under both authorities. This is what
lets a plan be driven either agent-by-agent (manual workers) or end-to-end
(`rhei run`) without rewriting `states.yaml`.

Normative rules:

- Under `worker` authority, the worker owns both the work and the transition.
  `rhei run` is not involved.
- Under `orchestrator` authority, the spawned subprocess owns the work;
  `rhei run` owns the transition. The subprocess must not call
  `rhei transition` or `rhei complete`, and must not edit `**State:**` lines
  directly. The one exception is a nested execution started from within the
  agent, which manages its own state independently of the outer `rhei run`.
- `instructions` and `personality` describe domain work only. They must not
  describe how or when to stop, whether to call transition commands, or how
  completion is detected. Those are properties of the execution model, not of
  the state.
- Gating states (`gating: true`) bypass completion authority: no subprocess is
  spawned and no automatic transition fires.

The worker-authority exception covers only externally launched sessions. Every
subprocess Rhei starts, including one framed as a custom worker or started by an
embedded runtime, is orchestrator-owned for admission and ancestry and must
pass §FS-rhei-budgets.4. A nested execution manages its own ticket state but
inherits the same Panta ledger and ancestor envelope. §REQ-bounded-neural-work.1

This separation keeps state transitions serialized through one orchestrator
even when many agents run in parallel.

### 3.2. Completion Condition

When completion authority is `orchestrator`, `rhei run` decides deterministically
when the state's work is complete. The completion condition is a property of
the state machine and the execution mode, not of the prompt — so the same
determinism applies to every execution of that state regardless of which agent
is resolved.

The condition is normative and universal for agent states:

1. The subprocess exits with code `0`, **and**
2. Every required artifact declared in the state's `outputs:` list exists on disk, **and**
3. When the transition this exit would select lands on a `final: true` state,
   `runtime/results/<task-id>.md` exists and is non-empty — or, on a fanned-out
   state, *this invocation's own* fragment
   `runtime/results/<task-id>/<state>/<visit_count>/<identity>.md` does
   ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)).

Condition (2) resolves declared `outputs:` against the execution root of the
rhei that owns the ticket ([§FS-rhei-plan-language.3.10](rhei-plan-language.spec.md#310-state-artifact-contracts))
— the same root the invocation's prompt names — at both
moments this condition is asked: after the subprocess exits, and before a pass
decides whether the invocation can be skipped. In a Panta project this root can
sit below the run-level workspace root that `rhei run` was pointed at; the two
must not be conflated.

All three are evaluated after the process exits — and condition (3) selects its
edge against the plan **as re-read after that exit**, not the copy the pass held
when it spawned, so a child the invocation appended or cancelled counts toward
the operands that selection reads ([§FS-rhei-supervision.4.1](rhei-supervision.spec.md#41-the-opendescendants-operand)), while the edge
itself still leaves the state the invocation ran in, not the state that re-read
plan now shows the ticket in. The skip test
below asks the same condition before anything has run, where the plan the pass
holds is already the current one. If the state declares no
`outputs:`, condition (2) is vacuously true. If the selected transition is
non-terminal, condition (3) is vacuously true — it is a property of the edge
being taken, not of the state being left ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). Condition (3) is
per-invocation for the same reason condition (2) is: a fanned-out state's
artifacts are keyed by invocation identity, and one worker's file must not
excuse a sibling that wrote nothing.

The same condition is also what lets `rhei run` decide *not* to spawn. Before a
pass spawns an invocation of a state that declares `outputs:`, it asks whether
that invocation's work is eligible for the current **visit** as well as already
on disk. A visit is the span during which the ticket's transition-ledger move
count does not change. The first visit to a canonical state may reuse deliberately
pre-populated work without a spawn when neither transition history nor an older
spawn record establishes an earlier visit to that state. Moves through other
states, including the first move into this state, do not alone establish
re-entry. A recorded departure from this state (including a self-loop), or
multiple recorded entries into it, does establish an earlier visit even when
its spawn record is absent. Counted state suffixes refer to the same canonical
state. When neither usable history source establishes an earlier visit, the
legacy first-visit interpretation remains available. After re-entry, each invocation is
eligible only when its own spawn record (§8.4) proves a successful exit in the
current visit. A restart or a withheld edge that has not moved the ticket stays
in the same visit and may reuse that successful proof.

The pass therefore skips an invocation only when visit eligibility and
conditions (2) **and** (3) all hold for that invocation — successful
current-visit proof when re-entry requires it, the declared artifacts, and, on
a terminal edge, its own result. An older-visit record, or a current-visit
record that failed, was interrupted, timed out, or was provider-limited, cannot
supply successful proof. The artifact and result checks remain independent of
that proof: one fan-out identity cannot answer for a sibling, a visit-templated
path resolves for the current visit as before, and a successful record cannot
excuse a missing artifact or required result. There is one completion
condition, asked at two moments; a scheduling test that looked only at the
declared outputs would read an invocation that belongs to an earlier visit as
having nothing left to do, and the ticket would advance on stale work.

A state that declares no `outputs:` is never skipped this way. It has no
artifact of its own that could stand as proof its work was done, and
`runtime/results/<task-id>.md` cannot stand in for one: that file is shared with
every other state the ticket has passed through, so a result written earlier
would excuse a state that has not run at all.

This contract maps 1:1 onto the native headless mode of every supported agent.
All six built-ins — `claude-code -p`, `codex exec`, `gemini --prompt --yolo`,
`cursor-agent --print --force`, `kilo --auto --yolo`, and `pi -p` — run one
turn-loop and exit. Rhei detects completion from process exit plus declared
output artifacts; no cross-agent stop signal, sentinel file, or "done" RPC is
defined or needed.

Program states have their own exit-code-driven completion semantics documented
in [Program States Specification](rhei-programs.spec.md#3-exit-code-transitions);
the `outputs:` clause of the condition above does not apply to them unless the
state also declares `outputs:`.

#### 3.2.1. Runtime Semantics

Under `orchestrator` authority, `rhei run`:

1. Spawns the subprocess and waits on `(subprocess exit) OR (timeout fires)`.
2. On timeout, sends `SIGTERM` to the subprocess, 10 s grace, then `SIGKILL`.
   Timeout transitions fire per [Timeout Handling](#7-timeout-handling). Agents
   that fork long-running descendants should install their own cleanup; the
   engine kills only the direct subprocess.
3. On non-zero exit, routes through the exit-code / error transition path
   documented in the [Execution Loop](#52-execution-loop). The artifact check is
   skipped.
4. On exit code `0`:
   - Verify every required output artifact exists, plus the terminal result
     when the selected transition lands on a `final: true` state. The result
     is reported in the same list under the artifact name `result`, with the
     same resolved-path rendering, because it is enforced by the same rule.
   - If any is missing, the task stays in its current state and the engine
     logs `warning: agent exited 0 but required outputs are missing for task
     {id} in state '{state}': <name1> (<path1>), <name2> (<path2>)` — with
     `program` in place of `agent` when a program state stalls the same way —
     followed by a halt line that says what the run will do next. That line is
     a **prediction**, so it is conditioned on the attempt budget of §3.2.3 and
     not on the completion condition alone. While the visit has an attempt left
     it reads `halting Task {id} in state '{state}': the completion condition is
     not met, so no transition fires; a later pass runs the state again.` When
     the attempt that just finished was the *last* one this visit gets, that
     sentence would be false — no later pass will run the state again — and the
     exhaustion line of §3.2.3 is printed in its place, word for word, at the
     moment it becomes true rather than one run later. An operator must never
     read a promise of a retry from the run that spent the last attempt and the
     denial of one from the run after it.
     No transition fires either way. The same facts are recorded structurally,
     so the run report classifies the ticket by the artifacts it owes rather
     than as a generic stall ([§FS-rhei-run-report.3.1](rhei-run-report.spec.md#31-layout)). The ticket is not
     spawned again within the pass and the run continues with the other
     claimable tickets ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) step 5).
   - A later pass that reaches the same ticket spawns the state again, because
     the invocation still fails the completion condition it is scheduled
     against (§3.2). The spawn says so as it happens, beside the `Log:` line:
     `Re-spawning Task {id} in state '{state}': attempt {n} of {budget}; the
     previous attempt {ending} (previous log: {path})` — one line, naming the
     attempt being spent, the budget it comes out of, and the transcript it is
     retrying, so a re-spawn is not read as a state being started for the first
     time. On a poll state ([§FS-rhei-states.2](rhei-states.spec.md#2-polling-states)) the budget named is the state's
     own `poll.max_attempts`, and the line says which mechanism it is:
     `attempt {n} of {max_attempts} (poll.max_attempts)` — that is the bound
     that actually applies, since §3.2.3 exempts a poll state from `attempts:`.
     The engine never prints its internal "no budget applies" sentinel as a
     count. `{ending}` is read from that attempt's spawn record (§8.4) and says
     what actually happened to it — `timed out after 30m`, `was interrupted by
     a run shutdown`, `exited 3`, or `exited 0 without meeting this state's
     completion condition`. The engine never reports one ending as another: a
     run that was Ctrl-C'd and a state that stalled on its outputs are
     different facts and a retry says which it is retrying.
   - A retry is only worth spawning if it can do better than the attempt before
     it, so a re-spawned invocation is told it is one. Its prompt names the
     attempt number, what the previous attempt left unmet, and the result path
     it did not write ([§FS-rhei-memory.4.4](rhei-memory.spec.md#44-previous-visits)). Without that the retry receives a
     byte-identical prompt and repeats the attempt it is meant to recover from.
   - The retry is bounded, per state visit, by the **attempt budget** of §3.2.3.
     Spawn number `{budget} + 1` of a visit does not happen: the ticket stays
     where it is, is not scheduled again for the rest of the run, and the engine
     logs `halting Task {id} in state '{state}': {budget} attempts spent on this
     visit and the completion condition is still unmet: <name1> (<path1>). The
     ticket stays in '{state}'.` No transition fires — not an error transition,
     not a timeout transition, and not a move to a terminal state. An exhausted
     budget is a stall like any other stall of step 5 ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)), and it
     is reported as one; the engine never converts "I stopped paying for this"
     into "this ticket failed".
     This is one sentence with two moments, not two sentences: it is printed by
     the attempt that spends the last of the budget, and again by any later pass
     that reaches the ticket and declines to spawn it. Both say the same thing
     because both are the same fact.
   - Each entry names the artifact and the **resolved** path that was checked,
     so a stale or mis-templated path is visible without re-deriving it. When a
     resolved path still contains an unresolved `{...}` template, the entry is
     suffixed with ` (unresolved template)` — the path template referenced a
     variable outside the namespace in
     [Variable Namespace](rhei-states.spec.md#41-variable-namespace), which is
     left verbatim by design.
   - Otherwise, evaluate forward transitions in normal selection order and
     execute the first match.

#### 3.2.2. Timeout Requirement

Under `orchestrator` authority, a timeout must resolve to a finite value
through the chain documented in [Timeout Handling](#7-timeout-handling). States
that resolve to no timeout at any level are a validation error under
`orchestrator` authority. Under `worker` authority there is no timeout
enforcement.

This closes the one remaining non-determinism in the completion contract: an
agent that hangs without producing outputs is bounded by the timeout and
routed to the state's timeout transition (or fails the task with a warning
when no timeout transition is declared).

Timeout is one conjunct of neural admission, never its monetary proof. The
finite provider-spend reservation of §FS-rhei-budgets.6 is required before the
same invocation starts. §REQ-bounded-neural-work.2

#### 3.2.3. Attempt Budget

One state **visit** — the span between two consecutive moves of the ticket
(§8.1) — may be spawned a bounded number of times. This is not `visits:`:
`visits:` counts how many times a ticket may **enter** the state; `attempts:`
counts how many times a single entry may be **spawned** before the run halts.
Entering the state again is a new visit, and a new visit brings a fresh
`attempts:` budget.

The budget can be set at two levels, resolved the way a timeout is (§7.1):

1. **Per-state** — `attempts` field on a state definition:
   ```yaml
   states:
     implement:
       attempts: 3
   ```

2. **Defaults** — `defaults.attempts` in settings:
   ```json
   { "defaults": { "attempts": 3 } }
   ```

Resolution: state-level > settings defaults > the built-in **2**.

Two is the initial invocation plus one informed retry. It is deliberate. A
retry that is told nothing repeats its predecessor exactly and buys nothing, so
the one retry is worth spending only because §3.2.1 makes it an informed one; a
third attempt that has learned nothing new since the second is spend without a
mechanism for converging. A machine whose recovery genuinely needs more sets
`attempts:` and says so where a reader will find it.

The budget is **persisted with the visit**, in the spawn record of §8.4, so it
holds across separate `rhei run` invocations. A budget that reset when the
operator ran `rhei run` again would bound nothing: the runaway case this exists
for is a state re-spawned once per run, forever.

An invocation the **run itself interrupted** does not spend the budget. The
shutdown ended it, no transition fired, and the next `rhei run` re-executes it
([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)); charging it would let two Ctrl-Cs halt a ticket that has not
yet had one attempt of its own. It still gets its own attempt log and its own
spawn record — a transcript cut short is worth as much as any other.

A visit whose **release edge the engine withheld** does not spend it either.
A supervising visit that released nothing exited `0` and met its completion
condition; [§FS-rhei-supervision.3.6](rhei-supervision.spec.md#36-empty-visits) takes its self-loop away rather than
the worker failing to earn it, so the attempt charged for it is given back and
the spawn record says the edge was withheld. The reasoning is the
interruption's — the engine ended the visit, not the worker — with one thing
more: a held supervisor bars its whole subtree, so a budget spent there would
not halt one ticket but leave the workspace recoverable only by `rhei reset`.

A budget below `1` is raised to `1`: every visit gets at least the invocation
that makes it a visit.

A **poll state** ([§FS-rhei-states.2](rhei-states.spec.md#2-polling-states)) is exempt from the budget. Re-spawning
without moving is what a poll state is for, and it already declares its own
bound in `poll.max_attempts`; a second bound over the same spawns would end the
loop earlier than the machine's author said it should. The re-spawn note of
§3.2.1 still names a budget on a poll state — `poll.max_attempts` itself, not
this exemption's internal encoding of "no budget applies here".

A recognized provider-limited invocation (§FS-rhei-run.3.3) also does not
spend the state visit's budget. It is a completed spawn with a retained log and
spawn record, but the provider refused to begin the requested work; repeated
recognized refusals therefore cannot exhaust `attempts:`. Poll counters and
`poll.max_attempts` remain separate and are neither advanced nor refunded by
provider-limit recognition.

Attempt refunds affect only this per-visit counter. Every confirmed or
ambiguous neural start permanently consumes its Panta invocation unit and
retains provider exposure until authoritative settlement; neither this refund
nor a new visit credits the outer ledger. §FS-rhei-budgets.5

### 3.3. Consumed Export Preflight

Prompt composition is all-or-nothing for declared consumed exports. Before a
worker is spawned, Rhei resolves every `**Consumes:**` file under that
producer's execution root and requires non-whitespace text, as specified by
[§FS-rhei-plan-language.3.12.2](rhei-plan-language.spec.md#3122-consumer-availability). A missing or blank export is not silently omitted: composition fails with one
diagnostic that names every unavailable producer/export pair and the path
checked for each.

The check uses the same root-confinement refusal and pre-qualification-id
rename guidance as state artifacts. It runs for same-rhei and cross-rhei
producers, including a producer outside the candidate set of a narrowed run.
Failure happens before spawn and does not change task state. Repairing the
files and rerunning composes the prompt normally.

## 4. Environment Variables

An autonomous agent subprocess must not receive the outer run's execution
identity through its process environment. Before spawning any configured agent
profile, Rhei removes these five variables even when its own parent environment
contains them:

| Removed variable | Authoritative agent surface |
|------------------|-----------------------------|
| `RHEI_ROOT` | The execution-root map in `## Rhei Commands` |
| `RHEI_PLAN_PATH` | The plan path in `## Rhei Commands` |
| `RHEI_RESULT_PATH` | `## Result`, on an invocation whose state can finish the task |
| `RHEI_TASK_ID` | The qualified task identity in `## Position` and the task heading |
| `RHEI_TASK_ID_LOCAL` | The task heading in the named rhei |

The composed prompt (§3) is authoritative for those values. A custom agent
wrapper must read the prompt instead of these legacy variables. Because an
agent's commands inherit its environment, removal also keeps an independently
launched nested Rhei from adopting the outer plan, task, artifact root, or
result path. This is an intentional compatibility break: neither task-id
spelling is retained as ambient compatibility state.

This isolation applies only to autonomous agent workers. Program states keep
their explicit environment contract ([§FS-rhei-programs.2](rhei-programs.spec.md#2-environment-variables)), and transition callbacks keep their own
callback context and environment contract. Rhei continues to add these
agent-transport and invocation-detail variables to the autonomous agent:

| Variable | Value |
|----------|-------|
| `RHEI_CHECKOUT_ROOT` | Absolute path to the checkout directory used as the agent subprocess working directory |
| `RHEI_WORKTREE_ROOT` | Absolute path to the task git worktree when the task is running from a worktree reference; unset otherwise |
| `RHEI_STATE` | Current state name |
| `RHEI_ATTEMPT` | Which attempt of this state visit the invocation is, counting from 1. `2` and above mean the invocation is a retry of an attempt that did not finish, and the prompt's `## Previous Visits` says how that one ended ([§FS-rhei-memory.3.3](rhei-memory.spec.md#33--previous-visits)). Distinct from `RHEI_VISIT_COUNT`, which counts entries into the state and not spawns within one ([§FS-rhei-agents.8.1](rhei-agents.spec.md#81-log-file-naming)) |
| `RHEI_MODEL` | Model profile id, if configured |
| `RHEI_MODEL_PROVIDER` | Resolved provider id, if configured |
| `RHEI_MODEL_NAME` | Resolved provider model name, if configured |
| `RHEI_AGENT` | Agent identifier, if configured |
| `RHEI_MCP_SERVERS` | Comma-separated list of resolved MCP server ids that started successfully. Empty when none are attached. |
| `RHEI_MCP_<NAME>_AVAILABLE` | `true` or `false` for each declared MCP server in the state's effective set. `<NAME>` is the server id uppercased with hyphens and spaces replaced by underscores. |
| `RHEI_SKILLS` | Comma-separated list of resolved skill ids enabled for this state. Empty when none are attached. |
| `RHEI_SKILL_<ID>_AVAILABLE` | `true` or `false` for each declared skill in the state's effective set. `<ID>` follows the same transformation as MCP names. |

The agent subprocess working directory is the **checkout root**. `rhei run`
resolves it in this order:

1. `runtime/worktree-refs/<task-id>.yaml` under the artifact root, when present.
   The file must contain an absolute `path:` pointing at an existing git
   worktree root; that path becomes both `RHEI_CHECKOUT_ROOT` and
   `RHEI_WORKTREE_ROOT`.
2. The enclosing git repository root for the artifact root, when `git -C
   <artifact-root> rev-parse --show-toplevel` succeeds.
3. The `rhei run` process current working directory when no git repository is
   found.

Artifact contracts, logs, accounting, snapshots, result files, and other Rhei
runtime files remain rooted at the artifact root named in the prompt; the
checkout root exists so agents discover repository-local instructions such as
`AGENTS.md` and, when a task uses an isolated git worktree, edit inside that
worktree. Because the two roots can differ, every path the prompt hands the
worker resolves by the one rule stated next.

### 4.1. Paths in the Prompt

A worker is told where every file goes; it never infers a base. **Every
filesystem path in the composed prompt is either absolute, or relative to the
artifact root**, which the execution-root map in `## Rhei Commands` names
absolutely. No path in the prompt is relative to the working directory the
`rhei run` process was launched from. That directory is not in the prompt, is
not the worker's own working directory, and is no longer discoverable from the
environment now that the identity variables above are removed — so a path
expressed against it is a path the worker cannot resolve at all.

`rhei run` therefore resolves the artifact root to an absolute path **before**
composing any prompt, so `rhei run ws` hands the worker exactly the paths
`rhei run /absolute/ws` hands it. A root that reached the prompt still relative
would send a worker following this rule to a doubled path under the artifact
root, and a worker resolving it against its own working directory into the
checkout — two wrong answers where the rule promises one right one. Resolving
makes the root absolute and nothing else — the prompt shows the spelling the
caller gave, not a symlink-resolved or long-name rewriting of it — while the
question the table below asks is settled by resolving both roots, so one
directory named two ways answers that it is one.

Resolved exclusion targets passed to `deny_read.path_flag` are the exception to
prompt display shortening: adapter arguments are always logical or canonical
absolute paths. Prompt and dry-run rendering may show their normalized authored
root-relative form alongside those targets.

**A path already written under the root's own spelling renders unchanged.** Not
re-derived, not re-joined, not re-separated: the characters the caller gave. A
path that is textually under the root already answers both questions the table
asks — its tail is the relative form and the path itself is the absolute one —
and every rewriting of it can only lose what was written. Joining `states.yaml`
back onto a root typed `C:/Users/x/ws` spells it `C:/Users/x/ws\states.yaml`, a
path that resolves and that nobody wrote, which is the complaint this rule
exists to answer. Only a path that reaches the root by a *different* spelling —
through a symlink, or an 8.3 short name — is matched by resolving both sides
and re-joined onto the root as given, because that path never carried the
root's own spelling to keep.

Given an absolute artifact root, a path renders one of two ways:

| Where the roots stand | How a path renders |
|---|---|
| The checkout root **is** the artifact root | Relative to the artifact root, which is also the worker's working directory, so both readings agree |
| The checkout root **differs** from the artifact root | Absolute, under the artifact root |

The rule covers every path the run composes for a worker: `{input.<name>.path}`
and `{output.<name>.path}`, the export path each `**Provides:**` name is given
under `## Exports to Publish`, the `## Result` path below, and the session
directory a snapshot-emitting invocation is handed through its profile's
`session_dir_flag`
([§FS-rhei-snapshots.9.1](rhei-snapshots.spec.md#91-customagentprofilesession)).
Every one of them renders by the table above; a prompt that mixes an absolute
result path with a bare relative export path has told the worker to infer a
base, which is the one thing this rule denies it.

This is what keeps a worker following artifact instructions from the checkout
cwd without writing runtime files into the checkout by accident — a transcript
written to an unresolvable session path lands in the checkout, where the run
does not look for it and reports the invocation as having produced none. An
export lost the same way is quieter still: the consuming task's prompt is
composed with no `## Consumed Exports` section at all, and the run reports
every task completed.

**A `## Result` path is present exactly where a result is owed.** The section
is composed for an invocation that a terminal edge leaves by name, and it names
that invocation's own file: the ticket's result, or under fan-out that
invocation's own fragment
([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)). Its absence
says this invocation owes no result — the same answer the completion condition
gives — rather than naming a path the prompt forgot. A worker that finds no
`## Result` section writes none; a worker that rebuilds the conventional path
anyway writes a file nothing reads.

## 5. `rhei run` — Agent Mode

### 5.1. CLI

```
rhei run <RHEI_PLAN> [--dry-run] [--no-callbacks] [--no-agent] [--no-program]
                     [--agent <AGENT>] [--model <MODEL>] [--continue-on-error]
                     [--parallel <N>] [--program-timeout <DURATION>]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--dry-run` | false | Show what would be spawned without executing |
| `--no-callbacks` | false | Skip `on_leave`/`on_enter` callbacks |
| `--no-agent` | false | Disable agent spawning; fall back to callback-only advancement (pre-agent behavior) |
| `--no-program` | false | Disable program spawning; fall back to callback-only advancement for program states |
| `--agent <AGENT>` | | Override the agent transport for this run |
| `--model <MODEL>` | | Override the model profile id for this run |
| `--continue-on-error` | false | Continue to the next task when an agent or program exits non-zero |
| `--parallel <N>` | 1 | Maximum number of agents/programs to run concurrently. `0` means unlimited. |
| `--program-timeout <DURATION>` | | Override program timeout for this run (e.g., `10m`, `1h`). See [Program States Specification](rhei-programs.spec.md#4-timeout-handling). |

### 5.2. Execution Loop

#### 5.2.1. Sequential Mode (default, `--parallel 1`)

1. Load plan and state machine. Validate.
2. Find the next claimable task (same eligibility as `rhei next`).
3. Resolve the model and, if agent mode is enabled, the agent for the task's current state (resolution order above).
4. If agent mode is enabled and no agent is configured, fail with an error.
5. Resolve exclusions again, compose the filtered prompt, and construct any
   declared read-denial adapter arguments (see [Prompt Composition](#3-prompt-composition)).
6. Log the spawn to `runtime/logs/task-{task_id}-{state}[-{visit_count}].log`.
7. Spawn the agent CLI as a subprocess with the composed prompt.
8. Wait for the agent process to exit (subject to timeout — see [Timeout Handling](#7-timeout-handling)).
9. Re-read the plan. If some external actor changed the task's state while the agent was running, respect that authoritative plan state and continue the loop from there.
10. Otherwise, if the agent exited `0`, evaluate the current state's declared forward transitions in normal transition-selection order. If one transition matches, `rhei run` executes it and logs the resulting state change.
11. If the agent exited `0` but the state's completion condition fails — a
    required `outputs:` artifact missing, or a terminal-landing edge with no
    result ([§FS-rhei-states.3.3](rhei-states.spec.md#33-terminal-result)) — or if no forward transition matches, no
    transition fires and the task stays in its state. Log the warning (the
    missing-artifact form of [Runtime Semantics](#321-runtime-semantics) when
    artifacts are missing, `warning: agent exited 0 but task {id} did not
    advance from '{state}'` otherwise), mark the task stalled for the rest of
    the pass so it is not spawned again, and **continue to the next claimable
    task**. Sequential mode does not end the run on one task's stall any more
    than the worker pool does ([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop) step 5); `--continue-on-error`
    governs non-zero exits and does not enter into it.
12. If the agent exited non-zero:
    - If the result is a recognized provider limit, park it under
      §FS-rhei-run.3.3 and continue the scheduling loop. This path fires no
      transition or callback and is the same with or without
      `--continue-on-error`.
    - Without `--continue-on-error`: log the error and stop.
    - With `--continue-on-error`: log the error, skip this task, continue.
13. Repeat until every claimable task has advanced or stalled. A pass ends when
    no unstalled claimable task remains; the run ends when a whole pass makes no
    progress, or when all tasks are terminal.

#### 5.2.2. Parallel Mode (`--parallel N` where N > 1 or N = 0)

1. Load plan and state machine. Validate.
2. Find all claimable tasks (same eligibility as `rhei next`, but collect all candidates).
3. Select up to N tasks that are mutually independent (no dependency edges between them). When N = 0, select all independent claimable tasks.
4. For each selected task, resolve the same exclusion policy used by sequential
   mode, compose the filtered prompt, and spawn the agent subprocess
   concurrently. The check is independent of whether the excluded source
   existed before pool selection. Each agent writes to its own log file and is
   treated as a worker for the task's current state, not as a transition
   authority. Retries and each fan-out invocation repeat the same resolution
   immediately before spawn.
5. Wait for any agent to exit (timeout or completion).
6. When an agent exits:
   a. Re-read the plan.
   b. Process the result using the same rules as sequential mode: if an external actor already changed the task state, respect it; otherwise, on exit `0`, let `rhei run` evaluate and execute the next matching forward transition; on non-zero exit, apply the error path.
   c. Scan for newly claimable tasks (dependencies may have been unblocked).
   d. If new tasks are claimable and the pool is below N, spawn agents for them.
   A recognized provider limit releases the reporting slot immediately. The
   refill scan excludes queued invocations with the same active provider-limit
   identity, but may fill the slot with unrelated work. Workers already
   running under that identity are not stopped and each result is classified
   independently.
7. Repeat until no claimable tasks remain or all tasks are terminal.

**Independence rule:** Two tasks are independent when neither appears in the other's transitive `**Prior:**` chain. The engine must not spawn two agents that could produce conflicting edits to the same task file. For directory workspaces, each task lives in a separate file, so file conflicts are avoided by construction.

**Single-file plans:** Parallel mode is limited to `--parallel 1` for single-file plans because agents could produce conflicting edits to the same file. `rhei run` prints a warning if `--parallel` > 1 is requested with a single-file plan and falls back to sequential execution.

### 5.3. Interaction Between Agents and Callbacks

Agents and callbacks are complementary, not exclusive:

- **Agent** does the work of the current state (coding, reviewing, fixing, writing artifacts).
- **`rhei run`** evaluates success or failure and performs the state transition.
- **Callbacks** handle side effects of that transition (creating artifacts, spawning tasks, notifying systems).

When `rhei run` is in agent mode:
1. The agent is spawned for the current state.
2. The agent performs work and exits.
3. `rhei run` selects and executes the transition.
4. `on_leave` / `on_enter` callbacks fire as part of that engine-driven transition.

`--no-callbacks` suppresses callbacks but not agent or program spawning. `--no-agent` suppresses agent spawning but not program spawning or callbacks. `--no-program` suppresses program spawning but not agent spawning or callbacks. All three can be combined independently.

### 5.4. Gating States

When a task reaches a gating state (`gating: true`), `rhei run` does not spawn an agent. Instead it logs:

```
Task {id} is in gating state '{state}'. Waiting for human action.
```

The task is skipped and the engine continues with other claimable tasks. When the human transitions the task out of the gating state (via `rhei transition`), the next run pass picks it up.

## 6. Missing Tooling

Tooling — MCP servers and skills — can fail to be available for reasons that
only surface at spawn time: a binary is not on `PATH`, a remote MCP URL is
unreachable, an env var is unset, or a skill path does not exist. Rhei
distinguishes three failure classes:

| Failure | When detected | Default behavior |
|---------|---------------|------------------|
| Id referenced in a state but not in the merged registry and not an inline entry | `rhei validate` / settings load | Hard error. Same class as a dangling `model:` or `agent:` reference. |
| Registry entry exists but the MCP server cannot start, URL is unreachable, the handshake times out, or a skill path does not exist | Agent spawn | Depends on the entry's `optional` flag (see below). |
| MCP server starts but crashes mid-session | During agent execution | The agent surfaces the protocol error; Rhei does not intervene. The failure appears in the agent log. |

An MCP server or skill is considered **available** when:

- For `command`-based MCP servers: the subprocess launched, the MCP handshake
  completed, and the server remained alive, all within its `startup_timeout`
  (default `10s`).
- For `url`-based MCP servers: the transport-level connection completed and
  the MCP handshake succeeded within `startup_timeout`.
- For skills: the configured `path` exists and is readable.

### 6.1. Required vs optional entries

Per-state `mcp_servers` and `skills` entries may be declared required (the
default) or `optional: true`. The `defaults` lists in `settings.json` follow
the same rules.

- **Required (default):** if the entry fails its availability check,
  `rhei run` does not spawn the agent. The engine looks for an
  `mcp_unavailable` or `skill_unavailable` transition from the current state
  in `states.yaml`:
  - If one is declared, it fires with `triggeredBy: 'system'`. The callback
    receives the unavailable ids in `transitionData.unavailable`.
  - Otherwise the task stays in its current state and the engine logs
    `error: required tooling unavailable for task {id} in state '{state}':
    <id1>, <id2>`. `--continue-on-error` behaves the same way it does for
    agent exits: the engine logs and skips the task.
- **Optional (`optional: true`):** if the entry fails, `rhei run` logs a
  warning and spawns the agent with the remaining resolved tooling. The
  prompt's template variables and env vars reflect availability so
  instructions and callbacks can branch. Missing optional skills are always
  non-fatal and emit the same warning regardless of agent support.

An unsupported agent (one whose profile declares no `mcp_flag` /
`mcp_config_flag` for a required MCP entry, or no `skill_flag` for a required
skill entry) is treated identically to an availability failure: required
entries escalate to the `*_unavailable` path; optional entries produce a
warning and are dropped.

See [Transitions Specification](rhei-transitions.spec.md) for declaring
`mcp_unavailable` / `skill_unavailable` transitions and
[States Specification — Template Variables](rhei-states.spec.md#4-template-variables-in-instructions-and-personality)
for `{mcp.<name>.available}` and `{skill.<id>.available}` in prompts.

## 7. Timeout Handling

### 7.1. Configuration

Timeout can be set at four levels:

1. **Per-state** — `agent_timeout` field on a state definition:
   ```yaml
   states:
     pending:
       agent_timeout: 30m
   ```

2. **Per-model/agent binding** — `timeout` field in a model profile's `agents` binding:
   ```json
   {
     "models": {
       "impl-fast": {
         "provider": "anthropic",
         "model": "claude-sonnet-4-6",
         "agents": {
           "claude-code": { "timeout": "45m" }
         }
       }
     }
   }
   ```

3. **Per-agent profile** — `timeout` field on an entry in the `agents`
   registry:
   ```json
   {
     "agents": {
       "my-agent": { "command": ["my-agent"], "timeout": "1h" }
     }
   }
   ```

4. **Defaults** — `defaults.agent_timeout` in settings:
   ```json
   { "defaults": { "agent_timeout": "30m" } }
   ```

Resolution: state-level > model-agent binding > agent-profile > settings defaults.

Under `orchestrator` [Completion Authority](#31-completion-authority), a timeout
must resolve to a finite value at some level of the chain; missing timeouts on
orchestrator-driven states are a validation error. Under `worker` authority
the resolution is optional and the engine does not impose a timeout on manual
work.

### 7.2. Duration Format

Durations use a human-readable format: `30s`, `5m`, `1h`, `2h30m`. Supported units:

| Unit | Suffix | Example |
|------|--------|---------|
| Seconds | `s` | `30s` |
| Minutes | `m` | `5m` |
| Hours | `h` | `1h` |

Units can be combined: `1h30m`, `2h15m30s`.

### 7.3. Timeout Behavior

When an agent process exceeds its timeout:

1. `rhei run` sends `SIGTERM` to the agent's **process group**, not to the
   direct child alone, so the agent's own subprocesses — MCP servers, shell
   tools, background jobs — go with it ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)).
2. After a 10-second grace period, if the group has not exited, send `SIGKILL`
   to the group.
3. Log to the task log: `agent timed out after {duration}`.
4. Look for a timeout transition from the current state in the state machine.
5. If a timeout transition exists, fire it (with its `on_leave`/`on_enter` callbacks).
6. If no timeout transition exists, the task remains in its current state and the engine logs a warning.

When snapshot capture is enabled, a timeout may still produce an auto-emitted
transcript for operator inspection, but that snapshot is classified
`completion: timeout` and is not preloadable by authored `snapshot.inherit:`.
See [Snapshots Specification — Compatibility Predicates](rhei-snapshots.spec.md#5-compatibility-predicates).

### 7.4. Timeout Transitions

Timeout transitions are declared in the `transitions` array with the `timeout` field. The existing `timeout` field in the transition schema (see [Transitions Specification](rhei-transitions.spec.md#44-transition-definition)) is used by `rhei run` to determine what to do when an agent times out.

When a task is being worked by an agent and the agent exceeds the state's timeout:

1. The engine kills the agent process.
2. The engine evaluates timeout transitions from the current state.
3. The first matching timeout transition fires.

The `triggeredBy` field on these transitions is set to `'system'`.

Example:

```yaml
states:
  pending:
    description: Task is ready for implementation.
    model: impl-fast
    agent_timeout: 30m

  timed-out:
    description: Agent failed to complete within the time budget.
    gating: true
    instructions: |
      The agent timed out while working on this task. A human must decide:
      - Return to `pending` for another attempt
      - Cancel the task
      - Increase the timeout and retry

transitions:
  - from: pending
    to: timed-out
    description: Agent exceeded the time budget
    timeout: 30m
    on_enter: "cli:bash ./workflow.sh notify-timeout"

  - from: timed-out
    to: pending
    description: Human decided to retry

  - from: timed-out
    to: cancelled
    description: Human decided to abandon after timeout
```

The `timeout` field on the transition and `agent_timeout` on the state serve different roles:

| Field | Where | Purpose |
|-------|-------|---------|
| `agent_timeout` | State definition | How long the engine waits before killing the agent process |
| `timeout` | Transition definition | Which transition fires when a timeout occurs (existing field, now also used by agent mode) |

When `agent_timeout` is set on a state but no transition with `timeout` exists from that state, the agent is killed but the task remains in its current state with a warning logged.

### 7.5. Timeout Callbacks

Timeout transitions support the same `on_leave` and `on_enter` callbacks as any other transition. This enables notification, logging, or cleanup on timeout:

```yaml
transitions:
  - from: pending
    to: timed-out
    description: Agent exceeded time budget
    timeout: 30m
    on_leave: "cli:bash ./workflow.sh save-partial-work"
    on_enter: "cli:bash ./workflow.sh notify-timeout"
```

The callback receives a `TransitionContext` with `triggeredBy: 'system'` and the timeout duration in `transitionData.timeout`.

In serial and parallel runs, `on_leave` retains the timed-out agent's effective identity,
including independent CLI overrides (§FS-rhei-agents.1.4, §FS-rhei-agents.1.5).

## 8. Log Capture

All agent stdout and stderr are captured to log files in the `runtime/logs/` directory relative to the workspace or plan root.

Token and cost accounting is captured separately under `runtime/accounting/`.
Agent logs remain human-readable transcripts; accounting extractors must use
structured usage sources where the agent provides them and must not depend on
parsing the log body for billing facts. [§FS-rhei-cost-accounting](rhei-cost-accounting.spec.md#fs-rhei-cost-accounting-rhei-cost-accounting)

### 8.1. Log File Naming

| Scenario | Log file path |
|----------|---------------|
| Simple state | `runtime/logs/task-{task_id}-{state}.log` |
| Counted-loop state | `runtime/logs/task-{task_id}-{state}-{visit_count}.log` |
| Model-specific state | `runtime/logs/task-{task_id}-{state}-{model}.log` |
| Both visits and model | `runtime/logs/task-{task_id}-{state}-{model}-{visit_count}.log` |
| Retry within one visit | the name above with `-attempt{n}` appended, `n` counting from 2 |

`{visit_count}` counts only where [§FS-rhei-transitions.4.3](rhei-transitions.spec.md#43-counted-loops) keeps a counter, so
it cannot separate one stay in a state from the next: an ordinary state in a
cycle is `{visit_count}` 1 on every entry. The attempt suffix is therefore keyed
to a **state visit**, which this specification defines as the span between two
consecutive moves of the ticket. The first spawn of a visit uses the unsuffixed
name above; each further spawn *within that same visit* is `-attempt2`,
`-attempt3`, and so on, and the first spawn after the ticket moves again starts
over at the unsuffixed name — a fresh entry into a state is not a retry of the
last one, and must not be named or narrated as one. Without the suffix a retry
would truncate the one file that says why the attempt before it did not finish,
which is the file both a human and the retrying agent need most.

A visit's attempt number is read from the spawn record of §8.4, never inferred
from which log files happen to exist: a log is opened before its subprocess
starts, so its presence proves only that a spawn was attempted.

Where something names "the log of a visit" — the `Previous log:` line of a
prompt ([§FS-rhei-memory.4.4](rhei-memory.spec.md#44-previous-visits)), the evidence behind an engine-written result
([§FS-rhei-run.3](rhei-run.spec.md#3-execution-loop)) — it means the log named by that visit's spawn record, which is
the last thing that actually ran there.

### 8.2. Log Format

Each log file contains:

```
=== rhei agent log v1 ===
agent: claude-code
model: impl-fast
provider: anthropic
model_name: claude-sonnet-4-6
task: 3
state: pending
started: 2026-04-20T10:30:00Z
timeout: 30m
plan: /home/user/project/plan.rhei.md
mcp_servers: postgres,grafana?
skills: test-authoring
===

<human-readable agent output and stderr, interleaved; a built-in structured
result envelope is represented by its result text>

=== exit ===
code: 0
duration: 4m23s
ended: 2026-04-20T10:34:23Z
===
```

The header and footer are added by `rhei run`. The `v1` suffix is the log format version — increment it when the header/footer structure changes. The body is the raw, unmodified output of the agent process except that a built-in structured result envelope is represented by its human-readable `result` text.

An invocation the engine ended early says so in the footer, above `=== exit ===`
and again as a machine-readable flag inside it:

| Ending | Line above `=== exit ===` | Flag inside `=== exit ===` |
| --- | --- | --- |
| Timeout (§7.3) | `agent timed out after {duration}` | `timed_out: true` |
| Run interrupted ([§FS-rhei-run.3.2](rhei-run.spec.md#32-interruption-and-process-ownership)) | `agent interrupted by run shutdown after {duration}` | `interrupted: true` |
| Provider limited (§FS-rhei-run.3.3) | `agent provider-limited until {nextAttemptAt}` | `provider_limited: true` |

A program log (`=== rhei program log v1 ===`) carries the same two lines with
`program` in place of `agent`. Neither flag appears on a normal exit, so a
reader — and a script — can tell an agent that finished from one the engine
stopped without inferring it from the exit code.

Each entry in `mcp_servers:` and `skills:` is the resolved id; an entry
suffixed with `?` was declared `optional: true` and failed its availability
check — it was dropped before spawn and is recorded for diagnostics. A
missing line means the state declared no entries of that kind.

### 8.3. Log Directory

`runtime/logs/` is created automatically by `rhei run` if it does not exist. `rhei reset` removes the entire `runtime/` directory, including logs.

### 8.4. Spawn Records

A log answers *what a worker said*. Two other questions are asked of a finished
spawn — *did a worker actually run in this state* and *is the next spawn a retry
of the same visit* — and neither can be answered from `runtime/logs/`. The log
file is created and its header written before the subprocess starts, so a
`command:` naming a binary that does not exist leaves a complete-looking header
behind; and the log's own name cannot say which visit it belongs to, because
`{visit_count}` does not count every visit (§8.1).

So `rhei run` writes one **spawn record** per invocation, in
`runtime/spawns/`, named after the invocation exactly as its log is minus the
`-attempt{n}` suffix: `task-{task_id}-{state}{suffix}.json`. It is written when
the subprocess **ends**, never when it is merely opened, and it is rewritten in
place by each further attempt of the same invocation. It holds:

| Field | Meaning |
|-------|---------|
| `task`, `state` | the invocation this record answers for, spelled out |
| `moves` | how many times the ticket had moved when this spawn started — the visit key of §8.1 |
| `attempt` | which attempt of that visit this was, counting from 1 |
| `kind`, `worker` | `agent` or `program`, and the resolved agent id or command |
| `log` | the transcript this spawn wrote |
| `started`, `ended`, `duration`, `code` | when it ran, for how long, and how it exited |
| `ending` | `exited`, `timed out`, `interrupted`, `provider_limited`, or `withheld` — why it stopped or remained in its state |
| `attempt_charged` | whether this invocation consumed the state visit's `attempts:` budget |

A provider-limited record keeps the subprocess's real non-zero `code`, uses
`ending: provider_limited`, and sets `attempt_charged: false`. Its log, timing,
resolved worker identity, and any usage the worker reported remain ordinary
auditable spawn evidence.

The engine changes a successful `exited` record to `ending: withheld` only when
the subprocess exited with code `0`, the invocation met its completion condition,
and the engine withheld its release edge without moving the ticket
(§FS-rhei-supervision.3.6). The record retains code `0` and the same visit and
invocation identity. This ending cannot turn an unsuccessful invocation into
successful work.

For the pre-spawn completion check (§3.2), a record proves successful work for
one invocation only when its `task`, canonical `state`, and invocation-specific
filename identify that invocation, its `moves` equals the ticket's current
transition-ledger move count, its `ending` is `exited` or `withheld`, and its
`code` is `0`. A withheld record supplies proof only for that same visit and
invocation; it cannot excuse work after a later move.
Older visits and records ending in failure, timeout, interruption, or provider
limiting remain useful history but are not successful current-visit proof. The
existing per-invocation filename keeps fan-out identities independent; this
rule adds no persisted field or format.

For neural work the spawn record also carries the budget reservation id,
qualification tuple/evidence hash, ancestor reservation, and the matching
§FS-rhei-cost-accounting.3.7 attempt identity. The authoritative financial
record remains §FS-rhei-budgets.3's journal; the spawn record is its diagnostic
correlation and cannot release exposure.

`task` and `state` are stored as fields and matched as fields. A reader looking
for "a worker that ran in state `review`" must not match record *file names* by
prefix: state names share prefixes — `agent-review` and `agent-review-fix` ship
in one profile in [§FS-rhei-states.8](rhei-states.spec.md#8-profiles) — and a prefix match would attribute one
state's transcript, worker, and duration to its neighbour.

`runtime/spawns/` is created on demand and, like `runtime/logs/`, is removed by
`rhei reset`.

## 9. Dry-Run Output

`rhei run --dry-run` in agent mode shows what would be spawned without executing:

```
Pass 1: 2 ready, 0 terminal, 5 total.

Would spawn: claude --output-format json -p --model claude-sonnet-4-6 --
  Task app.1: Set up database schema [draft -> pending]
  Agent: claude-code, Model: impl-fast (anthropic/claude-sonnet-4-6), Timeout: 30m
  Log: runtime/logs/task-app.1-pending.log
  Exclusions: checkout=notes/private.md
  Enforcement: composition only; paths remain readable outside Rhei-composed context

Would spawn: gemini --prompt "<prompt...>" --model gemini-3-pro
  Task app.3: Write frontend components [draft -> pending]
  Agent: gemini, Model: impl-alt (google/gemini-3-pro), Timeout: 30m
  Log: runtime/logs/task-app.3-pending.log

Dry run complete - no agents were spawned.
```

The prompt is never printed in full. An agent that carries it in `argv` shows
the `<prompt...>` placeholder where it would sit; an agent that delivers it on
stdin has no prompt on its command line at all, which is why the `claude-code`
line above ends at the separator.
Dry-run output lists normalized exclusions and the same capability-derived
enforcement label as the prompt. For an adapter profile, the displayed command
also contains every repeated denial flag and absolute target before the
separator. Dry run does not ask the wrapper to enforce them.

## 10. `rhei run --no-agent` — Callback-Only Mode

When `--no-agent` is passed, `rhei run` reverts to pre-agent behavior: it advances tasks through the state machine using transition callbacks only, without spawning any agent processes. This is the existing behavior for backward compatibility.

## Related Specifications

- [States Specification](rhei-states.spec.md) — state machine format, `model`/`agent` fields, template variables
- [Program States Specification](rhei-programs.spec.md) — deterministic program execution (the algorithmic complement to agent states)
- [Transitions Specification](rhei-transitions.spec.md) — transition callbacks, timeout transitions, exit-code transitions
- [How Rhei Is Used](rhei-usage.spec.md) — roles, coordination patterns, agent workflows
- [Next Command](rhei-next.spec.md) — `rhei next` behavioral contract
- [Complete Command](rhei-complete.spec.md) — `rhei complete` behavioral contract
- [Install Skills](rhei-install-skills.spec.md) — `rhei install-skills` for agent integration
