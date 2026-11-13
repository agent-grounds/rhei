/// The effective Codex family uses native configuration only for its `-c`
/// attachment mapping. Explicit overrides and standalone adapters keep their
/// existing meaning. §FS-rhei-agents.1.1.2.1
fn uses_codex_mcp_config(resolved: &ResolvedAgent) -> bool {
    resolved.family() == "codex" && resolved.profile.mcp_flag.as_deref() == Some("-c")
}

/// One native definition, used both to decide attachability before preflight
/// and to serialize the available selection. Registry remote transports cannot
/// be represented by Codex's Streamable HTTP. §FS-rhei-agents.1.1.2.1
fn codex_mcp_config_entry(
    def: &McpServerProfile,
    checkout_root: &Path,
) -> Result<toml_edit::InlineTable, String> {
    let Some(command) = &def.command else {
        return Err(match &def.url {
            Some(_) => format!(
                "Codex native MCP configuration cannot represent transport '{}'",
                def.transport.as_deref().unwrap_or("unspecified")
            ),
            None => "declares neither 'command' nor 'url'".to_string(),
        });
    };
    let Some((program, args)) = command.split_first().filter(|(program, _)| !program.is_empty())
    else {
        return Err("has an empty 'command'".to_string());
    };
    let mut entry = toml_edit::InlineTable::new();
    entry.insert("command", program.as_str().into());
    let args: toml_edit::Array = args.iter().map(String::as_str).collect();
    entry.insert("args", args.into());
    if !def.env.is_empty() {
        let mut env = toml_edit::InlineTable::new();
        for (key, value) in &def.env {
            env.insert(key.as_str(), expand_env_vars(value).into());
        }
        entry.insert("env", env.into());
    }
    if def.working_directory.is_some() {
        let directory = mcp_command_directory(def, checkout_root, true);
        let directory = directory
            .to_str()
            .ok_or_else(|| "Codex MCP working directory is not valid UTF-8".to_string())?;
        entry.insert("cwd", directory.into());
    }
    // startup_timeout bounds Rhei's URL preflight, not Codex's handshake.
    Ok(entry)
}

/// Pass the whole selected table as one TOML value: Codex splits override paths
/// at literal dots, so per-id paths corrupt dotted ids and repeated whole-table
/// overrides discard earlier definitions. §FS-rhei-agents.1.1.2.1
fn append_codex_mcp_config(
    cmd: &mut std::process::Command,
    tooling: &ResolvedTooling,
    checkout_root: &Path,
) {
    let mut servers = toml_edit::InlineTable::new();
    for entry in &tooling.mcp_servers {
        if let Some(definition) = &entry.definition {
            let definition = codex_mcp_config_entry(definition, checkout_root)
                .expect("the spawn gate checked native MCP attachability");
            servers.insert(entry.id.as_str(), definition.into());
        }
    }
    if !servers.is_empty() {
        cmd.arg("-c").arg(format!("mcp_servers={servers}"));
    }
}
