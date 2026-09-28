/// The built-in agent ids, in the order a diagnostic lists them. A profile's
/// `family` must be one of these, and a built-in agent is its own family.
// §FS-rhei-agents.1.1.2: An unknown family is a settings error.
const BUILT_IN_AGENT_FAMILIES: [&str; 6] =
    ["claude-code", "codex", "gemini", "cursor", "kilocode", "pi"];

/// Every key an `agents.<id>` entry may carry. A key outside this list loads
/// and does nothing, so it earns a warning rather than a refusal.
// §FS-rhei-agents.1.1.2: An unknown key is a warning naming the known fields.
const AGENT_PROFILE_FIELDS: [&str; 14] = [
    "family",
    "command",
    "prompt_flag",
    "model_flag",
    "stdin_prompt",
    "intervene_stdin",
    "timeout",
    "mcp_flag",
    "mcp_config_flag",
    "skill_flag",
    "deny_read",
    "modes",
    "effort",
    "session",
];

/// The built-in a profile behaves as: what it declared, or its own id.
///
/// This is the one place the rule lives. Inheritance has already run by the
/// time anything calls it, so a caller reads a complete profile and never
/// resolves a family a second time. §FS-rhei-agents.1.1.2
fn resolved_agent_family<'a>(profile: &'a CustomAgentProfile, id: &'a str) -> &'a str {
    profile.family.as_deref().unwrap_or(id)
}

/// The known families, as a diagnostic names them. §FS-rhei-agents.1.1.2
fn known_agent_families() -> String {
    BUILT_IN_AGENT_FAMILIES.join(", ")
}

/// Apply family inheritance once, after the merge has produced the effective
/// registry.
///
/// The oracle for "did this entry set the field" is the set of keys the entry
/// actually wrote, which the merge already keeps for roster provenance — so a
/// written `false` beats an inherited `true` without any field changing type.
/// `modes`, `effort`, `session`, and `deny_read` are replaced whole. Every
/// field taken from the family joins the entry's supplied set, because
/// `rhei roster` reports such a profile resolved.
/// §FS-rhei-agents.1.3 §FS-rhei-agents.1.1.2 §FS-rhei-agents.1.1.7
fn apply_agent_family_inheritance(
    agents: &mut BTreeMap<String, CustomAgentProfile>,
    agent_fields: &mut BTreeMap<String, BTreeSet<String>>,
) {
    // A family is always the pristine built-in, never another entry, so there
    // is no chain to follow and no order to get right.
    let built_ins = built_in_agents();
    let declared = agents
        .iter()
        .filter_map(|(id, profile)| profile.family.clone().map(|family| (id.clone(), family)))
        .collect::<Vec<_>>();
    for (id, family) in declared {
        // An unrecognized family inherits nothing; `validate_intrinsic_settings`
        // is what reports it, in the same shape as an empty `command`.
        let Some(parent) = built_ins.get(&family) else { continue };
        let written = agent_fields.get(&id).cloned().unwrap_or_default();
        let profile = agents.get_mut(&id).expect("the id was just read from this map");
        let mut supplied = written.clone();
        let mut inherit = |field: &str, present: bool| {
            let take = !written.contains(field);
            if take && present {
                supplied.insert(field.to_string());
            }
            take
        };

        if inherit("command", !parent.command.is_empty()) {
            profile.command = parent.command.clone();
        }
        if inherit("prompt_flag", parent.prompt_flag.is_some()) {
            profile.prompt_flag = parent.prompt_flag.clone();
        }
        if inherit("model_flag", parent.model_flag.is_some()) {
            profile.model_flag = parent.model_flag.clone();
        }
        if inherit("stdin_prompt", parent.stdin_prompt) {
            profile.stdin_prompt = parent.stdin_prompt;
        }
        if inherit("intervene_stdin", parent.intervene_stdin) {
            profile.intervene_stdin = parent.intervene_stdin;
        }
        if inherit("timeout", parent.timeout.is_some()) {
            profile.timeout = parent.timeout.clone();
        }
        if inherit("mcp_flag", parent.mcp_flag.is_some()) {
            profile.mcp_flag = parent.mcp_flag.clone();
        }
        if inherit("mcp_config_flag", parent.mcp_config_flag.is_some()) {
            profile.mcp_config_flag = parent.mcp_config_flag.clone();
        }
        if inherit("skill_flag", parent.skill_flag.is_some()) {
            profile.skill_flag = parent.skill_flag.clone();
        }
        if inherit("deny_read", parent.deny_read.is_some()) {
            profile.deny_read = parent.deny_read.clone();
        }
        if inherit("modes", !parent.modes.is_empty()) {
            profile.modes = parent.modes.clone();
        }
        if inherit("effort", parent.effort.is_some()) {
            profile.effort = parent.effort.clone();
        }
        if inherit("session", parent.session.is_some()) {
            profile.session = parent.session.clone();
        }
        agent_fields.insert(id, supplied);
    }
}

/// Warn once per key that an `agents.<id>` entry carries a key that is not a
/// field. Such a key loads today and does nothing — the shape that let a
/// wrapped profile's `autonomous_args` be read and discarded in silence — so
/// refusing it would reject files that load now.
// §FS-rhei-agents.1.1.2: An unknown key is a warning, not a refusal.
fn warn_unknown_agent_profile_keys(id: &str, raw: &serde_json::Value) {
    let Some(entry) = raw.as_object() else { return };
    for key in entry.keys() {
        if AGENT_PROFILE_FIELDS.contains(&key.as_str()) || !claim_unknown_agent_key_warning(id, key)
        {
            continue;
        }
        eprintln!(
            "warning: agents.{id} declares '{key}', which is not a field of an agent profile \
             and is ignored. The fields are {}.",
            AGENT_PROFILE_FIELDS.join(", ")
        );
    }
}

/// Settings are merged many times in one process; the operator is told once.
fn claim_unknown_agent_key_warning(id: &str, key: &str) -> bool {
    static SEEN: std::sync::OnceLock<Mutex<BTreeSet<String>>> = std::sync::OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(BTreeSet::new()))
        .lock()
        .map(|mut seen| seen.insert(format!("{id}.{key}")))
        .unwrap_or(false)
}
