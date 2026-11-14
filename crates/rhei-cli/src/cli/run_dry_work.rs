// Pending work previews share the real dispatch planner before predicting success.
// §FS-rhei-run.4

/// Plan only pending work, using the invocation identity and runtime its real
/// dispatch uses. Refusals stay on the frontend and in the run's final verdict;
/// no prompt, process, callback or runtime write is needed. §FS-rhei-run.4
fn preview_spawn(
    spawns: &SpawnSiteRoots<'_>,
    rhei: &rhei_core::ast::Rhei,
    task: &rhei_core::ast::Task,
    state: &str,
    agent: Option<&ResolvedAgent>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    refusals: &mut SpawnRefusals,
) -> bool {
    let task_id = task.id.to_string();
    let machine = spawns.machines.for_task(&task.id);
    let task_root = spawns.tasks.artifact_root(&task_id);
    let runtime = match agent {
        Some(_) => spawns.machines.agent_runtime_dir(&task_id, task_root, spawns.runtime_dir),
        None => task_root.join("runtime"),
    };
    let identity = agent.and_then(resolved_agent_log_identity);
    let visit =
        render_visit_count(rhei.metadata.as_ref(), &task.id, state, task.state.as_str(), machine);
    let planned = plan_named_spawn(
        machine,
        task_root,
        &runtime,
        &task_id,
        state,
        visit,
        identity.as_deref(),
        &spawns.machines.reset,
    );
    refusals.observe(&task_id, state, identity.as_deref(), planned, sink).is_ok()
}

/// Both sequential and pooled dispatch preview their pending items here. Work
/// already reusable and callback-only states were separated by the ready scan;
/// only successful plans earn spawn and post-work edge projections. §FS-rhei-run.4
#[allow(clippy::too_many_arguments)]
fn preview_pending_work(
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    programs: &[(String, String, String, ResolvedProgram)],
    agents: &[(String, String, String, ResolvedAgent)],
    sink: &Arc<dyn rhei_tui::EventSink>,
    refusals: &mut SpawnRefusals,
) -> MietteResult<()> {
    let loaded = load_run_plan(input)?;
    let spawns =
        SpawnSiteRoots::new(workspace_root, runtime_dir, &loaded, machines, Some(settings), opts);
    for (task_id, raw_state, state, _) in programs {
        let Some(task) = find_task_by_id(&loaded.rhei.tasks, &parse_task_id(task_id)) else {
            continue;
        };
        if !preview_spawn(&spawns, &loaded.rhei, task, state, None, sink, refusals) {
            continue;
        }
        let machine = machines.for_task_str(task_id);
        if let Some(route) =
            find_program_exit_transition(machine, loaded.rhei.metadata.as_ref(), task, state, 0)?
        {
            emit_run_message(
                sink,
                rhei_tui::MessageLevel::Info,
                format_dry_run_transition(task_id, raw_state, &route.to, machine),
            );
        }
    }
    for (task_id, raw_state, state, resolved) in agents {
        let Some(task) = find_task_by_id(&loaded.rhei.tasks, &parse_task_id(task_id)) else {
            continue;
        };
        if !preview_spawn(&spawns, &loaded.rhei, task, state, Some(resolved), sink, refusals) {
            continue;
        }
        let machine = machines.for_task_str(task_id);
        let task_root = loaded.task_root(task_id, workspace_root);
        let checkout = resolve_agent_checkout_root(&task_root, task_id)?;
        let callbacks = machines.callbacks_for_str(task_id);
        let exclusions = loaded_task_exclusions(
            &loaded,
            task,
            &task_root,
            &checkout.path,
            &callbacks.plan_path,
            machine,
            callbacks.state_machine_path.as_deref(),
            settings,
            opts,
        )
        .map_err(exclusion_report)?;
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format_dry_run_agent_spawn(task_id, raw_state, resolved, &exclusions),
        );
        if let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? {
            emit_run_message(
                sink,
                rhei_tui::MessageLevel::Info,
                format_dry_run_agent_transition(
                    task_id,
                    raw_state,
                    &to_state,
                    resolved,
                    machine,
                    &exclusions,
                ),
            );
        }
    }
    Ok(())
}
