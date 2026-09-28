/// The two `metadata.tasks.<id>` keys this file writes, named by the register of
/// the keys rhei writes. §FS-rhei-transitions.2.5
const STATE_VISITS_KEY: &str = rhei_core::metadata::STATE_VISITS_KEY;
const POLL_NEXT_ATTEMPT_AT_KEY: &str = rhei_core::metadata::POLL_NEXT_ATTEMPT_AT_KEY;

fn render_frontmatter_yaml(metadata: &Metadata) -> MietteResult<String> {
    let mut rendered = serde_yaml::to_string(metadata)
        .map_err(|err| miette!(
            help = internal_error_help(),
            "failed to serialize frontmatter: {err}"
        ))?;
    if let Some(stripped) = rendered.strip_prefix("---\n") {
        rendered = stripped.to_string();
    }
    Ok(rendered.trim_end().to_string())
}

fn rewrite_frontmatter(raw: &str, metadata: &Metadata) -> MietteResult<String> {
    let lines = raw.lines().collect::<Vec<_>>();
    // Both manifest forms carry frontmatter in the same position, and the
    // Panta manifest owns basin tickets' runtime metadata
    // (§FS-rhei-panta.6.1), so it must be rewritable too.
    let header_index = lines
        .iter()
        .position(|line| {
            let line = line.trim_start();
            line.starts_with("# Rhei:") || line.starts_with("# Panta:")
        })
        .ok_or_else(|| {
            miette!(
                help = plan_authoring_help(),
                "could not find a '# Rhei:' or '# Panta:' header when rewriting frontmatter"
            )
        })?;

    let mut idx = header_index + 1;
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    if idx < lines.len() && lines[idx].trim_start().starts_with("**States:**") {
        idx += 1;
    }
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }

    let start = idx;
    let mut end = idx;
    if start < lines.len() && lines[start].trim() == "---" {
        end += 1;
        while end < lines.len() && lines[end].trim() != "---" {
            end += 1;
        }
        if end == lines.len() {
            return Err(miette!(
                help = plan_authoring_help(),
                "unterminated YAML frontmatter in plan source"
            ));
        }
        end += 1;
        while end < lines.len() && lines[end].trim().is_empty() {
            end += 1;
        }
    }

    let mut result = Vec::with_capacity(lines.len() + 8);
    result.extend(lines[..start].iter().map(|line| (*line).to_string()));
    result.push("---".to_string());
    let rendered_yaml = render_frontmatter_yaml(metadata)?;
    if !rendered_yaml.is_empty() {
        result.extend(rendered_yaml.lines().map(|line| line.to_string()));
    }
    result.push("---".to_string());
    result.push(String::new());
    result.extend(lines[end..].iter().map(|line| (*line).to_string()));

    let mut output = result.join("\n");
    if raw.ends_with('\n') || !output.is_empty() {
        output.push('\n');
    }
    Ok(output)
}

fn ensure_mapping(parent: &mut YamlMapping, key: YamlValue) -> &mut YamlMapping {
    if !matches!(parent.get(&key), Some(YamlValue::Mapping(_))) {
        parent.insert(key.clone(), YamlValue::Mapping(YamlMapping::new()));
    }

    match parent.get_mut(&key) {
        Some(YamlValue::Mapping(mapping)) => mapping,
        _ => unreachable!("mapping just initialized"),
    }
}

fn ensure_current_state_visit_count(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    current_state: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<Metadata> {
    if !state_counts_visits(machine, current_state) {
        return None;
    }

    let current =
        current_state_visit_count(existing, task_id, current_state, current_state_raw, machine);
    if current == task_visit_count(existing, task_id, current_state) {
        return existing.cloned();
    }

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let state_visits = ensure_mapping(task_entry, yaml_key(STATE_VISITS_KEY));
    state_visits.insert(yaml_key(current_state), yaml_u64(current));
    Some(root)
}

fn update_metadata_for_transition(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    to_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<Metadata> {
    if !state_counts_visits(machine, to_state) {
        return None;
    }

    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let state_visits = ensure_mapping(task_entry, yaml_key(STATE_VISITS_KEY));
    let state_key = yaml_key(to_state);
    let next =
        state_visits.get(&state_key).and_then(yaml_value_to_u64).map(|n| n.max(1) + 1).unwrap_or(1);
    state_visits.insert(state_key, yaml_u64(next));
    Some(root)
}

/// Drop one of rhei's own keys from every task's metadata entry.
///
/// Which keys a caller passes is the register's business, not this function's:
/// `rhei reset` reads them off the `Cleared by rhei reset` column so that a
/// runtime key added later is cleared by adding its row.
// §FS-rhei-transitions.2.5 §FS-rhei-reset.2
fn clear_rhei_written_key(existing: Option<&Metadata>, key: &str) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };

    for value in tasks.values_mut() {
        if let YamlValue::Mapping(task_map) = value {
            task_map.remove(yaml_key(key));
        }
    }

    Some(root)
}

fn set_poll_next_attempt_metadata(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
    next_attempt_at: u64,
    next_attempt_count: u64,
) -> Metadata {
    let mut root = existing.cloned().unwrap_or_default();
    let metadata_section = ensure_mapping(&mut root, yaml_key("metadata"));
    let tasks = ensure_mapping(metadata_section, yaml_key("tasks"));
    let task_entry = ensure_mapping(tasks, task_id_yaml_key(task_id));
    let poll_next = ensure_mapping(task_entry, yaml_key(POLL_NEXT_ATTEMPT_AT_KEY));
    poll_next.insert(yaml_key(state_name), yaml_u64(next_attempt_at));
    let state_visits = ensure_mapping(task_entry, yaml_key(STATE_VISITS_KEY));
    state_visits.insert(yaml_key(state_name), yaml_u64(next_attempt_count.max(1)));
    root
}

fn clear_poll_state_metadata(
    existing: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<Metadata> {
    let mut root = existing.cloned()?;
    let Some(YamlValue::Mapping(metadata_section)) = root.get_mut(yaml_key("metadata")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(tasks)) = metadata_section.get_mut(yaml_key("tasks")) else {
        return Some(root);
    };
    let Some(YamlValue::Mapping(task_entry)) = tasks.get_mut(task_id_yaml_key(task_id)) else {
        return Some(root);
    };
    if let Some(YamlValue::Mapping(poll_next)) = task_entry.get_mut(yaml_key(POLL_NEXT_ATTEMPT_AT_KEY)) {
        poll_next.remove(yaml_key(state_name));
    }
    if task_entry
        .get(yaml_key(POLL_NEXT_ATTEMPT_AT_KEY))
        .and_then(YamlValue::as_mapping)
        .is_some_and(YamlMapping::is_empty)
    {
        task_entry.remove(yaml_key(POLL_NEXT_ATTEMPT_AT_KEY));
    }
    if let Some(YamlValue::Mapping(state_visits)) = task_entry.get_mut(yaml_key(STATE_VISITS_KEY)) {
        state_visits.remove(yaml_key(state_name));
    }
    if task_entry
        .get(yaml_key(STATE_VISITS_KEY))
        .and_then(YamlValue::as_mapping)
        .is_some_and(YamlMapping::is_empty)
    {
        task_entry.remove(yaml_key(STATE_VISITS_KEY));
    }
    Some(root)
}
