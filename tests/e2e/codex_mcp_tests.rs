//! Native Codex attachment, with the ticket's no-server control and real runs.
//! §FS-rhei-agents.1.1.2.1

use super::codex_mcp_support::*;
use super::*;
use serde_json::json;

#[test]
fn codex_mcp_required_command_uses_native_configuration() {
    let settings = json!({"mcp_servers": {"lookup": command_server()}});
    let control = run_case("control", settings.clone(), "    mcp_servers: []\n", "codex");
    let record = assert_completed(&control, "");
    assert!(record["config"].get("mcp_servers").is_none());
    if let Some(servers) = native_servers(&control) {
        assert_eq!(servers, json!([]));
    }

    let attached = run_case("required", settings, "    mcp_servers: [lookup]\n", "codex");
    assert_command_attached(&attached);
}

fn assert_command_attached(case: &CodexRun) {
    let record = assert_completed(case, "lookup");
    assert!(record["prompt"].as_str().expect("prompt").contains("lookup available: true"));
    let expected = json!({
        "command": "lookup-server", "args": ARGUMENTS,
        "env": {"TOKEN": TOKEN, "PATH": case.dir.join("bin"), "EMPTY": ""},
        "cwd": fs::canonicalize(&case.dir).expect("checkout root").join("server home")
    });
    assert_eq!(record["config"]["mcp_servers"], json!({"lookup": expected}));
    if let Some(servers) = native_servers(case) {
        assert_eq!(servers.as_array().expect("servers").len(), 1);
        assert_eq!(servers[0]["name"], "lookup");
        assert_eq!(servers[0]["enabled"], true);
        assert_eq!(servers[0]["transport"]["type"], "stdio");
        for (key, value) in expected.as_object().expect("expected fields") {
            assert_eq!(&servers[0]["transport"][key], value, "native {key}");
        }
        assert!(
            servers[0]["startup_timeout_sec"].is_null(),
            "Rhei's TCP timeout is not a native handshake timeout"
        );
    }
}

#[test]
fn codex_mcp_multiple_definitions_preserve_server_ids() {
    let case = run_case(
        "server-ids",
        json!({"mcp_servers": {
            "lookup": command_server(),
            "lookup.with.dot": {"command": ["lookup-server", "second"]}
        }}),
        "    mcp_servers: [lookup, lookup.with.dot]\n",
        "codex",
    );
    let record = assert_completed(&case, "lookup,lookup.with.dot");
    let servers = record["config"]["mcp_servers"].as_object().expect("server table");
    assert_eq!(servers.len(), 2);
    assert_eq!(servers["lookup"]["args"], json!(ARGUMENTS));
    assert_eq!(servers["lookup.with.dot"]["command"], "lookup-server");
    assert_eq!(servers["lookup.with.dot"]["args"], json!(["second"]));
    if let Some(servers) = native_servers(&case) {
        let servers = servers.as_array().expect("native servers");
        assert_eq!(servers.len(), 2);
        let dotted = servers
            .iter()
            .find(|server| server["name"] == "lookup.with.dot")
            .expect("one native server under the complete dotted id");
        assert_eq!(dotted["transport"]["command"], "lookup-server");
        assert_eq!(dotted["transport"]["args"], json!(["second"]));
    }
}

#[test]
fn codex_mcp_defaults_and_family_use_native_configuration() {
    // Exercise defaults on the built-in, and defaults/state entries on a wrapper.
    let mut failures = Vec::new();
    for (name, agent, defaults, field) in [
        ("defaults", "codex", json!(["lookup"]), ""),
        ("family-defaults", "cdx", json!(["lookup"]), ""),
        ("family-state", "cdx", json!([]), "    mcp_servers: [lookup]\n"),
    ] {
        let case = run_case(
            name,
            json!({
                "defaults": {"mcp_servers": defaults},
                "agents": {"cdx": {"family": "codex"}},
                "mcp_servers": {"lookup": command_server()}
            }),
            field,
            agent,
        );
        if !case.run.status.success() {
            failures.push(format!("{name}: {}", case.record.as_ref().expect("spawned")["error"]));
        } else {
            assert_command_attached(&case);
        }
    }
    assert!(failures.is_empty(), "Codex attachment failures:\n{}", failures.join("\n"));
}

#[test]
fn codex_mcp_required_unsupported_transports_are_unavailable() {
    for (transport, scheme) in [("sse", "http"), ("websocket", "ws")] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reachable host");
        let port = listener.local_addr().expect("address").port();
        let case = run_case(
            transport,
            json!({"mcp_servers": {"lookup": {
                "url": format!("{scheme}://127.0.0.1:{port}/mcp"),
                "transport": transport, "startup_timeout": "1s"
            }}}),
            "    mcp_servers: [lookup]\n",
            "codex",
        );
        assert!(
            case.record.is_none(),
            "unrepresentable {transport} must block before Codex is spawned: {}",
            case.record.as_ref().map(|record| record["argv"].to_string()).unwrap_or_default()
        );
        assert_success(&case.run);
        assert!(case.plan.contains("**State:** blocked"), "{}", case.plan);
        assert!(case.run.stdout.contains("mcp unavailable: lookup"), "{}", case.run.stdout);
    }
}

#[test]
fn codex_mcp_optional_unsupported_transports_are_dropped() {
    for (transport, scheme) in [("sse", "http"), ("websocket", "ws")] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reachable host");
        let port = listener.local_addr().expect("address").port();
        let case = run_case(
            transport,
            json!({"mcp_servers": {"lookup": {
                "url": format!("{scheme}://127.0.0.1:{port}/mcp"),
                "transport": transport
            }}}),
            "    mcp_servers: [{id: lookup, optional: true}]\n",
            "codex",
        );
        assert!(
            case.run.stderr.lines().any(|line| line.contains("optional mcp 'lookup' unavailable")
                && line.contains(transport)),
            "warning must name unsupported {transport}: {}",
            case.run.stderr
        );
        let record = assert_completed(&case, "");
        assert!(record["config"].get("mcp_servers").is_none());
        assert!(record["prompt"].as_str().expect("prompt").contains("lookup available: false"));
    }
}
