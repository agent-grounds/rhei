// Required artifact checks share execution identities with readiness and carry
// those identities into handoff composition; the actual invocation is checked
// before prompt composition. §AR-source-file-size.3

// §FS-rhei-plan-language.3.13 §FS-rhei-validate.4

fn exclusion_applicable_states<'a>(
    task: &rhei_core::ast::Task,
    machine: &'a rhei_validator::StateMachine,
) -> Vec<&'a str> {
    machine
        .profile_for_node(&task.kind, task.profile_level())
        .map(|profile| profile.allowed.iter().map(String::as_str).collect())
        .unwrap_or_else(|| machine.states.keys().map(String::as_str).collect())
}

/// Counted loops without a budget have no last visit. Besides the current
/// visit, check numbers named by the exclusion: a future visit-specific path
/// can overlap only at one of these numbers (including embedded digit runs).
/// Bounded states check every permitted visit, including canonical aliases.
fn exclusion_visit_candidates(
    policy: &ResolvedExclusions,
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    state: &str,
) -> BTreeSet<u64> {
    let current = render_visit_count(metadata, &task.id, state, task.state.as_str(), machine);
    if let Some(limit) = state_visit_limit(machine, state) {
        return (1..=limit.max(current)).collect();
    }
    let mut visits = BTreeSet::from([1, current]);
    if state_counts_visits(machine, state) {
        for entry in &policy.entries {
            for path in [&entry.logical, &entry.canonical] {
                for digits in path.to_string_lossy().split(|c: char| !c.is_ascii_digit()) {
                    for start in 0..digits.len() {
                        for end in start + 1..=digits.len() {
                            if let Ok(visit) = digits[start..end].parse::<u64>() {
                                if visit > 0 {
                                    visits.insert(visit);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    visits
}

/// Existing visit-named symlinks can resolve to an exclusion with no number in
/// its spelling. Inspect names at the first visit placeholder as well as the
/// authored/canonical exclusion names; no artifact content is read here.
#[allow(clippy::too_many_arguments)]
fn exclusion_artifact_visits(
    policy: &ResolvedExclusions,
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    state: &str,
    root: &Path,
    artifact: &rhei_validator::StateArtifactDef,
    identity: TransitionInvocationContext<'_>,
) -> BTreeSet<u64> {
    let mut visits = exclusion_visit_candidates(policy, task, machine, metadata, state);
    if state_visit_limit(machine, state).is_some() || !state_counts_visits(machine, state) {
        return visits;
    }
    let (target, model, provider, model_name, agent, mode) = identity;
    let (_, path) = resolve_artifact_path(
        root,
        artifact,
        &task.id.to_string(),
        state,
        None,
        target,
        model,
        provider,
        model_name,
        agent,
        mode,
    );
    let mut parent = PathBuf::new();
    for component in path.components() {
        let component = component.as_os_str().to_string_lossy();
        let Some((prefix, _)) = component.split_once("{visit_count}") else {
            parent.push(component.as_ref());
            continue;
        };
        if let Ok(entries) = fs::read_dir(&parent) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let Some(tail) = name.strip_prefix(prefix) else { continue };
                let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
                for end in 1..=digits.len() {
                    if let Ok(visit) = digits[..end].parse::<u64>() {
                        if visit > 0
                            && component.replace("{visit_count}", &visit.to_string()) == name
                        {
                            visits.insert(visit);
                        }
                    }
                }
            }
        }
        break;
    }
    visits
}

#[allow(clippy::too_many_arguments)]
fn check_required_exclusion_artifact(
    policy: &ResolvedExclusions,
    root: &Path,
    task: &rhei_core::ast::Task,
    state: &str,
    visit: u64,
    identity: TransitionInvocationContext<'_>,
    artifact: &rhei_validator::StateArtifactDef,
    kind: &str,
    errors: &mut Vec<String>,
) {
    let (target, model, provider, model_name, agent, mode) = identity;
    let (_, path) = resolve_artifact_path(
        root,
        artifact,
        &task.id.to_string(),
        state,
        Some(visit),
        target,
        model,
        provider,
        model_name,
        agent,
        mode,
    );
    if !policy.allows(&path) {
        let error = format!(
            "Task {} excludes required {kind} '{}' at '{}' (state '{state}', visit {visit})",
            task.id,
            artifact.name,
            path.display()
        );
        if !errors.contains(&error) {
            errors.push(error);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_exclusion_requirements(
    policy: &mut ResolvedExclusions,
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    opts: &RunOptions,
    root: &Path,
    metadata: Option<&Metadata>,
) -> Result<(), Vec<String>> {
    let states = exclusion_applicable_states(task, machine);
    let mut errors = Vec::new();
    for state in &states {
        match resolve_agent_invocations_for_task(machine, state, settings, opts, Some(task)) {
            Ok(invocations) => {
                policy.invocations.insert((*state).to_string(), invocations);
            }
            Err(error) => errors.push(format!(
                "Task {} exclusion requirements in state '{state}': {error}",
                task.id
            )),
        }
    }
    for state in &states {
        let Some(def) = machine.states.get(*state) else { continue };
        let invocations = policy.invocations.get(*state).map(Vec::as_slice).unwrap_or_default();
        for identity in transition_contexts_for_state(def, invocations) {
            for input in def.inputs.iter().filter(|input| !input.optional) {
                for visit in exclusion_artifact_visits(
                    policy, task, machine, metadata, state, root, input, identity,
                ) {
                    check_required_exclusion_artifact(
                        policy,
                        root,
                        task,
                        state,
                        visit,
                        identity,
                        input,
                        "input",
                        &mut errors,
                    );
                }
            }
        }
        let Some(handoff) = &def.handoff else { continue };
        for inherit in handoff.inherit.iter().filter(|inherit| inherit.required) {
            for source in &states {
                if !machine.transitions.iter().any(|rule| {
                    (rule.to.0 == *state || rule.to.0 == "*")
                        && (rule.from.0 == *source || rule.from.0 == "*")
                }) {
                    continue;
                }
                let Some(source_def) = machine.states.get(*source) else { continue };
                let invocations =
                    policy.invocations.get(*source).map(Vec::as_slice).unwrap_or_default();
                for identity in transition_contexts_for_state(source_def, invocations) {
                    for output in source_def.outputs.iter().filter(|output| {
                        output.kind.as_deref() == Some("handoff")
                            && inherit.name.as_ref().is_none_or(|name| name == &output.name)
                    }) {
                        for visit in exclusion_artifact_visits(
                            policy, task, machine, metadata, source, root, output, identity,
                        ) {
                            check_required_exclusion_artifact(
                                policy,
                                root,
                                task,
                                source,
                                visit,
                                identity,
                                output,
                                "handoff",
                                &mut errors,
                            );
                        }
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Both serial/pool dispatch (including retries and fan-out) and manual next
/// enter here with the actual visit and resolved execution identity.
// §FS-rhei-agents.5.2.1 §FS-rhei-agents.5.2.2 §FS-rhei-next.3.1
fn validate_invocation_exclusions(context: &RuntimeTemplateContext<'_>) -> MietteResult<()> {
    let Some(memory) = context.memory else { return Ok(()) };
    let policy = &memory.exclusions;
    if policy.entries.is_empty() {
        return Ok(());
    }
    let Some(def) = context.machine.states.get(context.state_name) else { return Ok(()) };
    let mut errors = Vec::new();
    if let Some(source) = context.machine.prompt_template_source(def) {
        if !policy.allows(source) {
            errors.push(format!(
                "Task {} excludes required prompt-template source '{}'",
                context.task.id,
                source.display()
            ));
        }
    }
    let visit = render_visit_count(
        context.metadata,
        &context.task.id,
        context.state_name,
        context.current_state_raw,
        context.machine,
    );
    let identity = (
        context.target,
        context.model,
        context.model_provider,
        context.model_name,
        context.agent,
        context.agent_mode,
    );
    for input in def.inputs.iter().filter(|input| !input.optional) {
        check_required_exclusion_artifact(
            policy,
            context.workspace_root,
            context.task,
            context.state_name,
            visit,
            identity,
            input,
            "input",
            &mut errors,
        );
    }
    if let Some(handoff) = &def.handoff {
        if let Some(source) = last_recorded_source_state_for_current(
            context.workspace_root,
            &context.task.id,
            context.state_name,
            context.machine,
        )? {
            if let Some(source_def) = context.machine.states.get(&source) {
                let visit = render_visit_count(
                    context.metadata,
                    &context.task.id,
                    &source,
                    &source,
                    context.machine,
                );
                for inherit in handoff.inherit.iter().filter(|inherit| inherit.required) {
                    for identity in source_handoff_contexts(context, source_def, &source) {
                        for output in source_def.outputs.iter().filter(|output| {
                            output.kind.as_deref() == Some("handoff")
                                && inherit.name.as_ref().is_none_or(|name| name == &output.name)
                        }) {
                            check_required_exclusion_artifact(
                                policy,
                                context.workspace_root,
                                context.task,
                                &source,
                                visit,
                                identity,
                                output,
                                "handoff",
                                &mut errors,
                            );
                        }
                    }
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(exclusion_report(errors))
    }
}
