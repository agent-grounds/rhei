// `rhei new --mcp-server` and `--skill`: one entry per value, read by the
// plan's own entry reader, and every id checked against the merged registry
// before anything is written. §FS-rhei-task-tooling.8 §FS-rhei-new.1.3

/// Refuse a malformed entry or an id the merged settings registry does not
/// hold, as an argument error, before the plan is touched.
// §FS-rhei-task-tooling.8
fn reject_task_tooling_flags(options: &NewOptions, target: &Path) -> MietteResult<()> {
    if options.mcp_servers.is_empty() && options.skills.is_empty() {
        return Ok(());
    }
    let mcp_servers = read_task_tooling_flag("--mcp-server", MCP_SERVERS_FIELD, &options.mcp_servers)?;
    let skills = read_task_tooling_flag("--skill", SKILLS_FIELD, &options.skills)?;
    let settings = load_merged_settings(&execution_workspace_root(target))?;
    reject_unknown_task_tooling_ids("--mcp-server", "mcp server", &mcp_servers, &settings.mcp_servers)?;
    reject_unknown_task_tooling_ids("--skill", "skill", &skills, &settings.skills)
}

/// Read one flag's values, one entry each, with the reader the plan parser
/// uses, so `rhei new` writes nothing the parser would refuse.
fn read_task_tooling_flag(
    flag: &str,
    field: &str,
    values: &[String],
) -> MietteResult<Vec<TaskToolingEntry>> {
    parse_tooling_entries(field, values.iter().map(String::as_str)).map_err(|message| {
        miette!(
            help = format!(
                "give one registry id per {flag}, optionally followed by ` (optional)`: \
                 {flag} 'grafana (optional)'. Repeat the flag for several."
            ),
            "{flag} is not valid: {message}"
        )
    })
}

fn reject_unknown_task_tooling_ids<T>(
    flag: &str,
    kind: &str,
    entries: &[TaskToolingEntry],
    registry: &BTreeMap<String, T>,
) -> MietteResult<()> {
    let Some(unknown) = entries.iter().find(|entry| !registry.contains_key(&entry.id)) else {
        return Ok(());
    };
    let known = registry.keys().map(String::as_str).collect::<Vec<_>>();
    let known = if known.is_empty() { "none".to_string() } else { known.join(", ") };
    Err(miette!(
        help = format!("name an id from the merged settings registry (known: {known})."),
        "{flag} '{}' references unknown {kind} '{}'",
        unknown.authored(),
        unknown.id
    ))
}
