//! Claude Code's resolved selection excludes native MCP additions.
//! §FS-rhei-states.7.2 §FS-rhei-states.7.3 §FS-rhei-agents.1.1.2

use super::*;
use serde_json::{json, Value};
use std::process::Stdio;

fn recorder() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mcp-surface-fixture"))
}

fn control(args: &[&str]) -> Value {
    let output =
        Command::new(recorder()).args(args).stdin(Stdio::null()).output().expect("fixture");
    assert!(output.status.success(), "fixture failed: {}", stderr(&output));
    let record = serde_json::from_slice(&output.stdout).expect("fixture JSON");
    println!("control {args:?}: {record}");
    record
}

#[test]
fn mcp_surface_fixture_controls_distinguish_additive_strict_and_positional_options() {
    assert_eq!(control(&["--"])["servers"], json!(["mail"]));
    assert_eq!(control(&["--strict-mcp-config", "--"])["servers"], json!([]));
    assert_eq!(control(&["--", "--strict-mcp-config"])["servers"], json!(["mail"]));
    let dir = unique_temp_dir("mcp-control");
    let config = write_fixture_file(
        &dir,
        "mcp.json",
        r#"{"mcpServers":{"declared":{"url":"https://fixture.invalid"}}}"#,
    );
    let config = config.to_str().expect("fixture config path");
    assert_eq!(control(&["--mcp-config", config, "--"])["servers"], json!(["declared", "mail"]));
    assert_eq!(
        control(&["--strict-mcp-config", "--mcp-config", config, "--"])["servers"],
        json!(["declared"])
    );
}

fn exercise_profile(agent: &str) {
    // Preserve absent/default union, clearing and same-id state precedence.
    let cases = [
        ("absent", json!([]), "", vec![]),
        ("empty", json!([]), "    mcp_servers: []\n", vec![]),
        ("declared", json!([]), "    mcp_servers: [declared]\n", vec!["declared"]),
        ("inherited", json!(["inherited"]), "", vec!["inherited"]),
        ("cleared", json!(["inherited"]), "    mcp_servers: []\n", vec![]),
        (
            "union",
            json!(["inherited"]),
            "    mcp_servers: [declared]\n",
            vec!["declared", "inherited"],
        ),
        (
            "override",
            json!(["declared"]),
            "    mcp_servers:\n      - id: declared\n        url: https://state.invalid\n        transport: sse\n",
            vec!["declared"],
        ),
    ];
    let mut leaks = Vec::new();
    for (case, defaults, state_field, expected) in cases {
        let dir = unique_temp_dir(&format!("mcp-surface-{agent}-{case}"));
        let bin_dir = dir.join("bin");
        fs::create_dir_all(&bin_dir).expect("private PATH directory");
        let claude = bin_dir.join(format!("claude{}", std::env::consts::EXE_SUFFIX));
        fs::copy(recorder(), &claude).expect("install native recording executable as claude");
        let mut settings = json!({
            "defaults": {"mcp_servers": defaults},
            "mcp_servers": {
                "declared": {"url": "https://registry.invalid", "transport": "sse"},
                "inherited": {"url": "https://inherited.invalid", "transport": "sse"}
            },
            "agents": {"cld": {"family": "claude-code", "command": [claude]}}
        });
        // An absent selection is authored without a defaults MCP field.
        if case == "absent" {
            settings["defaults"].as_object_mut().expect("defaults").remove("mcp_servers");
        }
        let config_dir = dir.join(".agent-grounds/rhei");
        fs::create_dir_all(&config_dir).expect("settings dir");
        write_fixture_file(&config_dir, "settings.json", &settings.to_string());
        let plan = write_fixture_file(
            &dir,
            "plan.rhei.md",
            "# Rhei: MCP surface\n\n## Tasks\n\n### Task 1: Record tooling\n**State:** work\n",
        );
        let machine = write_fixture_file(&dir, "states.yaml", &format!(
            "name: mcp-surface\nversion: 1\nstates:\n  work:\n    agent: {agent}\n{state_field}    agent_timeout: 5s\n  completed:\n    final: true\ntransitions:\n  - from: work\n    to: completed\n"
        ));
        let mut path = vec![bin_dir];
        path.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let output = rhei_command(dir.join("home"))
            .current_dir(&dir)
            .env_remove("RHEI_BUDGET_PARENT_RESERVATION")
            .env("PATH", std::env::join_paths(path).expect("fixture PATH"))
            .env("RHEI_MCP_RECORD_DIR", dir.join("records"))
            .args(["--state-machine"])
            .arg(machine)
            .arg("run")
            .arg(plan)
            .args(["--no-tui", "--no-callbacks", "--until-idle"])
            .output()
            .expect("run rhei against recorder");
        assert!(output.status.success(), "{agent}/{case}: {}", stderr(&output));
        let record: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("records/record.json"))
                .expect("agent ran and recorded argv"),
        )
        .expect("record JSON");
        println!("{agent}/{case}: {record}");
        let argv: Vec<&str> = record["argv"]
            .as_array()
            .expect("argv")
            .iter()
            .map(|arg| arg.as_str().expect("arg"))
            .collect();
        assert_eq!(argv.last(), Some(&"--"), "separator last: {argv:?}");
        let mut selected: Vec<_> = record["selected"]
            .as_str()
            .expect("selected CSV")
            .split(',')
            .filter(|s| !s.is_empty())
            .collect();
        selected.sort();
        assert_eq!(selected, expected, "Rhei selection {agent}/{case}");
        let config = &record["config"];
        if expected.is_empty() {
            assert!(config.is_null(), "empty selection generates no config: {record}");
            assert!(!argv.contains(&"--mcp-config"));
            assert!(!dir.join("runtime/tmp").exists(), "no MCP config file for empty selection");
        } else {
            let ids: Vec<_> = config["mcpServers"]
                .as_object()
                .expect("attached MCP JSON")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(ids, expected, "attached IDs {agent}/{case}");
            let url = if case == "override" {
                "https://state.invalid"
            } else {
                "https://registry.invalid"
            };
            if expected.contains(&"declared") {
                assert_eq!(config["mcpServers"]["declared"]["url"], url);
            }
            let flag = argv.iter().position(|arg| *arg == "--mcp-config").expect("attachment flag");
            assert!(flag + 1 < argv.len() - 1, "config before separator");
            assert!(Path::new(argv[flag + 1]).is_file(), "generated file exists");
        }
        if record["servers"] != json!(expected) || record["strict"] != true {
            leaks.push(format!("{agent}/{case}: missing --strict-mcp-config before --; native mail leaked; servers={} expected={}", record["servers"], json!(expected)));
        } else {
            let strict =
                argv.iter().position(|arg| *arg == "--strict-mcp-config").expect("strict flag");
            assert!(strict < argv.len() - 1, "strict before separator");
            if let Some(config) = argv.iter().position(|arg| *arg == "--mcp-config") {
                assert!(strict < config, "strict before attachment");
            }
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
}

#[test]
fn mcp_surface_builtin_excludes_native_mail_for_every_selection() {
    exercise_profile("claude-code");
}

#[test]
fn mcp_surface_family_wrapper_excludes_native_mail_for_every_selection() {
    exercise_profile("cld");
}
