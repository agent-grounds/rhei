struct RuntimeTemplateContext<'a> {
    workspace_root: &'a Path,
    /// Execution root of every ticket in the loaded project, keyed by qualified
    /// id. `None` outside a Panta project, where one root owns every ticket.
    /// Lets prompt composition read a *prior's* artifacts from the rhei that
    /// owns them.
    // §FS-rhei-panta.6.1: artifacts live under the owning rhei's root.
    task_roots: Option<&'a HashMap<String, PathBuf>>,
    /// The loaded plan's task roots, when the caller has them. Prompt
    /// composition walks them to name the supervising ancestor whose brief it
    /// is rendering; nothing else needs the tree.
    // §FS-rhei-supervision.5.2
    plan_tasks: Option<&'a [rhei_core::ast::Task]>,
    checkout_root: &'a Path,
    plan_path: &'a Path,
    state_machine_path: Option<&'a Path>,
    plan_title: &'a str,
    task: &'a rhei_core::ast::Task,
    state_name: &'a str,
    current_state_raw: &'a str,
    machine: &'a rhei_validator::StateMachine,
    metadata: Option<&'a Metadata>,
    target: Option<&'a ExecutionTarget>,
    model: Option<&'a str>,
    model_provider: Option<&'a str>,
    model_name: Option<&'a str>,
    agent: Option<&'a str>,
    agent_mode: Option<&'a str>,
    /// Resolved MCP servers and skills for the current state (Half A).
    /// Availability here reflects registry resolution only; Half B will
    /// overlay real handshake results.
    tooling: Option<&'a ResolvedTooling>,
    /// The project's mid-term memory, when the caller loaded it. `None` on the
    /// template-only contexts a program state builds, which render no prompt.
    // §FS-rhei-memory.4.1
    memory: Option<&'a PromptMemory>,
}

fn yaml_value_to_template_string(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::Null => Some(String::new()),
        YamlValue::Bool(value) => Some(value.to_string()),
        YamlValue::Number(value) => Some(value.to_string()),
        YamlValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn render_visit_count(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
    current_state_raw: &str,
    machine: &rhei_validator::StateMachine,
) -> u64 {
    let visit =
        current_state_visit_count(metadata, task_id, state_name, current_state_raw, machine);
    visit.max(1)
}

#[allow(clippy::too_many_arguments)]
fn artifact_relative_path(
    artifact: &rhei_validator::StateArtifactDef,
    task_id: &str,
    state_name: &str,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
) -> String {
    let mut relative = artifact.path.replace("{task_id}", task_id).replace("{state}", state_name);
    if let Some(visit_count) = visit_count {
        relative = relative.replace("{visit_count}", &visit_count.to_string());
    }
    if let Some(target) = target {
        relative = relative.replace("{target}", &target.selector());
        relative = relative.replace("{target.slug}", &target.slug());
    }
    if let Some(provider) = model_provider.or_else(|| target.and_then(|target| target.provider.as_deref())) {
        relative = relative.replace("{model.provider}", provider);
    }
    if let Some(model) = model {
        relative = relative.replace("{model}", model);
    }
    if let Some(model_name) = model_name.or(model) {
        relative = relative.replace("{model.name}", model_name);
    }
    if let Some(agent) = agent {
        relative = relative.replace("{agent}", agent);
    }
    if let Some(agent_mode) = agent_mode {
        relative = relative.replace("{agent.mode}", agent_mode);
    }
    relative
}

#[allow(clippy::too_many_arguments)]
fn resolve_artifact_path(
    workspace_root: &Path,
    artifact: &rhei_validator::StateArtifactDef,
    task_id: &str,
    state_name: &str,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
) -> (String, PathBuf) {
    let relative = artifact_relative_path(
        artifact,
        task_id,
        state_name,
        visit_count,
        target,
        model,
        model_provider,
        model_name,
        agent,
        agent_mode,
    );
    (relative.clone(), workspace_root.join(&relative))
}

/// How one runtime path is spelled for the worker that reads it: relative to
/// the artifact root while that root is also the working directory, absolute
/// under `root` once the checkout is somewhere else.
///
/// Declared artifacts, the export path and the result path all come through
/// here, because the rule only works if a prompt applies it to all of them. One
/// bare relative path beside absolute neighbours is worse than none: it teaches
/// the worker that a path in this prompt is relative to its own cwd, and that
/// reading writes the file into the checkout, where nothing looks for it.
// §FS-rhei-agents.4.1
fn prompt_path_shown(
    context: &RuntimeTemplateContext<'_>,
    root: &Path,
    relative: String,
) -> String {
    if roots_are_one_directory(context.checkout_root, context.workspace_root) {
        relative
    } else {
        // Through `spelled_path`, like every other absolute path in the
        // prompt: a root that arrives canonicalized carries Windows's `\\?\`
        // prefix, which no shell the worker pastes it into wants.
        spelled_path(&root.join(relative))
    }
}

fn render_artifact_template_path(
    context: &RuntimeTemplateContext<'_>,
    artifact: &rhei_validator::StateArtifactDef,
    visit_count: Option<u64>,
) -> String {
    let (relative, _) = resolve_artifact_path(
        context.workspace_root,
        artifact,
        &context.task.id.to_string(),
        context.state_name,
        visit_count,
        context.target,
        context.model,
        context.model_provider,
        context.model_name,
        context.agent,
        context.agent_mode,
    );
    prompt_path_shown(context, context.workspace_root, relative)
}

fn artifact_relative_path_escapes_root(relative: &str) -> bool {
    let mut depth = 0usize;
    for component in Path::new(relative).components() {
        match component {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => return true,
            std::path::Component::ParentDir => {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
        }
    }
    false
}

/// The ticket id as written in its rhei file's heading: the project-qualified
/// id with the qualification prefix stripped. Falls back to the full id when
/// the task carries no prefix. §AR-rhei-panta.3
fn rhei_local_id_of(task: &rhei_core::ast::Task) -> String {
    let prefix = task.profile_depth_offset as usize;
    if prefix == 0 || task.id.segments.len() <= prefix {
        return task.id.to_string();
    }
    task.id.segments[prefix..].iter().map(|s| s.to_string()).collect::<Vec<_>>().join(".")
}

/// The owning rhei's id (the qualification prefix), when the task carries one.
fn rhei_id_of(task: &rhei_core::ast::Task) -> Option<String> {
    let prefix = task.profile_depth_offset as usize;
    if prefix == 0 || task.id.segments.len() <= prefix {
        return None;
    }
    Some(task.id.segments[..prefix].iter().map(|s| s.to_string()).collect::<Vec<_>>().join("."))
}

fn resolve_runtime_template_variable(
    variable: &str,
    context: &RuntimeTemplateContext<'_>,
) -> Option<String> {
    match variable {
        "task_id" => Some(context.task.id.to_string()),
        // §AR-rhei-panta.3: the qualification prefix is never authored into
        // task headings, so instructions that author or grep headings need
        // the rhei-local id and the owning rhei's id as separate variables.
        "task_id_local" => Some(rhei_local_id_of(context.task)),
        "rhei_id" => rhei_id_of(context.task),
        "task_title" => Some(context.task.title.clone()),
        "state" => Some(context.state_name.to_string()),
        "visit_count" => Some(
            render_visit_count(
                context.metadata,
                &context.task.id,
                context.state_name,
                context.current_state_raw,
                context.machine,
            )
            .to_string(),
        ),
        "visits" => state_visit_limit(context.machine, context.state_name).map(|n| n.to_string()),
        "target" => context.target.map(ExecutionTarget::selector),
        "target.slug" => context.target.map(ExecutionTarget::slug),
        "model" => context.model.map(str::to_string),
        "model.provider" => context.model_provider.map(str::to_string),
        "model.name" => context.model_name.or(context.model).map(str::to_string),
        "agent" => context.agent.map(str::to_string),
        "agent.mode" => context.agent_mode.map(str::to_string),
        "plan_title" => Some(context.plan_title.to_string()),
        "plan_path" => Some(context.plan_path.display().to_string()),
        "rhei_root" => Some(context.workspace_root.display().to_string()),
        "checkout_root" => Some(context.checkout_root.display().to_string()),
        _ => {
            // §FS-rhei-gate-brief.7.3
            if let Some(resolved) = gate_answer_variable(variable, context) {
                return resolved;
            }
            if variable.strip_prefix("model.").is_some() {
                return None;
            }
            if let Some(key) = variable.strip_prefix("meta.") {
                return task_metadata_map(context.metadata, &context.task.id)
                    .and_then(|task_map| task_map.get(yaml_key(key)))
                    .and_then(yaml_value_to_template_string);
            }

            let visit_count = Some(render_visit_count(
                context.metadata,
                &context.task.id,
                context.state_name,
                context.current_state_raw,
                context.machine,
            ));
            let state_def = context.machine.states.get(context.state_name)?;

            if let Some(name) =
                variable.strip_prefix("input.").and_then(|v| v.strip_suffix(".path"))
            {
                return state_def
                    .inputs
                    .iter()
                    .find(|artifact| artifact.name == name)
                    .map(|artifact| render_artifact_template_path(context, artifact, visit_count));
            }

            if let Some(name) =
                variable.strip_prefix("input.").and_then(|v| v.strip_suffix(".exists"))
            {
                return state_def.inputs.iter().find(|artifact| artifact.name == name).map(
                    |artifact| {
                        let (_, path) = resolve_artifact_path(
                            context.workspace_root,
                            artifact,
                            &context.task.id.to_string(),
                            context.state_name,
                            visit_count,
                            context.target,
                            context.model,
                            context.model_provider,
                            context.model_name,
                            context.agent,
                            context.agent_mode,
                        );
                        path.exists().to_string()
                    },
                );
            }

            if let Some(name) =
                variable.strip_prefix("output.").and_then(|v| v.strip_suffix(".path"))
            {
                return state_def
                    .outputs
                    .iter()
                    .find(|artifact| artifact.name == name)
                    .map(|artifact| render_artifact_template_path(context, artifact, visit_count));
            }

            if let Some(name) =
                variable.strip_prefix("mcp.").and_then(|v| v.strip_suffix(".available"))
            {
                return context.tooling.map(|t| t.mcp_available(name).to_string());
            }

            if let Some(id) =
                variable.strip_prefix("skill.").and_then(|v| v.strip_suffix(".available"))
            {
                return context.tooling.map(|t| t.skill_available(id).to_string());
            }

            None
        }
    }
}

/// Evaluate a condition expression for `{if <condition>}` blocks.
///
/// Supported forms: `input.<name>.exists`, `mcp.<name>.available`,
/// `skill.<id>.available`.
fn evaluate_if_condition(condition: &str, context: &RuntimeTemplateContext<'_>) -> bool {
    // §FS-rhei-gate-brief.7.3
    if let Some(resolved) = gate_answer_variable(condition, context) {
        return resolved.as_deref() == Some("true");
    }
    if let Some(name) = condition.strip_prefix("input.").and_then(|s| s.strip_suffix(".exists")) {
        let visit_count = Some(render_visit_count(
            context.metadata,
            &context.task.id,
            context.state_name,
            context.current_state_raw,
            context.machine,
        ));
        if let Some(state_def) = context.machine.states.get(context.state_name) {
            if let Some(artifact) = state_def.inputs.iter().find(|a| a.name == name) {
                let (_, path) = resolve_artifact_path(
                    context.workspace_root,
                    artifact,
                    &context.task.id.to_string(),
                    context.state_name,
                    visit_count,
                    context.target,
                    context.model,
                    context.model_provider,
                    context.model_name,
                    context.agent,
                    context.agent_mode,
                );
                return path.exists();
            }
        }
        return false;
    }

    if let Some(name) = condition.strip_prefix("mcp.").and_then(|s| s.strip_suffix(".available")) {
        return context.tooling.map_or(false, |t| t.mcp_available(name));
    }

    if let Some(id) = condition.strip_prefix("skill.").and_then(|s| s.strip_suffix(".available")) {
        return context.tooling.map_or(false, |t| t.skill_available(id));
    }

    false
}

/// Parse the body of an `{if}` block (text after the opening `{if ...}\n`).
///
/// Returns `(true_branch, optional_false_branch, text_after_endif)`.
/// Tag lines (`{else}`, `{endif}`) are consumed and excluded from all slices.
fn parse_if_block(body: &str) -> (&str, Option<&str>, &str) {
    if let Some(else_pos) = body.find("{else}") {
        let true_branch = &body[..else_pos];
        let after_else_tag = else_pos + "{else}".len();
        let false_start = if body[after_else_tag..].starts_with('\n') {
            after_else_tag + 1
        } else {
            after_else_tag
        };
        if let Some(endif_rel) = body[false_start..].find("{endif}") {
            let false_branch = &body[false_start..false_start + endif_rel];
            let after_endif_tag = false_start + endif_rel + "{endif}".len();
            let after_endif = if body[after_endif_tag..].starts_with('\n') {
                &body[after_endif_tag + 1..]
            } else {
                &body[after_endif_tag..]
            };
            return (true_branch, Some(false_branch), after_endif);
        }
        // Malformed: {else} but no {endif} — treat whole body as true branch
        return (body, None, "");
    }

    if let Some(endif_pos) = body.find("{endif}") {
        let true_branch = &body[..endif_pos];
        let after_endif_tag = endif_pos + "{endif}".len();
        let after_endif = if body[after_endif_tag..].starts_with('\n') {
            &body[after_endif_tag + 1..]
        } else {
            &body[after_endif_tag..]
        };
        return (true_branch, None, after_endif);
    }

    // Malformed: no {endif} — treat whole body as true branch
    (body, None, "")
}

/// Collapse runs of three or more consecutive newlines to exactly two.
///
/// Two newlines (`\n\n`) represent a single blank line in prose. When a
/// conditional block is removed, adjacent blank lines from the surrounding text
/// and the removed block merge into a run of 3+; this collapses them back to
/// one blank line so the output stays clean.
fn collapse_extra_blank_lines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut newline_run = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(ch);
            }
        } else {
            newline_run = 0;
            result.push(ch);
        }
    }
    result
}

/// Pre-pass over `text` that resolves `{if condition}…{else}…{endif}` blocks
/// before variable substitution runs.
///
/// Supported conditions (v1): `input.<name>.exists`
/// Nesting is not supported in v1.
fn process_conditional_blocks(text: &str, context: &RuntimeTemplateContext<'_>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(if_start) = remaining.find("{if ") {
        let after_open = if_start + "{if ".len();
        let Some(close_brace) = remaining[after_open..].find('}') else {
            // Malformed opening tag — pass through the '{' and move on
            result.push_str(&remaining[..if_start + 1]);
            remaining = &remaining[if_start + 1..];
            continue;
        };
        let condition = &remaining[after_open..after_open + close_brace];
        let tag_end = after_open + close_brace + 1; // position after '}'

        // Consume the newline that follows the opening tag line
        let body_start = if remaining[tag_end..].starts_with('\n') { tag_end + 1 } else { tag_end };

        // Emit everything before the opening tag unchanged
        result.push_str(&remaining[..if_start]);

        let (true_branch, false_branch, after_endif) = parse_if_block(&remaining[body_start..]);

        if evaluate_if_condition(condition, context) {
            result.push_str(true_branch);
        } else if let Some(fb) = false_branch {
            result.push_str(fb);
        }
        // else: block removed entirely

        remaining = after_endif;
    }

    result.push_str(remaining);
    collapse_extra_blank_lines(&result)
}

fn resolve_runtime_template_text(text: &str, context: &RuntimeTemplateContext<'_>) -> String {
    let preprocessed = process_conditional_blocks(text, context);
    let text = preprocessed.as_str();
    let mut rendered = String::with_capacity(text.len());
    let mut idx = 0usize;

    while idx < text.len() {
        if text[idx..].starts_with("\\{") {
            rendered.push('{');
            idx += 2;
            continue;
        }
        if text[idx..].starts_with("\\}") {
            rendered.push('}');
            idx += 2;
            continue;
        }
        if !text[idx..].starts_with('{') {
            let ch = text[idx..].chars().next().expect("substring should have a char");
            rendered.push(ch);
            idx += ch.len_utf8();
            continue;
        }

        let mut end = idx + 1;
        while end < text.len() && !text[end..].starts_with('}') {
            end += 1;
        }
        if end >= text.len() {
            rendered.push('{');
            idx += 1;
            continue;
        }

        let token = &text[idx + 1..end];
        if let Some(value) = resolve_runtime_template_variable(token, context) {
            rendered.push_str(&value);
        } else {
            rendered.push_str(&text[idx..=end]);
        }
        idx = end + 1;
    }

    rendered
}
