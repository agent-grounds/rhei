// The agents Rhei supports out of the box: one ready-to-use profile per id,
// with its named modes and the native flag mappings its family approves.
//
// Its own part because the catalogue is data about agents rather than a
// settings type: the merge reads it, and a user entry with the same id
// replaces an entry here wholesale.

// §AR-source-file-size.3 §FS-rhei-agents.2 §FS-rhei-snapshots.9.2

/// Built-in agent registry.
///
/// Each entry is a ready-to-use `CustomAgentProfile` for one of the agents
/// that Rhei supports out of the box. The per-agent "autonomous" flag set
/// that was hard-coded as `default_args` is now exposed as a named `yolo`
/// mode so states and defaults can select it explicitly via `agent_mode`.
///
/// A user-written entry with the same id in global or project settings
/// replaces the built-in entry wholesale (see `load_merged_settings`).
fn built_in_agents() -> BTreeMap<String, CustomAgentProfile> {
    fn flags(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    let modes_yolo_only = |yolo: Vec<String>| {
        let mut modes = IndexMap::new();
        modes.insert("yolo".to_string(), yolo);
        modes
    };

    // §FS-rhei-agents.1.1.2: Built-in profiles expose the approved native mappings.
    let effort = |values: &[&str], args: &[&str]| rhei_validator::AgentEffortProfile {
        values: values.iter().map(|value| ((*value).to_string(), (*value).to_string())).collect(),
        args: flags(args),
        conflicts: Vec::new(),
    };

    let mut agents = BTreeMap::new();

    // claude-code: the prompt travels on stdin, so a brief is not bounded by one
    // argument. `-p` stays declared and bare — it is `--print`, not the text.
    // §FS-rhei-agents.2 §FS-rhei-agents.1.1.2: Built-in claude-code profile.
    agents.insert(
        "claude-code".to_string(),
        CustomAgentProfile {
            command: flags(&["claude"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_config_flag: Some("--mcp-config".to_string()),
            skill_flag: Some("--skill".to_string()),
            modes: modes_yolo_only(flags(&["--permission-mode", "bypassPermissions"])),
            effort: Some(effort(
                &["low", "medium", "high", "xhigh", "max"],
                &["--effort", "{value}"],
            )),
            ..Default::default()
        },
    );

    // codex: `codex exec` is non-interactive. The `yolo` mode mirrors the
    // known-agent profile table: `--sandbox danger-full-access --skip-git-repo-check
    // -c approval_policy="never"`. `-c approval_policy="never"` replaced the
    // older `-a never` short flag, which codex-cli no longer accepts.

    // Its session block is the §FS-rhei-snapshots.9.2 row: `resume` as a
    // `codex exec` subcommand and no `fork` (§FS-rhei-snapshots.9.3.4),
    // `--ephemeral` off, no `session_dir_flag`, so §FS-rhei-snapshots.9.1.1 finds the rollout.

    // Interactive continuation is the top-level `codex`, not `codex exec`.
    // §FS-rhei-agents.2: Built-in codex profile.
    agents.insert(
        "codex".to_string(),
        CustomAgentProfile {
            command: flags(&["codex", "exec"]),
            prompt_flag: None,
            model_flag: Some("--model".to_string()),
            stdin_prompt: true,
            mcp_flag: Some("--mcp".to_string()),
            modes: modes_yolo_only(flags(&[
                "--sandbox",
                "danger-full-access",
                "--skip-git-repo-check",
                "-c",
                "approval_policy=\"never\"",
            ])),
            effort: Some(effort(
                &["minimal", "low", "medium", "high", "xhigh"],
                &["-c", "model_reasoning_effort=\"{value}\""],
            )),
            session: Some(serde_json::json!({
                "resume": {"flag": "resume"},
                "interactive": {"command": ["codex"]},
                "no_session_flag": "--ephemeral",
                "layout": {
                    "kind": "FlatById",
                    "dir_template": "~/.codex/sessions",
                    "ext": "jsonl",
                    "nested": true,
                    "id_from_stem": "trailing_uuid",
                    "confirm_cwd_path": ["payload", "cwd"]
                }
            })),
            ..Default::default()
        },
    );

    // gemini: `--approval-mode yolo` is the autonomous posture
    // (`auto_edit` still prompts on shell tool calls).
    agents.insert(
        "gemini".to_string(),
        CustomAgentProfile {
            command: flags(&["gemini"]),
            prompt_flag: Some("--prompt".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--approval-mode", "yolo"])),
            ..Default::default()
        },
    );

    // kilocode: `kilo --auto "<prompt>"` is the documented CI invocation;
    // `--yolo` auto-approves tool permissions. `--auto` takes the prompt
    // as its argument, so it maps onto `prompt_flag`.
    agents.insert(
        "kilocode".to_string(),
        CustomAgentProfile {
            command: flags(&["kilo"]),
            prompt_flag: Some("--auto".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--yolo"])),
            effort: Some(effort(
                &["minimal", "low", "high", "max"],
                &["--variant", "{value}"],
            )),
            ..Default::default()
        },
    );

    // cursor: the headless binary is `cursor-agent` (distinct from the
    // `cursor` IDE launcher). `-p`/`--print` is the non-interactive flag;
    // `--force` is the auto-approve posture.
    agents.insert(
        "cursor".to_string(),
        CustomAgentProfile {
            command: flags(&["cursor-agent"]),
            prompt_flag: Some("--print".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            modes: modes_yolo_only(flags(&["--force"])),
            ..Default::default()
        },
    );

    // pi (openclaw / badlogic pi-coding-agent). Headless mode exits
    // deterministically after one turn. pi has no permission layer — modes
    // are intentionally empty; isolation is the caller's responsibility
    // (e.g. sandbox/container).

    // §FS-rhei-agents.2: Built-in pi profile.
    agents.insert(
        "pi".to_string(),
        CustomAgentProfile {
            command: flags(&["pi"]),
            prompt_flag: Some("-p".to_string()),
            model_flag: Some("--model".to_string()),
            stdin_prompt: false,
            skill_flag: Some("--skill".to_string()),
            effort: Some(effort(
                &["off", "minimal", "low", "medium", "high", "xhigh"],
                &["--thinking", "{value}"],
            )),
            session: Some(serde_json::json!({
                "resume": {"flag": "--continue"},
                "fork": {"flag": "--fork"},
                "interactive": {},
                "session_dir_flag": "--session-dir",
                "no_session_flag": "--no-session",
                "layout": {"kind": "FlatById", "ext": "jsonl"}
            })),
            ..Default::default()
        },
    );

    agents
}
