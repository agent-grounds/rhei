
/// Parse a task ID string into a [`TaskId`].
///
/// Accepts both single-segment ids (`1`, `api`) and dotted paths (`1.2`,
/// `api.cache`). Malformed input is treated as a single named segment so
/// downstream lookups fail cleanly with a "not found" message.
fn parse_task_id(s: &str) -> TaskId {
    if s.is_empty() {
        return TaskId::named(s);
    }
    let mut segments = Vec::new();
    for part in s.split('.') {
        if part.is_empty() {
            return TaskId::named(s);
        }
        if let Ok(n) = part.parse::<u32>() {
            segments.push(rhei_core::ast::TaskIdSegment::Number(n));
        } else {
            segments.push(rhei_core::ast::TaskIdSegment::Named(part.to_string()));
        }
    }
    TaskId::from_segments(segments)
}

/// Execute the `next` subcommand: transition the next ready task to the next state,
/// and print the task details with instructions.
fn next_command(
    input: &Path,
    state_machine_path: Option<&Path>,
    task_id_filter: Option<&str>,
    as_json: bool,
    no_callbacks: bool,
    peek: bool,
    rhei_scope: &[String],
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input, &loaded)?;
    let workspace_root = execution_workspace_root(&machines.default_callbacks.plan_path);

    // Validate the plan first.
    let report = rhei_validator::validate_with_machine_set(&loaded.rhei, &machines.set);
    if report.has_errors() {
        return Err(validation_report(
            input,
            &resolved.validation_sources(&loaded.rhei_ids), // §FS-rhei-validate.6
            &report.errors,
            &report.help,
        ));
    }

    // Find the target task to claim. §FS-rhei-panta.6: accept the qualified
    // id or an unambiguous rhei-local shorthand.
    let resolved_filter = task_id_filter
        .map(|tid| resolve_cli_task_id(&loaded, tid, &scope))
        .transpose()?;
    let (task_id_str, current_state_raw, current_state, task_workspace_root) = if let Some(tid) = resolved_filter.as_deref() {
        let target_id = parse_task_id(tid);
        let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
            .ok_or_else(|| {
                miette!(
                    help = format!(
                        "list the task ids in this plan with: rhei list {}",
                        shell_quote(&input.display().to_string())
                    ),
                    "task '{}' not found in the plan",
                    tid
                )
            })?;
        // A non-leaf ticket is a task in its own right, so the only thing that
        // stops a claim is its own subtree still being open. Nothing advances a
        // parent when its children advance, so the refusal names the open
        // descendants rather than describing a cascade that does not exist.

        // §FS-rhei-next.3.4
        // §FS-rhei-supervision.3.2: a supervisor is worked *between* its
        // descendants, so an open subtree is not what stops its claim.
        let open_descendants = if task_is_supervising(task, machines.for_task_str(tid)) {
            Vec::new()
        } else {
            open_descendant_tasks(task, &machines.set)
        };
        if !open_descendants.is_empty() {
            let claimable = narrow_to_rhei_scope(
                find_claimable_tasks(
                    &loaded.rhei,
                    &machines.set,
                    &ReadySetRoots {
                        workspace_root: &workspace_root,
                        task_roots: &loaded.task_roots,
                    },
                ),
                &scope,
            );
            let next_step = match claimable.first() {
                Some(candidate) => format!(
                    "claim what is ready instead: rhei next {} --task {}",
                    shell_quote(&input.display().to_string()),
                    candidate.id
                ),
                None => format!(
                    "finish or cancel the open descendants first, then claim this ticket. \
                     See every task and its state with: rhei list {}",
                    shell_quote(&input.display().to_string())
                ),
            };
            return Err(miette!(
                help = next_step,
                "Task {} cannot be claimed while {} descendant task(s) are still open.\n\
                 Open descendants: {}",
                tid,
                open_descendants.len(),
                format_open_descendants(&open_descendants, &machines.set)
            ));
        }
        // §FS-rhei-supervision.3.4: `rhei next` never claims a descendant of a
        // held supervisor, and says which supervisor holds it rather than
        // leaving the worker to read a stall.
        if let Some(hold) = held_by_supervisor(task, &loaded.rhei, &machines.set) {
            let (supervisor, supervisor_state) = (&hold.supervisor, &hold.state);
            // §FS-rhei-supervision.3.4: the supervisor is the ticket to work,
            // but not if someone already holds it — then the way out is
            // `rhei release`, not a claim that will be refused.
            let holder = find_task_by_id(&loaded.rhei.tasks, supervisor)
                .and_then(|task| task.assignee.clone());
            // §FS-rhei-supervision.3.1: a gate-parked supervisor has no next
            // visit, so pointing the worker at `rhei next` on it is a dead end.
            let next_step = if let Some(holder) = holder {
                format!(
                    "Task {supervisor} is the ticket to work and {holder} holds it; hand it \
                     back with: rhei release {} --task {supervisor}",
                    shell_quote(&input.display().to_string())
                )
            } else if hold.awaiting_human {
                format!(
                    "Task {supervisor} is at a human gate and still holds this subtree. A human \
                     moves it back into its supervising state to resume supervision, or anywhere \
                     else to release the subtree: rhei{} transition {} --task {supervisor} \
                     --from {supervisor_state} --to <state>",
                    state_machine_flag(resolved.default.path.as_deref()),
                    shell_quote(&input.display().to_string())
                )
            } else {
                format!(
                    "the supervisor releases the subtree on its next visit. Work the supervisor \
                     instead: rhei{} next {} --task {supervisor}",
                    state_machine_flag(resolved.default.path.as_deref()),
                    shell_quote(&input.display().to_string())
                )
            };
            return Err(miette!(
                help = next_step,
                "Task {} is held by supervisor Task {} ({})",
                tid,
                supervisor,
                supervisor_state
            ));
        }
        if let Some(assignee) = task.assignee.as_deref() {
            // `rhei release` is the command that owns `**Assignee:**`; telling a
            // worker to hand-edit the line contradicts every other surface,
            // which says the field is CLI-owned. §FS-rhei-release
            let plan_arg = shell_quote(&input.display().to_string());
            return Err(miette!(
                help = format!(
                    "hand it back with: rhei release {plan_arg} --task {tid} — or claim \
                     whatever is ready instead: rhei next {plan_arg}"
                ),
                "Task {} is already assigned to {}",
                tid,
                assignee
            ));
        }
        let machine = machines.for_task_str(tid);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let is_initial = task_is_in_initial_state(task, &state_name, machine);
        if is_initial {
            let mut all_tasks = Vec::new();
            collect_plan_tasks(&loaded.rhei.tasks, &mut all_tasks);
            let state_map = plan_state_map(&all_tasks, &machines.set);
            let all_priors_done = task.prior.iter().all(|dep_id| {
                state_map
                    .get(dep_id)
                    .map(|s| dependency_is_satisfied(s, machines.set.for_task(dep_id)))
                    .unwrap_or(false)
            });
            if !all_priors_done {
                let detail = first_blocking_prior(task, &state_map, &machines.set, &scope)
                    .map(|prior| format!("; waiting on {}", prior))
                    .unwrap_or_default();
                return Err(miette!(
                    help = format!(
                        "finish the prerequisite first, or see what is claimable now: rhei list {}",
                        shell_quote(&input.display().to_string())
                    ),
                    "Task {} is blocked by incomplete prerequisites{}",
                    tid,
                    detail
                ));
            }
        }
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| {
                miette!(help = internal_error_help(), "state '{}' missing from loaded machine", state_name)
            })?;
        let settings = load_merged_settings(&workspace_root)?;
        let task_workspace_root = loaded.task_root(tid, &workspace_root);
        ensure_state_inputs_exist_for_transition(
            &task_workspace_root,
            Some(task),
            tid,
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                machine,
            )),
            machine,
            &settings,
            &format!("Task {} cannot be claimed in state {}.", tid, state_name),
        )?;
        (tid.to_string(), task.state.as_str().to_string(), state_name, task_workspace_root)
    } else {
        // §FS-rhei-panta.6.1: `--rhei` narrows candidates, not prior resolution.
        let ready = narrow_to_rhei_scope(
            find_claimable_tasks(
                &loaded.rhei,
                &machines.set,
                &ReadySetRoots { workspace_root: &workspace_root, task_roots: &loaded.task_roots },
            ),
            &scope,
        );
        if ready.is_empty() {
            return Err(miette!(
                help = "see every task and its state with: rhei list <plan>",
                "{}",
                diagnose_no_claimable(
                    &loaded.rhei,
                    &machines.set,
                    input,
                    resolved.default.path.as_deref(),
                    &scope
                )
            ));
        }
        let task = ready.into_iter().next().unwrap();
        let machine = machines.for_task(&task.id);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let state_def = machine
            .states
            .get(&state_name)
            .ok_or_else(|| {
                miette!(help = internal_error_help(), "state '{}' missing from loaded machine", state_name)
            })?;
        let settings = load_merged_settings(&workspace_root)?;
        let task_workspace_root = loaded.task_root(&task.id.to_string(), &workspace_root);
        ensure_state_inputs_exist_for_transition(
            &task_workspace_root,
            Some(task),
            &task.id.to_string(),
            &state_name,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state_name,
                task.state.as_str(),
                machine,
            )),
            machine,
            &settings,
            &format!("Task {} cannot be claimed in state {}.", task.id, state_name),
        )?;
        (task.id.to_string(), task.state.to_string(), state_name, task_workspace_root)
    };

    // Determine whether we need a state transition.
    // Tasks in an initial state (e.g. draft) are transitioned forward.
    let target_id = parse_task_id(&task_id_str);
    let machine = machines.for_task_str(&task_id_str);
    let callback_paths = machines.callbacks_for_str(&task_id_str);
    let selected_task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| {
            miette!(
                help = format!(
                    "list the task ids in this plan with: rhei list {}",
                    shell_quote(&input.display().to_string())
                ),
                "task '{}' not found in the plan",
                task_id_str
            )
        })?;
    let is_initial = task_is_in_initial_state(selected_task, &current_state, machine);
    let current_state_def = machine
        .states
        .get(&current_state)
        .ok_or_else(|| {
            miette!(help = internal_error_help(), "state '{}' missing from loaded machine", current_state)
        })?;
    // §FS-rhei-next.3: claim initial states in place when the next edge is terminal completion.
    let auto_transition_initial = is_initial
        && !state_declares_autonomous_execution(current_state_def)
        && initial_state_has_non_terminal_forward_transition(selected_task, &loaded.rhei, machine)?;

    let route = loaded.task_route(&task_id_str, input);
    let claim_eligibility = ClaimEligibilityContext {
        input,
        machines: &machines,
        workspace_root: &workspace_root,
    };

    let final_state = if auto_transition_initial && !peek {
        // Advance from a setup-only initial state (for example planning -> pending).
        let target_id = parse_task_id(&task_id_str);
        let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
            .ok_or_else(|| {
                miette!(
                    help = format!(
                        "list the task ids in this plan with: rhei list {}",
                        shell_quote(&input.display().to_string())
                    ),
                    "task '{}' not found in the plan",
                    task_id_str
                )
            })?;
        let to_state = find_next_transition(task, &loaded.rhei, machine)?.ok_or_else(|| {
            miette!(
                help = format!(
                    "no transition leaves '{current_state_raw}'. See the machine's edges with: \
                     rhei states"
                ),
                "no forward transition available from state '{}'",
                current_state_raw
            )
        })?;
        // Gated above, so no *declared* edge lands terminal and there is nothing
        // to carry; an `on_leave` redirect into one is refused on the shared path.
        // §FS-rhei-next.3 §FS-rhei-states.3.3
        execute_claim_transition(
            TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: &task_id_str },
            callback_paths,
            machine,
            &route.local_id,
            &current_state,
            &to_state,
            no_callbacks,
            &claim_eligibility,
        )?
    } else {
        current_state.clone()
    };

    // Re-load to get the updated task for output.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id)
        .ok_or_else(|| {
            miette!(help = internal_error_help(), "task '{}' not found after transition", task_id_str)
        })?;

    // Resolve agent/model for display. `next` should still print the next
    // task even when the state's agent is misconfigured, so demote resolution
    // errors to a stderr warning instead of failing the command outright.
    let settings = load_merged_settings(&workspace_root)?;
    let no_agent_opts = default_run_options();
    let resolved = match resolve_agent_for_task(machine, &final_state, &settings, &no_agent_opts, task) {
        Ok(resolved) => resolved,
        Err(err) => {
            eprintln!(
                "warning: could not resolve agent for state '{}': {}",
                final_state, err
            );
            None
        }
    };
    let agent_id_str = resolved.as_ref().map(|r| r.agent.id().to_string());
    let model_id_str = resolved.as_ref().and_then(|r| r.model.clone());
    let model_provider_str = resolved.as_ref().and_then(|r| r.model_provider.clone());
    let model_name_str = resolved.as_ref().and_then(|r| r.model_name.clone());

    // Claim mode only: write `**Assignee:**` to the task file so a second
    // `rhei next` cannot re-claim the same task. Skipped in peek mode and
    // when the task already has an assignee set.
    let mut claimed_as = if auto_transition_initial && !peek {
        task.assignee.clone()
    } else {
        None
    };
    if !peek && !auto_transition_initial && task.assignee.is_none() {
        let assignee = agent_id_str.as_deref().unwrap_or("manual");
        claimed_as = Some(assignee.to_string());
        write_task_assignee(
            &route.task_file,
            &route.local_id,
            &task_id_str,
            &final_state,
            machine,
            LockedTaskAssigneeClaimContext {
                metadata_file: &route.metadata_file,
                eligibility: &claim_eligibility,
            },
            assignee,
        )?;
    }
    let tooling = resolve_tooling(machine, &final_state, &settings);
    // A manual worker is handed the same memory `rhei run` composes; nothing of
    // a run is in flight here. §FS-rhei-memory.5
    let checkout_root = resolve_agent_checkout_root(&task_workspace_root, &task_id_str)?;
    let exclusions = loaded_task_exclusions(
        &loaded,
        task,
        &task_workspace_root,
        &checkout_root.path,
        &callback_paths.plan_path,
        machine,
        callback_paths.state_machine_path.as_deref(),
        &settings,
        &no_agent_opts,
    )
    .map_err(exclusion_report)?;
    let mut memory =
        prompt_memory(&loaded, input, &workspace_root.join("runtime"), BTreeSet::new());
    memory.exclusions = exclusions;
    // Manual work has no Rhei-owned process boundary. §FS-rhei-next.3.1
    memory.exclusions_filesystem_denied = false;
    // §FS-rhei-memory.4.3: `rhei next` prints neither `## Prior Task Results`
    // nor `## Child Task Results`, so a summary here has nothing to defer to.
    memory.pastes_task_inputs = false;
    // §FS-rhei-memory.3.4: `rhei next` exports no `RHEI_ROOT` and promises the
    // reader no working directory, so a relative path here anchors on nothing.
    memory.absolute_paths = true;
    let render_context = RuntimeTemplateContext {
        workspace_root: &task_workspace_root,
        task_roots: Some(&loaded.task_roots),
        plan_tasks: Some(&loaded.rhei.tasks),
        checkout_root: &checkout_root.path,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: &final_state,
        current_state_raw: task.state.as_str(),
        machine,
        metadata: loaded.rhei.metadata.as_ref(),
        target: resolved.as_ref().and_then(|r| r.target.as_ref()),
        model: model_id_str.as_deref(),
        model_provider: model_provider_str.as_deref(),
        model_name: model_name_str.as_deref(),
        agent: agent_id_str.as_deref(),
        agent_mode: resolved.as_ref().and_then(|r| r.mode.as_deref()),
        tooling: Some(&tooling),
        memory: Some(&memory),
    };
    validate_invocation_exclusions(&render_context)?;
    let instructions = resolve_runtime_template_text(
        state_instructions(machine, &final_state).as_str(),
        &render_context,
    );
    let personality = state_personality(machine, final_state.as_str())
        .map(|text| resolve_runtime_template_text(&text, &render_context));
    // The checkpoints a supervisor is owed and the brief a descendant was
    // written, from the renderers the run prompt uses. §FS-rhei-supervision.3.4
    let checkpoints = render_supervision_checkpoints(&render_context)?;
    let supervisor_brief = render_supervisor_brief(&render_context)?;
    // The mid-term memory sections, from the renderers the run prompt uses, in
    // the run prompt's order. §FS-rhei-memory.5
    let position = render_position(&render_context);
    let plan_history = render_plan_history(&render_context)?;
    let previous_visits = render_previous_visits(&render_context)?;
    // §FS-rhei-memory.5: `rhei next` prints no `## Rhei Commands`, so the two
    // sub-sections would arrive with no `##` parent above them. They get their
    // own on this surface; the JSON field is still `navigation`.
    let navigation = render_rhei_navigation(&render_context);
    let navigation = if navigation.is_empty() {
        String::new()
    } else {
        format!("\n## Rhei Navigation\n{navigation}")
    };
    let exclusions = memory.exclusions.render(false);
    // What `rhei run` carries in `## Rhei Commands` and `## Result`, neither of
    // which `rhei next` renders. §FS-rhei-supervision.3.4
    let release_command = format!(
        "rhei{} transition {} --task {} --from {} --to {}",
        state_machine_flag(state_machine_path),
        shell_quote(&input.display().to_string()),
        task.id,
        shell_quote(&final_state),
        shell_quote(&final_state)
    );
    let supervising = render_supervisor_visit_notes(&render_context, &release_command);

    print_next_output(NextOutput {
        as_json,
        peek,
        claimed_as: claimed_as.as_deref(),
        task,
        from_state: &current_state_raw,
        to_state: task.state.as_str(),
        from_state_name: &current_state,
        to_state_name: &final_state,
        machine,
        personality: personality.as_deref(),
        instructions: &instructions,
        checkpoints: &checkpoints,
        supervisor_brief: &supervisor_brief,
        supervising: &supervising,
        position: &position,
        plan_history: &plan_history,
        previous_visits: &previous_visits,
        navigation: &navigation,
        exclusions: &exclusions,
        agent_id: agent_id_str.as_deref(),
        model_id: model_id_str.as_deref(),
    });

    Ok(())
}
