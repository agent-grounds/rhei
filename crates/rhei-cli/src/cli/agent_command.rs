
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AgentStdinFormat {
    PlainLine,
    ClaudeCodeStreamJson,
}

fn agent_stdin_format(resolved: &ResolvedAgent) -> AgentStdinFormat {
    if resolved.agent.id() == "claude-code" && resolved.profile.intervene_stdin {
        AgentStdinFormat::ClaudeCodeStreamJson
    } else {
        AgentStdinFormat::PlainLine
    }
}

fn stdin_message_bytes(format: AgentStdinFormat, message: &str) -> Vec<u8> {
    match format {
        AgentStdinFormat::PlainLine => {
            let mut bytes = message.as_bytes().to_vec();
            bytes.push(b'\n');
            bytes
        }
        AgentStdinFormat::ClaudeCodeStreamJson => {
            let mut bytes = serde_json::json!({
                "type": "user",
                "message": {
                    "role": "user",
                    "content": [{ "type": "text", "text": message }]
                }
            })
            .to_string()
            .into_bytes();
            bytes.push(b'\n');
            bytes
        }
    }
}

/// Remove the enclosing run's identity before any autonomous agent starts.
///
/// `Command` otherwise inherits values that were present on `rhei run` itself,
/// so merely ceasing to set these names would still leak them through custom
/// wrappers and every child the worker launches. §FS-rhei-agents.4
fn remove_outer_rhei_identity(cmd: &mut std::process::Command) {
    for name in [
        "RHEI_ROOT",
        "RHEI_PLAN_PATH",
        "RHEI_RESULT_PATH",
        "RHEI_TASK_ID",
        "RHEI_TASK_ID_LOCAL",
    ] {
        cmd.env_remove(name);
    }
}

/// Build a `Command` for the resolved agent.
///
/// Flag order:
/// `<command...> <mode flags...> <autonomous_args...> <effort args...> <accounting flags...>
///  <prompt_flag> <prompt>? <model_flag> <model>? <snapshot_args...>
///  <mcp/skill flags...> --?`
/// `--` is appended last when `stdin_prompt` is `true`, to match
/// `codex exec --`-style invocations that expect stdin: past a separator a flag
/// is prompt text, so everything rhei has to say to the agent — the snapshot,
/// MCP and skill flags included — is emitted before it
/// (§FS-rhei-agents.2.2). A prompt flag declared alongside
/// `stdin_prompt` is emitted with no value, because the flag says the agent is
/// non-interactive and the value is the prompt: two different questions
/// (§FS-rhei-agents.1.1.2). `intervene_stdin` also
/// requests a stdin pipe. For `claude-code`, opting into `intervene_stdin`
/// switches the command to stream-json stdin so the running process actually
/// consumes interventions.
///
/// `snapshot_args` are the resolved snapshot preload's strategy flags —
/// `--session-dir <dir>`, `--fork <path>`, `--continue <id>`, or a resume the
/// agent spells as a positional subcommand. They go in one slot, ahead of the
/// separator and of the MCP and skill flags. §FS-rhei-snapshots.10.1
///
/// `runtime_dir` is used to materialize an MCP config file for agents that
/// declare `mcp_config_flag` (e.g. `claude-code --mcp-config <path>`). The
/// file is written under `runtime_dir/tmp/` and overwritten on every spawn.
#[allow(clippy::too_many_arguments)]
fn build_agent_command(
    resolved: &ResolvedAgent,
    prompt: &str,
    checkout_root: &Path,
    worktree_root: Option<&Path>,
    state_machine_path: Option<&Path>,
    task_id: &str,
    state_name: &str,
    visit_count: u64,
    // Which attempt of this visit the invocation is. §FS-rhei-agents.4
    attempt: u64,
    tooling: &ResolvedTooling,
    runtime_dir: &Path,
    // The snapshot preload's strategy flags, emitted at the one slot the spawn
    // sequence reserves for them. §FS-rhei-snapshots.10.1
    snapshot_args: &[String],
) -> std::process::Command {
    let profile = &resolved.profile;
    let id = resolved.agent.id();
    let stdin_format = agent_stdin_format(resolved);
    let claude_stream_json = stdin_format == AgentStdinFormat::ClaudeCodeStreamJson;

    let (program, base_args) =
        profile.command.split_first().expect("registry profile has non-empty command");

    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(checkout_root);
    remove_outer_rhei_identity(&mut cmd);
    for arg in base_args {
        cmd.arg(arg);
    }

    if let Some(mode) = resolved.mode.as_deref() {
        if let Some(flags) = profile.modes.get(mode) {
            for arg in flags {
                cmd.arg(arg);
            }
        }
    }

    // `models.<id>.agents.<agent>.autonomous_args` extend the mode flag set
    // for autonomous (`rhei run`-driven) invocations. They appear after
    // mode flags and before the prompt/model so an agent that requires its
    // sandbox/permission flag right after the subcommand still gets them in
    // the right slot.
    for arg in &resolved.autonomous_args {
        cmd.arg(arg);
    }
    configure_agent_accounting_args(&mut cmd, resolved);

    if profile.stdin_prompt || profile.intervene_stdin {
        cmd.stdin(std::process::Stdio::piped());
    } else {
        // No agent reads the operator's terminal: an inherited tty read from a
        // background process group stops the child on SIGTTIN, and the
        // keystrokes belong to `rhei run`. §FS-rhei-run.3.2
        cmd.stdin(std::process::Stdio::null());
    }
    if claude_stream_json {
        // §FS-rhei-agents.1.1.2: Claude Code live intervention uses stream-json stdin.
        if let Some(flag) = &profile.prompt_flag {
            cmd.arg(flag);
        }
        cmd.arg("--input-format").arg("stream-json");
        cmd.arg("--output-format").arg("stream-json");
        cmd.arg("--verbose");
    } else if let Some(flag) = &profile.prompt_flag {
        // The flag says the agent is non-interactive and the value is the
        // prompt: two different questions, so stdin delivery drops the value
        // and keeps the flag rather than discarding both. §FS-rhei-agents.1.1.2
        cmd.arg(flag);
        if !profile.stdin_prompt {
            cmd.arg(prompt);
        }
    }

    // Use the concrete provider model name from the `models` registry when
    // available; fall back to the model id otherwise so untracked literals
    // §FS-rhei-agents.1.1.2 §FS-rhei-agents.1.1.3: Model flag value resolution.
    let model_flag_value = resolved.model_name.as_deref().or(resolved.model.as_deref());
    if let (Some(flag), Some(model)) = (&profile.model_flag, model_flag_value) {
        cmd.arg(flag).arg(model);
    }

    // One slot: after the mode, prompt and model flags, ahead of the tooling
    // flags, and so ahead of the separator a resume would read as prompt text.
    // §FS-rhei-snapshots.10.1
    for arg in snapshot_args {
        cmd.arg(arg);
    }

    // Append MCP and skill flags. Only entries whose definition resolved
    // (registry hit or inline) are emitted; this matches `RHEI_MCP_SERVERS`
    // §FS-rhei-agents.2 §FS-rhei-agents.6: Emit available tooling flags only.
    if let Some(flag) = profile.mcp_flag.as_deref() {
        for entry in &tooling.mcp_servers {
            if entry.definition.is_some() {
                cmd.arg(flag).arg(&entry.id);
            }
        }
    } else if let Some(flag) = profile.mcp_config_flag.as_deref() {
        let available: Vec<&ResolvedMcpEntry> =
            tooling.mcp_servers.iter().filter(|e| e.definition.is_some()).collect();
        if !available.is_empty() {
            if let Some(path) =
                write_mcp_config_file(runtime_dir, task_id, state_name, id, &available)
            {
                cmd.arg(flag).arg(path);
            }
        }
    }
    if let Some(flag) = profile.skill_flag.as_deref() {
        for entry in &tooling.skills {
            if entry.definition.is_some() {
                cmd.arg(flag).arg(&entry.id);
            }
        }
    }

    // Last, after everything rhei has to say to the agent: past a separator a
    // flag is prompt text, which the agent neither applies nor complains about.
    // §FS-rhei-agents.2.2
    if profile.stdin_prompt {
        cmd.arg("--");
    }

    cmd.env("RHEI_CHECKOUT_ROOT", checkout_root)
        .env("RHEI_STATE", state_name)
        .env("RHEI_VISIT_COUNT", visit_count.to_string())
        // A retry that is handed the same environment as the attempt it is
        // recovering from has no way to know it is one. §FS-rhei-agents.4
        .env("RHEI_ATTEMPT", attempt.to_string())
        .env("RHEI_AGENT", id);
    if let Some(path) = worktree_root {
        cmd.env("RHEI_WORKTREE_ROOT", path);
    } else {
        cmd.env_remove("RHEI_WORKTREE_ROOT");
    }
    if let Some(path) = state_machine_path {
        cmd.env("RHEI_STATE_MACHINE_PATH", path);
    }
    if let Some(model) = &resolved.model {
        cmd.env("RHEI_MODEL", model);
    }
    if let Some(mode) = &resolved.mode {
        cmd.env("RHEI_AGENT_MODE", mode);
    }
    if let Some(target) = &resolved.target {
        cmd.env("RHEI_TARGET", target.selector());
        cmd.env("RHEI_TARGET_SLUG", target.slug());
    }
    if let Some(provider) = resolved.model_provider.as_deref() {
        cmd.env("RHEI_MODEL_PROVIDER", provider);
    }
    if let Some(model_name) = resolved.model_name.as_deref() {
        cmd.env("RHEI_MODEL_NAME", model_name);
    }
    inject_tooling_env(&mut cmd, tooling);
    cmd
}

/// Materialize a `mcp_config_flag`-style MCP config file under
/// `runtime_dir/tmp/`. The file uses the Anthropic-flavoured `mcpServers`
/// envelope (`{ "mcpServers": { id: { command|url, ... } } }`) so a single
/// path can be passed to agents like `claude-code --mcp-config <path>`. The
/// file is overwritten on every spawn so stale entries do not linger.
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

    let mut servers = serde_json::Map::new();
    for entry in entries {
        let Some(def) = entry.definition.as_ref() else {
            continue;
        };
        let mut obj = serde_json::Map::new();
        if let Some(command) = &def.command {
            obj.insert(
                "command".to_string(),
                serde_json::Value::Array(
                    command.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                ),
            );
        }
        if let Some(url) = &def.url {
            obj.insert("url".to_string(), serde_json::Value::String(url.clone()));
        }
        if let Some(transport) = &def.transport {
            obj.insert("transport".to_string(), serde_json::Value::String(transport.clone()));
        }
        if !def.env.is_empty() {
            let mut env_map = serde_json::Map::new();
            for (k, v) in &def.env {
                env_map.insert(k.clone(), serde_json::Value::String(expand_env_vars(v)));
            }
            obj.insert("env".to_string(), serde_json::Value::Object(env_map));
        }
        if let Some(wd) = &def.working_directory {
            obj.insert("workingDirectory".to_string(), serde_json::Value::String(wd.clone()));
        }
        servers.insert(entry.id.clone(), serde_json::Value::Object(obj));
    }
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

/// Expand `${VAR}` references in a string against the current process
/// environment. Unknown variables expand to the empty string, matching the
/// §FS-rhei-agents.1.1.4: Expand MCP profile environment references.
fn expand_env_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end_rel) = bytes[i + 2..].iter().position(|&b| b == b'}') {
                let name_start = i + 2;
                let name_end = name_start + end_rel;
                let name = &input[name_start..name_end];
                out.push_str(&std::env::var(name).unwrap_or_default());
                i = name_end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Emit spawn-time warnings for tooling the agent cannot consume.
///
/// Skills resolve only when the agent declares `skill_flag`; MCP entries
/// resolve only when the agent declares `mcp_flag` or `mcp_config_flag`.
/// State-declared entries that the agent cannot wire are reported here so
/// operators can see why no flags were emitted. Required-entry escalation to
/// error is part of the availability subsystem and is not driven from here.
// §FS-rhei-agents.1.1.5 §FS-rhei-agents.6: Unsupported tooling diagnostics.
fn collect_unsupported_tooling_warnings(
    resolved: &ResolvedAgent,
    tooling: &ResolvedTooling,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let agent_id = resolved.agent.id();
    if resolved.profile.mcp_flag.is_none()
        && resolved.profile.mcp_config_flag.is_none()
        && tooling.mcp_servers.iter().any(|e| e.definition.is_some())
    {
        let ids: Vec<&str> = tooling
            .mcp_servers
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect();
        warnings.push(format!(
            "warning: agent '{agent_id}' declares no mcp_flag/mcp_config_flag; \
             dropping MCP entries: {}",
            ids.join(", ")
        ));
    }
    if resolved.profile.skill_flag.is_none()
        && tooling.skills.iter().any(|e| e.definition.is_some())
    {
        let ids: Vec<&str> = tooling
            .skills
            .iter()
            .filter(|e| e.definition.is_some())
            .map(|e| e.id.as_str())
            .collect();
        warnings.push(format!(
            "warning: agent '{agent_id}' declares no skill_flag; dropping \
             skill entries: {}",
            ids.join(", ")
        ));
    }
    warnings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolingKind {
    Mcp,
    Skill,
}

impl ToolingKind {
    fn as_str(self) -> &'static str {
        match self {
            ToolingKind::Mcp => "mcp",
            ToolingKind::Skill => "skill",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolingUnavailable {
    kind: ToolingKind,
    id: String,
    reason: String,
}

#[derive(Debug, Clone, Default)]
struct ToolingGateResult {
    tooling: ResolvedTooling,
    warnings: Vec<String>,
    required: Vec<ToolingUnavailable>,
}

fn gate_tooling_for_agent(
    resolved: &ResolvedAgent,
    tooling: &ResolvedTooling,
) -> ToolingGateResult {
    let mut result = ToolingGateResult::default();
    let agent_id = resolved.agent.id();
    let mcp_supported =
        resolved.profile.mcp_flag.is_some() || resolved.profile.mcp_config_flag.is_some();
    for entry in &tooling.mcp_servers {
        let reason = if entry.definition.is_none() {
            Some("definition is unavailable".to_string())
        } else if !mcp_supported {
            Some(format!("agent '{agent_id}' declares no mcp_flag/mcp_config_flag"))
        } else {
            None
        };
        if let Some(reason) = reason {
            if entry.optional {
                result.warnings.push(format!(
                    "warning: optional mcp '{}' unavailable ({}); dropping",
                    entry.id, reason
                ));
                let mut unavailable = entry.clone();
                unavailable.definition = None;
                result.tooling.mcp_servers.push(unavailable);
            } else {
                result.required.push(ToolingUnavailable {
                    kind: ToolingKind::Mcp,
                    id: entry.id.clone(),
                    reason,
                });
            }
        } else {
            result.tooling.mcp_servers.push(entry.clone());
        }
    }

    let skill_supported = resolved.profile.skill_flag.is_some();
    for entry in &tooling.skills {
        let reason = if entry.definition.is_none() {
            Some("definition is unavailable".to_string())
        } else if !skill_supported {
            Some(format!("agent '{agent_id}' declares no skill_flag"))
        } else {
            None
        };
        if let Some(reason) = reason {
            if entry.optional {
                result.warnings.push(format!(
                    "warning: optional skill '{}' unavailable ({}); dropping",
                    entry.id, reason
                ));
                let mut unavailable = entry.clone();
                unavailable.definition = None;
                result.tooling.skills.push(unavailable);
            } else {
                result.required.push(ToolingUnavailable {
                    kind: ToolingKind::Skill,
                    id: entry.id.clone(),
                    reason,
                });
            }
        } else {
            result.tooling.skills.push(entry.clone());
        }
    }

    result
}

fn unavailable_ids(required: &[ToolingUnavailable], kind: ToolingKind) -> Vec<String> {
    required.iter().filter(|issue| issue.kind == kind).map(|issue| issue.id.clone()).collect()
}

fn format_required_tooling_error(
    task_id: &str,
    state_name: &str,
    required: &[ToolingUnavailable],
) -> String {
    let details = required
        .iter()
        .map(|issue| format!("{}:{} ({})", issue.kind.as_str(), issue.id, issue.reason))
        .collect::<Vec<_>>()
        .join(", ");
    format!("required tooling unavailable for task {task_id} in state '{state_name}': {details}")
}

/// Format a [`SystemTime`] as an ISO 8601 UTC instant with second
/// precision (`YYYY-MM-DDThh:mm:ssZ`). Used for the `started:` / `ended:`
/// lines in the agent log header and footer.
///
/// Implemented without a date crate. Howard Hinnant's `civil_from_days`
/// algorithm converts an epoch-day count to year/month/day; epoch-second
/// arithmetic gives hour/minute/second. Times before the Unix epoch fall
/// back to the epoch itself.
fn format_iso8601_utc(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let sec_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = sec_of_day / 3_600;
    let minute = (sec_of_day % 3_600) / 60;
    let second = sec_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Convert a day count since 1970-01-01 to a Gregorian (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Format a duration in seconds as a human-readable string with the same
/// shape as `agent_timeout` literals (`30m`, `1h`, `2h30m`, `4m23s`, `0s`).
/// Zero-value units are omitted; an all-zero duration renders as `0s`.
fn format_duration_human(secs: u64) -> String {
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    let mut out = String::new();
    if hours > 0 {
        out.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        out.push_str(&format!("{minutes}m"));
    }
    if seconds > 0 || out.is_empty() {
        out.push_str(&format!("{seconds}s"));
    }
    out
}

/// Format the `mcp_servers:` / `skills:` line in the agent log header.
///
/// Returns `None` when the slice is empty (no line is written). An entry
/// suffixed with `?` is `optional: true` and failed its availability check —
/// it was dropped before spawn but appears here for diagnostics.
fn format_tooling_log_line<T, F>(entries: &[T], project: F) -> Option<String>
where
    F: Fn(&T) -> (&str, bool, bool),
{
    if entries.is_empty() {
        return None;
    }
    let rendered: Vec<String> = entries
        .iter()
        .map(|entry| {
            let (id, optional, available) = project(entry);
            if optional && !available {
                format!("{id}?")
            } else {
                id.to_string()
            }
        })
        .collect();
    Some(rendered.join(","))
}

/// Set `RHEI_MCP_*` and `RHEI_SKILL_*` env vars on the agent command.
///
/// Aggregates exposed:
/// - `RHEI_MCP_SERVERS`: comma-separated ids whose registry lookup succeeded
/// - `RHEI_SKILLS`: same, for skills
///
/// Per-entry availability is exposed as `RHEI_MCP_<NAME>_AVAILABLE` and
/// `RHEI_SKILL_<ID>_AVAILABLE` with `<NAME>` / `<ID>` normalized by
/// [`env_id_segment`].
fn inject_tooling_env(cmd: &mut std::process::Command, tooling: &ResolvedTooling) {
    cmd.env("RHEI_MCP_SERVERS", tooling.mcp_servers_csv());
    cmd.env("RHEI_SKILLS", tooling.skills_csv());
    for entry in &tooling.mcp_servers {
        cmd.env(
            format!("RHEI_MCP_{}_AVAILABLE", env_id_segment(&entry.id)),
            entry.definition.is_some().to_string(),
        );
    }
    for entry in &tooling.skills {
        cmd.env(
            format!("RHEI_SKILL_{}_AVAILABLE", env_id_segment(&entry.id)),
            entry.definition.is_some().to_string(),
        );
    }
}
