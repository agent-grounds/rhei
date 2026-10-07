# FS-rhei-mcp-config-file: The MCP Config File an Agent Is Handed

An agent profile that declares `mcp_config_flag` (§FS-rhei-agents.1.1.2) is
handed the MCP servers its state resolved as one JSON file, passed once with
that flag. This point says what the file contains. The `mcp_servers` registry
(§FS-rhei-agents.1.1.4) is agent-neutral, and the same entry also feeds an
`mcp_flag` agent, so the file is not a copy of it. Each entry is written in the
schema Claude Code reads with `--mcp-config`, which is also the common
`.mcp.json` shape, whichever profile carries the flag.

The file is `{ "mcpServers": { "<id>": { … } } }`, one key per server whose
definition resolved, from the registry or inline in the state, and nothing
else. An entry in any other shape is refused before the agent starts anything:
Claude Code skips a `command` array as `invalid_config` and a `url` without a
`type` as `url_missing_type`. Its agent then runs without the server, while
`RHEI_MCP_SERVERS` and `{mcp.<id>.available}` (§FS-rhei-agents.4) report it
attached.

## 1. Entries

A `command` entry is written as:

| Key | Value |
|-----|-------|
| `command` | The first element of the registry's `command`, as a string. |
| `args` | The remaining elements, as an array. Written even when it is empty. |
| `env` | The entry's `env`, each `${VAR}` expanded against Rhei's own environment (§FS-rhei-agents.1.1.4). Omitted when the entry declares none. |

No `type` is written for it: an entry without one is a local (`stdio`) server.

A `url` entry is written as:

| Key | Value |
|-----|-------|
| `url` | The registry's `url`, unchanged. |
| `type` | The registry's `transport`: `sse` is written as `sse` and `websocket` as `ws`. A value outside those two is written unchanged, for the agent to accept or refuse. |

Nothing else is written. That includes `transport` under its own name,
`working_directory` (§2), `startup_timeout`, and the fields the registry gives
no meaning for the entry's kind: `env` on a `url` entry, and `transport` on a
`command` entry.

## 2. A Working Directory the File Cannot Carry

Claude Code accepts a `cwd` or a `workingDirectory` key on a local server and
starts the server in its own working directory regardless, so no key carries
`working_directory` and Rhei writes none. When a `command` entry written to the
file declares `working_directory`, the spawn warns instead, naming the server
id: the file cannot carry `working_directory`, so the server starts in the
agent's working directory, the checkout root (§FS-rhei-agents.4). The server is
still attached and still reported available; the warning is the only
difference.
