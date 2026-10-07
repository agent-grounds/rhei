/// Materialize the file a `mcp_config_flag` profile is handed, under
/// `runtime_dir/tmp/`: one `mcpServers` key per resolved entry, each written in
/// the shape §FS-rhei-mcp-config-file.1 gives rather than copied from the
/// agent-neutral registry (§FS-rhei-agents.1.1.4). The file is overwritten on
/// every spawn so stale entries do not linger.
fn write_mcp_config_file(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    agent_id: &str,
    entries: &[&ResolvedMcpEntry],
) -> Option<PathBuf> {
    let tmp_dir = runtime_dir.join("tmp");
    if let Err(err) = fs::create_dir_all(&tmp_dir) {
        diag_warn!("warning: failed to create MCP config tmp dir '{}': {err}", tmp_dir.display());
        return None;
    }
    let safe_agent = env_id_segment(agent_id).to_lowercase();
    let path = tmp_dir.join(format!("mcp-{task_id}-{state_name}-{safe_agent}.json"));

    let servers: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .filter_map(|entry| Some((entry.id.clone(), mcp_config_entry(entry.definition.as_ref()?))))
        .collect();
    let envelope = serde_json::json!({ "mcpServers": serde_json::Value::Object(servers) });
    match serde_json::to_string_pretty(&envelope) {
        Ok(text) => match fs::write(&path, text) {
            Ok(()) => Some(path),
            Err(err) => {
                diag_warn!("warning: failed to write MCP config '{}': {err}", path.display());
                None
            }
        },
        Err(err) => {
            diag_warn!("warning: failed to serialize MCP config: {err}");
            None
        }
    }
}

/// One registry entry as the `--mcp-config` schema reads it
/// (§FS-rhei-mcp-config-file.1): a `command` entry as a string `command`, its
/// `args` and its expanded `env`; a `url` entry as its `url` and the `type` its
/// `transport` maps to. Nothing else the registry carries is written.
fn mcp_config_entry(def: &McpServerProfile) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(command) = &def.command {
        // An inline `command: []` is refused at load (`validate_state_mcp_entries`), a registry
        // one is not: it is written as `""`, for the agent to refuse as it would any bad command.
        let (program, args) =
            command.split_first().map_or(("", &[][..]), |(program, args)| (program.as_str(), args));
        obj.insert("command".into(), program.into());
        obj.insert("args".into(), args.into());
        if !def.env.is_empty() {
            let env = def.env.iter().map(|(k, v)| (k.clone(), expand_env_vars(v).into())).collect();
            obj.insert("env".into(), serde_json::Value::Object(env));
        }
    } else if let Some(url) = &def.url {
        obj.insert("url".into(), url.as_str().into());
        if let Some(transport) = &def.transport {
            obj.insert("type".into(), mcp_config_type(transport).into());
        }
    }
    serde_json::Value::Object(obj)
}

/// The `type` an `--mcp-config` entry takes for the registry's `transport`:
/// `sse` is `sse` and `websocket` is `ws`, and any other value is written
/// unchanged for the agent to accept or refuse. §FS-rhei-mcp-config-file.1
fn mcp_config_type(transport: &str) -> &str {
    match transport {
        "websocket" => "ws",
        other => other,
    }
}

/// One warning per `command` entry bound for the `--mcp-config` file that
/// declares a `working_directory`: the file cannot carry it, so the server starts
/// in the agent's working directory. The entry stays attached and available.
/// §FS-rhei-mcp-config-file.2
fn mcp_config_working_directory_warnings(
    resolved: &ResolvedAgent,
    tooling: &ResolvedTooling,
) -> Vec<String> {
    // The file is written only where `build_agent_command` writes it.
    let Some(flag) = resolved.profile.mcp_config_flag.as_deref() else {
        return Vec::new();
    };
    if resolved.profile.mcp_flag.is_some() {
        return Vec::new();
    }
    tooling
        .mcp_servers
        .iter()
        .filter_map(|entry| {
            let def = entry.definition.as_ref()?;
            let dir = def.working_directory.as_deref().filter(|_| def.command.is_some())?;
            Some(format!(
                "warning: mcp '{}' declares working_directory '{dir}', which agent '{}' cannot \
                 be given in its {flag} file; the server starts in the agent's working directory",
                entry.id,
                resolved.agent.id()
            ))
        })
        .collect()
}
