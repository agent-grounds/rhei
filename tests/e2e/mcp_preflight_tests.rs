//! An MCP server that cannot be reached is unavailable before the agent starts.
//! Rhei used to count any server with a definition as available, so a required
//! server whose binary was missing still reported `true`, the agent started
//! without it, and `mcp_unavailable` never fired.
//!
//! The runs use the built-in `claude-code` profile, whose `claude` is the
//! compiled stand-in that keeps the `--mcp-config` file it is handed
//! (§REQ-cross-platform.4). No server is ever started; what is pinned is what
//! the spawn preflight lets through.
// §FS-rhei-agents.6.2

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use super::*;

const PLAN: &str = "# Rhei: Preflight

## Tasks

### Task 1: Read the dashboards
**State:** work
";

/// `work` attaches `servers`; a server the preflight refuses routes the ticket
/// to `blocked` through `mcp_unavailable: [lookup]`.
fn machine(servers: &str) -> String {
    format!(
        "name: preflight
version: 1
states:
  work:
    initial: true
    agent: claude-code
    agent_timeout: 30s
    mcp_servers: {servers}
    instructions: \"lookup available: {{mcp.lookup.available}}\"
  blocked:
    gating: true
  completed:
    final: true
transitions:
  - from: work
    to: completed
  - from: work
    to: blocked
    mcp_unavailable: [lookup]
  - from: blocked
    to: work
"
    )
}

/// One `rhei run` with `lookup` registered as `lookup`, and the stand-in staged
/// as `claude` ahead of the real one. Returns the run, the plan as it was left,
/// and the `--mcp-config` file the agent was handed, if any.
fn run(name: &str, lookup: serde_json::Value, servers: &str) -> (CliRun, String, Option<String>) {
    let dir = unique_temp_dir(name);
    let bin = dir.join("bin");
    stage_mcp_config_fixture(&bin, &["claude"]);
    let settings_dir = dir.join(".agent-grounds/rhei");
    fs::create_dir_all(&settings_dir).expect("create settings directory");
    let settings = serde_json::json!({ "mcp_servers": { "lookup": lookup } });
    write_fixture_file(&settings_dir, "settings.json", &settings.to_string());
    let plan = write_fixture_file(&dir, "plan.rhei.md", PLAN);
    let states = write_fixture_file(&dir, "states.yaml", &machine(servers));
    let copy: PathBuf = dir.join("mcp-config.json");

    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    let path: OsString = std::env::join_paths(paths).expect("join stand-in PATH");
    let output = rhei_command(dir.join("home"))
        .env("PATH", path)
        .env("RHEI_E2E_MCP_CONFIG_COPY", &copy)
        .arg("--state-machine")
        .arg(&states)
        .arg("run")
        .arg(&plan)
        .args(["--no-tui", "--no-callbacks"])
        .output()
        .expect("run rhei");
    let left = fs::read_to_string(&plan).expect("read the plan back");
    (CliRun::from(&output), left, fs::read_to_string(&copy).ok())
}

/// A required server whose executable is on no `PATH` blocks the spawn, and the
/// machine's `mcp_unavailable` transition routes the ticket.
#[test]
fn a_required_server_with_no_executable_fires_mcp_unavailable() {
    let (run, plan, handed) = run(
        "mcp-preflight-required",
        serde_json::json!({ "command": ["rhei-e2e-no-such-mcp-server"] }),
        "[lookup]",
    );
    assert_success(&run);
    assert!(
        run.stdout.contains(
            "Tooling-unavailable transition: Task plan.1 'work' -> 'blocked' (mcp unavailable: lookup)"
        ),
        "the preflight's refusal must fire `mcp_unavailable`; stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(plan.contains("**State:** blocked"), "{plan}");
    assert_eq!(handed, None, "no agent started, so no file was handed over");
}

/// An optional server whose host accepts no connection is dropped with the
/// reason, and the agent starts without it.
#[test]
fn an_optional_server_nothing_listens_for_is_dropped_with_its_reason() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    let (run, plan, handed) = run(
        "mcp-preflight-optional",
        serde_json::json!({
            "url": format!("http://127.0.0.1:{port}/sse"),
            "transport": "sse",
            "startup_timeout": "1s"
        }),
        "[{id: lookup, optional: true}]",
    );
    assert_success(&run);
    let said = format!("{}{}", run.stdout, run.stderr);
    assert!(
        said.contains(&format!(
            "warning: optional mcp 'lookup' unavailable (127.0.0.1:{port} accepted no connection within 1s); dropping"
        )),
        "the warning must name the preflight's reason; got:\n{said}"
    );
    assert!(plan.contains("**State:** completed"), "the agent ran without it: {plan}");
    assert_eq!(handed, None, "a dropped server reaches no --mcp-config file");
}
