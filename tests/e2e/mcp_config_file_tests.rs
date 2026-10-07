//! The file the built-in `claude-code` agent is handed with `--mcp-config`, read
//! from outside. agent-grounds/rhei#476: rhei copied each registry entry into it
//! field for field, so `command` stayed an array and `transport` stood where
//! Claude Code reads `type`. Claude Code then skipped every declared server as
//! invalid config, while `RHEI_MCP_SERVERS` named them all.
//!
//! The built-in profile runs `claude` from `PATH`, so a compiled stand-in takes
//! that name (§REQ-cross-platform.4 keeps a shell script out of a fixture) and
//! keeps the file. Claude Code itself is not on CI. What is pinned is the shape,
//! which Claude Code accepts or refuses entry by entry before it launches
//! anything.
// §FS-rhei-mcp-config-file

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const PLAN: &str = "# Rhei: MCP Config File

## Tasks

### Task 1: Use the declared servers
**State:** work
";

fn machine(servers: &str) -> String {
    format!(
        r#"name: mcp-config-file
version: 1
states:
  work:
    initial: true
    description: Use the declared servers
    agent: claude-code
    agent_timeout: 30s
    mcp_servers: [{servers}]
    instructions: Use the declared servers.
  completed:
    final: true
    description: Done
transitions:
  - from: work
    to: completed
"#
    )
}

/// One `rhei run` of the plan on the built-in `claude-code` profile, with
/// `settings` as the project settings. Returns the run and the `--mcp-config`
/// file the agent was handed, parsed.
fn run_on_claude_code(
    name: &str,
    settings: serde_json::Value,
    servers: &str,
) -> (CliRun, serde_json::Value) {
    let dir = unique_temp_dir(name);
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("create stand-in directory");
    fs::copy(
        PathBuf::from(env!("CARGO_BIN_EXE_mcp-config-fixture")),
        bin.join(format!("claude{}", std::env::consts::EXE_SUFFIX)),
    )
    .expect("stage the stand-in as claude");
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    write_fixture_file(&settings_dir, "settings.json", &settings.to_string());
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let states = write_fixture_file(&dir, "states.yaml", &machine(servers));
    let copy = dir.join("mcp-config.json");

    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let path: OsString = std::env::join_paths(paths).expect("join stand-in PATH");
    let output = rhei_command(dir.join("home"))
        .env("PATH", path)
        .env("RHEI_E2E_MCP_CONFIG_COPY", &copy)
        .env("RHEI_E2E_MCP_TOKEN", "expanded-token")
        .arg("--state-machine")
        .arg(&states)
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-callbacks"])
        .output()
        .expect("run rhei");
    let run = CliRun::from(&output);
    assert_success(&run);
    (run, read_handed_file(&copy))
}

fn read_handed_file(copy: &Path) -> serde_json::Value {
    let text = fs::read_to_string(copy).unwrap_or_else(|err| {
        panic!("rhei handed claude-code no --mcp-config file ({}: {err})", copy.display())
    });
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("--mcp-config is not JSON: {err}"))
}

/// Every server the state declares reaches the file in the schema Claude Code
/// reads: a local one as a string `command` plus `args`, with its `env`
/// expanded, and a remote one as a `url` with a `type`.
// §FS-rhei-mcp-config-file.1
#[test]
#[ignore = "red until #476 writes the --mcp-config file in the agent's schema"]
fn a_claude_code_agent_is_handed_each_declared_server_in_the_mcp_config_schema() {
    let (_, handed) = run_on_claude_code(
        "mcp-config-schema",
        serde_json::json!({
            "mcp_servers": {
                "declared": {
                    "command": ["mcp-declared", "serve", "--log", "launches.log"],
                    "env": { "TOKEN": "${RHEI_E2E_MCP_TOKEN}" }
                },
                "events": { "url": "http://127.0.0.1:9/sse", "transport": "sse" },
                "stream": { "url": "ws://127.0.0.1:9/ws", "transport": "websocket" }
            }
        }),
        "declared, events, stream",
    );

    assert_eq!(
        handed,
        serde_json::json!({
            "mcpServers": {
                "declared": {
                    "command": "mcp-declared",
                    "args": ["serve", "--log", "launches.log"],
                    "env": { "TOKEN": "expanded-token" }
                },
                "events": { "url": "http://127.0.0.1:9/sse", "type": "sse" },
                "stream": { "url": "ws://127.0.0.1:9/ws", "type": "ws" }
            }
        }),
        "the --mcp-config file claude-code was handed is not in its schema"
    );
}

/// Claude Code takes a `cwd` or `workingDirectory` key and ignores both, so the
/// file carries no working directory, and the run says so rather than letting
/// the server start somewhere its entry did not ask for.
// §FS-rhei-mcp-config-file.2
#[test]
#[ignore = "red until #476 writes the --mcp-config file in the agent's schema"]
fn a_working_directory_the_mcp_config_file_cannot_carry_is_warned_about() {
    let (run, handed) = run_on_claude_code(
        "mcp-config-working-directory",
        serde_json::json!({
            "mcp_servers": {
                "declared": { "command": ["mcp-declared"], "working_directory": "server-home" }
            }
        }),
        "declared",
    );

    assert!(
        run.stderr
            .lines()
            .any(|line| line.contains("declared") && line.contains("working_directory")),
        "no warning names server 'declared' and the working_directory it cannot be given; \
         stderr:\n{}",
        run.stderr
    );
    assert_eq!(
        handed,
        serde_json::json!({ "mcpServers": { "declared": { "command": "mcp-declared", "args": [] } } }),
        "the --mcp-config file carries no working-directory key Claude Code would ignore"
    );
}
