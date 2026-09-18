/// Pure preparation: normal task/state safeguards, then complete after-images. §FS-rhei-transition-cmd.6
fn prepare_forced_transition(request: &ForcedRequest<'_>) -> MietteResult<PreparedForce> {
    validate_force_options(Some(request.reason), None, false)?;
    let input = normalize_workspace_input(request.input);
    let loaded = load_plan(&input)?;
    let scope = resolve_rhei_scope(&loaded, request.scope)?;
    let task_id = resolve_cli_task_id(&loaded, request.task, &scope)?;
    let route = loaded.task_route(&task_id, &input);
    let root = rhei_core::platform::canonical_path(&route.execution_root)
        .map_err(|err| file_io_report(&route.execution_root, "failed to resolve recovery root", err))?;
    forced_marker_location(&root)?;
    let resolved = resolve_state_machines_for_loaded_plan(&input, &loaded, request.machine_path)?;
    let machines = ExecutionMachines::build(&resolved, &input, &loaded)?;
    let machine = machines.for_task_str(&task_id);
    let from = request.from;
    let to = request.to;
    for state in [from, to] {
        if !machine.is_valid_state(state) {
            return Err(miette!("'{state}' is not a valid state. Allowed: [{}]", machine.allowed_states().collect::<Vec<_>>().join(", ")));
        }
    }
    let target_id = parse_task_id(&task_id);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id).ok_or_else(|| miette!("task '{task_id}' not found"))?;
    let current = normalized_state_name(&task.state, machine);
    if current != from {
        return Err(miette!("conflict: Task {} is in state '{}', expected '{}'", task_id, task.state, from));
    }
    ensure_task_profile_allows_state(machine, &task_id, &task.kind, parse_task_id(&route.local_id).depth() as u8, to)?;
    let metadata_raw = rhei_core::source::read_to_string(&route.metadata_file)
        .map_err(|err| file_io_report(&route.metadata_file, "failed to read recovery metadata", err))?;
    let task_raw = if route.task_file == route.metadata_file { metadata_raw.clone() } else {
        rhei_core::source::read_to_string(&route.task_file)
            .map_err(|err| file_io_report(&route.task_file, "failed to read recovery task", err))?
    };
    let metadata = parse_metadata_from_raw(&route.metadata_file, &metadata_raw)?;
    let key = parse_task_id(&route.metadata_id);
    let normalized = ensure_current_state_visit_count(metadata.as_ref(), &key, from, &task.state, machine);
    let checked_metadata = normalized.as_ref().or(metadata.as_ref());
    let declared = machine.transitions().iter().find(|rule| rule.from.0 == from && rule.to.0 == to)
        .or_else(|| (!machine.states[from].terminal).then(|| machine.transitions().iter()
            .find(|rule| rule.from.0 == "*" && rule.to.0 == to)).flatten());
    // Declaration is independent of applicability and every task-owned safeguard. §FS-rhei-transition-cmd.6
    let prepared = (|| -> MietteResult<Option<PreparedForce>> {
    if let Some(rule) = declared {
        if !transition_rule_is_applicable(rule, machine, checked_metadata, &key, Some(task), from, &task.state)? {
            let reason = describe_blocked_transition(rule, machine, checked_metadata, &key, from, &task.state);
            return Err(miette!("transition from '{from}' to '{to}' is not currently applicable: {reason}"));
        }
    }
    if let Some(spent) = spent_loop_budget(machine, checked_metadata, &key, from, &task.state, to) {
        return Err(miette!("{spent}"));
    }
    let terminal = machine.states[to].terminal;
    if terminal && declared.is_none() && request.result.is_none_or(|result| result.trim().is_empty()) {
        return Err(miette!("forced entry into terminal state '{to}' requires a fresh non-empty --result"));
    }
    require_non_blank_result(request.result, "transition")?;
    let ancestors = ancestor_chain(&loaded.rhei.tasks, &target_id).into_iter().cloned().collect::<Vec<_>>();
    let owner = nearest_in_scope_supervising_owner(machine, &ancestors);
    // Claim release is deliberately separate from this correction. §FS-rhei-transition-cmd.6
    fn claimed(task: &rhei_core::ast::Task) -> Option<&rhei_core::ast::Task> {
        if task.assignee.is_some() { Some(task) } else { task.children.iter().find_map(claimed) }
    }
    if let Some(claim) = claimed(task).or_else(|| owner.filter(|owner| owner.assignee.is_some())) {
        return Err(miette!("Task {} is assigned to {}; release the claim before forced recovery", claim.id, claim.assignee.as_deref().unwrap_or("?")));
    }
    if !terminal {
        if let Some(ancestor) = ancestors.iter().find(|ancestor| machine.states.get(&normalized_state_name(&ancestor.state, machine)).is_some_and(|state| state.terminal)) {
            return Err(miette!("terminal ancestor {} must be reopened before forcing {} into a non-terminal state", ancestor.id, task_id));
        }
    }
    ensure_descendants_terminal_for_terminal_entry(machine, task, &task_id, &task_id, to, &input)?;
    let mut updated = update_metadata_for_transition(checked_metadata, &key, to, machine).or_else(|| normalized.clone());
    if machine.states[from].poll.is_some() && to != from {
        updated = clear_poll_state_metadata(updated.as_ref().or(checked_metadata), &key, from);
    }
    let from_visit = Some(render_visit_count(checked_metadata, &key, from, &task.state, machine));
    let to_visit = updated.as_ref().map(|meta| task_visit_count(Some(meta), &key, to)).filter(|count| *count > 0);
    let settings = load_merged_settings(&root)?;
    if !rhei_validator::is_cancelled_state_name(to) {
        ensure_state_outputs_exist_for_transition(
            &root,
            Some(task),
            &task_id,
            from,
            &machine.states[from],
            from_visit,
            machine,
            &settings,
            &default_run_options(),
            terminal,
        )?;
    }
    ensure_state_inputs_exist_for_transition(&root, Some(task), &task_id, to, &machine.states[to], to_visit, machine, &settings,
        &format!("Task {task_id} cannot enter state {to}."))?;
    if declared.is_some() {
        ensure_terminal_result_available(machine, &root, &task_id, from, to, request.result, &input)?;
        return Ok(None);
    }
    // The shared pure supervision update includes counted visits and checkpoint delivery.
    // §FS-rhei-transition-cmd.6
    if let Some(next) = apply_supervision_transition(updated.as_ref().or(checked_metadata), SupervisionTransition {
        machine, task, supervising_owner: owner, metadata_key: &key,
        metadata_prefix: route.metadata_id.strip_suffix(&route.local_id).unwrap_or(""),
        local_id: &route.local_id, from, to, to_visit: to_visit.unwrap_or(1), operation_supervisor: None,
    }) { updated = Some(next); }
    let rendered = format_task_state_value(to, to_visit, machine);
    let mut new_task = rewrite_task_for_transition(&task_raw, &route.local_id, &rendered, transition_ends_supervisor_visit(machine, from, to))?;
    if terminal || machine.states[from].terminal {
        new_task = forced_result_link(&new_task, &route.local_id, &task_id, terminal);
    }
    let mut files = Vec::new();
    if route.task_file == route.metadata_file {
        if let Some(metadata) = &updated { new_task = rewrite_frontmatter(&new_task, metadata)?; }
        files.push(forced_file(&root, &route.task_file, &["task", "metadata", "checkpoint"], &forced_newlines(&task_raw, &new_task))?);
    } else {
        let new_metadata = match &updated { Some(metadata) => rewrite_frontmatter(&metadata_raw, metadata)?, None => metadata_raw.clone() };
        files.push(forced_file(&root, &route.metadata_file, &["metadata", "checkpoint"], &forced_newlines(&metadata_raw, &new_metadata))?);
        files.push(forced_file(&root, &route.task_file, &["task"], &forced_newlines(&task_raw, &new_task))?);
    }
    if let Some(result) = request.result {
        let path = root.join(format!("runtime/results/{task_id}.md"));
        let mut bytes = ForcedImage::read(&path)?.bytes()?.unwrap_or_default();
        bytes.extend_from_slice(format!("## Result\n\n{result}\n\n").as_bytes());
        files.push(forced_file(&root, &path, &["result"], &bytes)?);
    }
    if files.iter().any(|file| file.owner == Some(ForcedOwner::BasinProjectMetadata)) {
        for file in &mut files { file.owner.get_or_insert(ForcedOwner::ExecutionRoot); }
    }
    files.sort_by(|left, right| left.key().cmp(&right.key()));
    let roots = forced_owner_roots(&root, &files)?;
    Ok(Some(PreparedForce { root, roots, task_id, files }))
    })();
    match prepared {
        Ok(Some(prepared)) => Ok(prepared),
        Ok(None) => Err(miette!("the state machine already declares this edge; drop --force")),
        Err(error) if declared.is_some() => Err(miette!("{error}. --force does not bypass safeguards on declared edges")),
        Err(error) => Err(error),
    }
}

/// Rewriters operate on lines; images keep the source newline convention. §FS-rhei-recover.2
fn forced_newlines(original: &str, rewritten: &str) -> Vec<u8> {
    let normalized = rewritten.replace("\r\n", "\n");
    if original.contains("\r\n") { normalized.replace('\n', "\r\n").into_bytes() }
    else { normalized.into_bytes() }
}
