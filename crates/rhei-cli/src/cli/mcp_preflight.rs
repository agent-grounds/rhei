// The spawn preflight an MCP server's definition must pass to be available: what
// Rhei can check without starting the server, which only the agent does.
// §FS-rhei-agents.6.2

/// The `startup_timeout` an entry that declares none waits. §FS-rhei-agents.1.1.4
const DEFAULT_MCP_STARTUP_TIMEOUT: &str = "10s";

/// The gate a spawn applies: `gate_tooling_for_agent` with every MCP definition
/// held to the preflight. Servers start in `checkout_root`, or in a declared
/// `working_directory` when the agent attaches them with `mcp_flag`, the one
/// wiring that can carry it. §FS-rhei-agents.6.2 §FS-rhei-mcp-config-file.2
fn gate_tooling_at_spawn(
    resolved: &ResolvedAgent,
    tooling: &ResolvedTooling,
    checkout_root: &Path,
) -> ToolingGateResult {
    let carries_working_directory = resolved.profile.mcp_flag.is_some();
    let preflight = |def: &McpServerProfile| {
        preflight_mcp_server(def, checkout_root, carries_working_directory)
    };
    gate_tooling_for_agent(resolved, tooling, &preflight)
}

/// `Err` carries the reason the optional-entry warning and the required-tooling
/// error name. §FS-rhei-agents.6.2
fn preflight_mcp_server(
    def: &McpServerProfile,
    checkout_root: &Path,
    carries_working_directory: bool,
) -> Result<(), String> {
    if let Some(command) = &def.command {
        return preflight_mcp_command(def, command, checkout_root, carries_working_directory);
    }
    let timeout = def.startup_timeout.as_deref().unwrap_or(DEFAULT_MCP_STARTUP_TIMEOUT);
    match &def.url {
        Some(url) => preflight_mcp_url(url, timeout),
        None => Err("declares neither 'command' nor 'url'".to_string()),
    }
}

/// The executable resolves where the server starts, on the `PATH` it starts
/// with: the entry's own `env.PATH` when it declares one. §FS-rhei-agents.6.2
fn preflight_mcp_command(
    def: &McpServerProfile,
    command: &[String],
    checkout_root: &Path,
    carries_working_directory: bool,
) -> Result<(), String> {
    let Some(program) = command.first().filter(|program| !program.is_empty()) else {
        return Err("has an empty 'command'".to_string());
    };
    let start_dir = match def.working_directory.as_deref().filter(|_| carries_working_directory) {
        Some(dir) => {
            let dir = checkout_root.join(dir);
            if !dir.is_dir() {
                return Err(format!("working directory '{}' does not exist", dir.display()));
            }
            dir
        }
        None => checkout_root.to_path_buf(),
    };
    let path = match def.env.get("PATH") {
        Some(value) => Some(std::ffi::OsString::from(expand_env_vars(value))),
        None => std::env::var_os("PATH"),
    };
    match resolve_executable(program, &start_dir, path.as_deref()) {
        Some(_) => Ok(()),
        None if names_a_path(program) => Err(format!(
            "command '{program}' is not an executable file in '{}'",
            start_dir.display()
        )),
        None => Err(format!("command '{program}' is not on PATH")),
    }
}

/// A TCP connection to the URL's host and port completes within `timeout`,
/// and is closed without a byte sent. §FS-rhei-agents.6.2
fn preflight_mcp_url(url: &str, timeout: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs as _;
    let Some((host, port)) = mcp_url_host_port(url) else {
        return Err(format!("url '{url}' names no host and port to connect to"));
    };
    let bound = rhei_validator::parse_duration_secs(timeout).filter(|secs| *secs > 0).unwrap_or(10);
    let addrs = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|err| format!("host '{host}' does not resolve: {err}"))?;
    let deadline = Instant::now() + Duration::from_secs(bound);
    for addr in addrs {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        if std::net::TcpStream::connect_timeout(&addr, left).is_ok() {
            return Ok(());
        }
    }
    Err(format!("{host}:{port} accepted no connection within {timeout}"))
}

/// The host and port a URL connects to, its port defaulting by scheme: `80` for
/// `http` and `ws`, `443` for `https` and `wss`. `None` when it names no host,
/// or no port under a scheme with no default. §FS-rhei-agents.6.2
fn mcp_url_host_port(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host_port)| host_port);
    let (host, port) = match host_port.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed.split_once(']')?;
            (host, after.strip_prefix(':'))
        }
        None => match host_port.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        },
    };
    if host.is_empty() {
        return None;
    }
    let port = match port {
        Some(port) => port.parse().ok()?,
        None => match scheme.to_ascii_lowercase().as_str() {
            "http" | "ws" => 80,
            "https" | "wss" => 443,
            _ => return None,
        },
    };
    Some((host.to_string(), port))
}

/// `startup_timeout` must be a duration greater than zero, since it bounds the
/// preflight's connection: one error per registry entry that breaks it.
/// §FS-rhei-agents.1.1.4
fn registry_startup_timeout_errors(registry: &BTreeMap<String, McpServerProfile>) -> Vec<String> {
    let timeouts =
        registry.iter().map(|(id, def)| (format!("mcp_servers.'{id}'"), &def.startup_timeout));
    timeouts
        .filter_map(|(label, timeout)| startup_timeout_error(&label, timeout.as_deref()))
        .collect()
}

/// The same rule for the inline entries of a list labelled `label`.
/// §FS-rhei-agents.1.1.4
fn inline_startup_timeout_errors(label: &str, entries: Option<&[StateMcpEntry]>) -> Vec<String> {
    let objects = entries.unwrap_or(&[]).iter().filter_map(|entry| match entry {
        StateMcpEntry::Object(object) => Some(object),
        StateMcpEntry::Id(_) => None,
    });
    objects
        .filter_map(|object| {
            let label = format!("{label} entry '{}'", object.id);
            startup_timeout_error(&label, object.startup_timeout.as_deref())
        })
        .collect()
}

fn startup_timeout_error(label: &str, timeout: Option<&str>) -> Option<String> {
    let timeout = timeout?;
    let positive = rhei_validator::parse_duration_secs(timeout).is_some_and(|secs| secs > 0);
    (!positive).then(|| {
        format!(
            "{label} has startup_timeout '{timeout}', which is not a duration greater than \
             zero such as '10s'"
        )
    })
}
