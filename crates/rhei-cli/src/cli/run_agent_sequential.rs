// Spawning one agent and waiting for it, the way a run with a single worker
// does it: gate its tooling, compose its prompt, stage its snapshot, run it,
// and record what it cost.
//
// Its own part because a sequential invocation owns the whole lifecycle inline —
// there is no channel, no slot, and no refill — which is what separates it from
// the worker pool beside it. What its exit means is the part next door.

// §AR-source-file-size.3 §FS-rhei-run.3

/// One agent invocation run to completion in the calling thread.
///
/// Returning `Ok(())` early is how a turn gives up — an interrupt, a missing
/// ticket, unavailable required tooling, or a prompt that would not compose.
/// The pass tail in `run_agent_mode` decides what that means for the pass.
// §FS-rhei-run.3
#[allow(clippy::too_many_arguments)]
fn run_sequential_agent_invocation(
    item: &(String, String, String, ResolvedAgent),
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    run_id: &str,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    progress: &mut AgentPassProgress<'_>,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent};
    use std::time::{Instant as TuiInstant, SystemTime};
    macro_rules! run_message { ($level:expr, $($arg:tt)*) => {{ emit_run_message(sink, $level, format!($($arg)*)); }}; }
    macro_rules! run_info { ($($arg:tt)*) => { run_message!(MessageLevel::Info, $($arg)*); }; }
    macro_rules! run_warn { ($($arg:tt)*) => { run_message!(MessageLevel::Warn, $($arg)*); }; }
    macro_rules! run_error { ($($arg:tt)*) => { run_message!(MessageLevel::Error, $($arg)*); }; }
    // The pass top's check is not enough on its own: this pass may
    // have spent minutes in the sequential program loop above.
    // §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(());
    }
    let (task_id_str, _current_state_raw, current_state, resolved) = item;
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(task_id_str);
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(()) };

    let tooling = resolve_tooling(machine, current_state, settings);
    let gate = gate_tooling_for_agent(resolved, &tooling);
    for warning in &gate.warnings {
        run_warn!("{warning}");
    }
    if !gate.required.is_empty() {
        let mcp_unavailable = unavailable_ids(&gate.required, ToolingKind::Mcp);
        let skill_unavailable = unavailable_ids(&gate.required, ToolingKind::Skill);
        let mut fired = false;
        if !mcp_unavailable.is_empty() {
            match fire_tooling_unavailable_transition(
                input,
                machines,
                task_id_str,
                current_state,
                ToolingKind::Mcp,
                &mcp_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => {
                    *progress.advanced_any = true;
                    fired = true;
                }
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired && !skill_unavailable.is_empty() {
            match fire_tooling_unavailable_transition(
                input,
                machines,
                task_id_str,
                current_state,
                ToolingKind::Skill,
                &skill_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => {
                    *progress.advanced_any = true;
                    fired = true;
                }
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired {
            let message =
                format_required_tooling_error(task_id_str, current_state, &gate.required);
            run_error!("  error: {message}");
            if !opts.continue_on_error() {
                return Err(miette!(
                    help = run_report_help(),
                    "{message}"
                ));
            }
            // Nothing routed the ticket anywhere; the pass moves on.
            // §FS-rhei-run.3
            progress.stalled_tasks.insert(task_id_str.clone());
        }
        return Ok(());
    }
    let tooling = gate.tooling;
    // §FS-rhei-panta.6.2 §AR-rhei-panta.5: task artifacts belong to the member runtime.
    let task_workspace_root = loaded.task_root(task_id_str, workspace_root);
    let task_runtime_dir = task_workspace_root.join("runtime");
    let visit_count = render_visit_count(
        loaded.rhei.metadata.as_ref(),
        &task.id,
        current_state,
        task.state.as_str(),
        machine,
    );
    // Settled before anything is composed or staged: a spawn this visit may not
    // have costs nothing to decline, and every step below it costs something.
    // §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.1
    let plan = plan_agent_spawn_attempt(
        &task_runtime_dir,
        &task_workspace_root,
        task_id_str,
        current_state,
        resolved_agent_log_suffix(resolved, Some(visit_count)).as_deref(),
        resolved,
        visit_count,
        run_id,
    );
    let budget = resolve_attempt_budget(machine.states.get(current_state), settings);
    if let Some(spent_budget) = plan.budget_spent(budget) {
        // The same stall step 5 gives any unmet completion condition: the ticket
        // keeps its state, no transition fires, and the pass moves on.
        // §FS-rhei-run.3 §FS-rhei-agents.3.2.3
        let owed = collect_missing_required_outputs(
            workspace_root,
            &task_workspace_root,
            machine,
            loaded.rhei.metadata.as_ref(),
            task,
            current_state,
            selected_forward_transition(&loaded.rhei, machine, task).as_deref(),
        );
        run_warn!(
            "{}",
            budget_spent_halt_line(
                task_id_str,
                current_state,
                spent_budget,
                &completion_debt_label(&owed)
            )
        );
        progress.stalled_tasks.insert(task_id_str.clone());
        return Ok(());
    }
    let checkout_root = resolve_agent_checkout_root(&task_workspace_root, task_id_str)?;
    // A sequential pass runs one invocation at a time, so nothing else of this
    // run is in flight. §FS-rhei-memory.4.3
    let memory = prompt_memory(&loaded, input, runtime_dir, BTreeSet::new());
    let render_context = RuntimeTemplateContext {
        workspace_root: &task_workspace_root,
        task_roots: Some(&loaded.task_roots),
        plan_tasks: Some(&loaded.rhei.tasks),
        checkout_root: &checkout_root.path,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: current_state,
        current_state_raw: task.state.as_str(),
        machine,
        metadata: loaded.rhei.metadata.as_ref(),
        target: resolved.target.as_ref(),
        model: resolved.model.as_deref(),
        model_provider: resolved.model_provider.as_deref(),
        model_name: resolved.model_name.as_deref(),
        agent: Some(resolved.agent.id()),
        agent_mode: resolved.mode.as_deref(),
        tooling: Some(&tooling),
        memory: Some(&memory),
    };
    // Same contract as the parallel scheduler: an uncomposable prompt
    // fails its own task, not the whole run. §FS-rhei-run.3
    let prompt = match compose_agent_prompt(&render_context) {
        Ok(prompt) => prompt,
        Err(err) => {
            run_error!("  error: Task {task_id_str} cannot be prompted: {err}");
            if !opts.continue_on_error() {
                return Err(err);
            }
            progress.unpromptable_tasks.insert(task_id_str.clone());
            return Ok(());
        }
    };
    // A retry gets its own attempt log rather than truncating the transcript
    // that explains the miss it is retrying. §FS-rhei-agents.8.1
    let log = plan.log.clone();

    run_info!(
        "\nSpawning agent '{}' for Task {}: {}",
        resolved.agent.id(),
        task_id_str,
        task.title
    );
    if let Some(m) = &resolved.model {
        run_info!("  Model: {m}");
    }
    run_info!("  Checkout: {}", checkout_root.path.display());
    run_info!("  Log: {}", log.display());
    // Names the rule, the attempt, and the budget it comes out of, so a loop is
    // visible while it spends rather than at the halt.
    // §FS-rhei-agents.3.2.1 §FS-rhei-run.3
    if let Some(note) = plan.respawn_note(task_id_str, current_state, budget) {
        run_info!("{note}");
    }

    // Spec § Execution Loop step 3: if the state declares
    // `snapshot.inherit:`, resolve and preload the source snapshot
    // before spawning the agent. The actual preload is owned by
    // impl-rhei-snapshots; this hook pins the call site so the
    // orchestration ordering is encoded in code.
    let snapshot_preload = preload_snapshot_inherit_before_spawn(
        input,
        SnapshotPreloadRoots { project: workspace_root, execution: &task_workspace_root },
        &checkout_root.path,
        machine,
        task,
        current_state,
        resolved,
        settings,
        visit_count,
        snapshot_override_selection,
        opts,
    )?;

    let started_at = TuiInstant::now();
    let started_wall = SystemTime::now();
    sink.emit(RunEvent::SlotAssigned {
        slot: 0,
        task: task_id_str.clone(),
        from: task.state.as_str().to_string(),
        to: current_state.clone(),
        agent: Some(resolved.agent.id().to_string()),
        template_context: Some(agent_template_context(resolved)),
        log_path: log.clone(),
        started_at,
        wall_clock: started_wall,
    });

    let spawn_result = spawn_and_wait_agent(
        resolved,
        opts.price_book(),
        &prompt,
        &task_workspace_root,
        &checkout_root.path,
        checkout_root.worktree_root.as_deref(),
        &callback_paths.plan_path,
        callback_paths.state_machine_path.as_deref(),
        task_id_str,
        current_state,
        visit_count,
        &tooling,
        &log,
        &task_runtime_dir,
        Some(&snapshot_preload),
        0,
        sink.clone(),
        intervene,
        // Written when this spawn ends, so its presence proves one ran.
        // §FS-rhei-agents.8.4
        &plan,
        // A fanned-out invocation writes its own result fragment.
        // §FS-rhei-states.3.3
        fanout_result_identity(
            machine.states.get(current_state),
            resolved.target.as_ref(),
            resolved.model.as_deref(),
        )
        .as_deref(),
    );
    let duration_ms = started_at.elapsed().as_millis() as u64;
    let finished_wall = SystemTime::now();
    let (outcome, exit_code) = slot_outcome(&spawn_result);
    // Held rather than emitted: whether this attempt was a handled wait is
    // known only once the completion below selects its transition.
    // §FS-rhei-states.2.2
    let release = PendingSlotRelease::hold(
        sink.clone(),
        SlotRelease {
            slot: 0,
            task: task_id_str.clone(),
            from: task.state.as_str().to_string(),
            to: current_state.clone(),
            log_path: log.clone(),
            outcome,
            finished_at: TuiInstant::now(),
            wall_clock: finished_wall,
            exit_code,
            duration_ms,
        },
    );
    // §FS-rhei-cost-accounting.4: Extraction happens after agent exit.
    match record_agent_accounting_attempt(
        AgentAccountingInvocation {
        workspace_root: &task_workspace_root,
        task,
        state: current_state,
        resolved,
        visit: visit_count,
        started_at: started_wall,
        ended_at: finished_wall,
        slot: Some(0),
        usage_capture_path: spawn_result
            .as_ref()
            .ok()
            .and_then(|outcome| outcome.usage_capture_path.as_deref()),
        cli_session: spawn_result
            .as_ref()
            .ok()
            .and_then(|outcome| outcome.cli_session.as_ref()),
        log_path: Some(&log),
        price_book: opts.price_book(),
        sink,
        },
        plan.accounting
            .as_ref()
            .expect("agent spawn plans carry accounting identity"),
    ) {
        Ok(Some(_)) => {
            if let Err(err) = regenerate_accounting_indexes(workspace_root, &loaded.rhei)
            {
                run_warn!("  warning: failed to update accounting rollups: {}", err);
            }
        }
        Ok(None) => {}
        Err(err) => {
            run_warn!("  warning: failed to record accounting: {}", err);
        }
    }
    handle_sequential_agent_completion(
        input,
        machines,
        settings,
        opts,
        workspace_root,
        &loaded,
        sink,
        SequentialAgentCompletion {
            task_id_str: task_id_str.clone(),
            state_name: current_state.clone(),
            started_at: started_wall,
            task,
            release,
            task_workspace_root,
            resolved,
            log,
            snapshot_preload,
            visit_count,
            retry_outlook: plan.retry_outlook(budget),
            // §FS-rhei-supervision.3.6: what the visit is about to be judged
            // against, read before it can change anything.
            subtree_before: subtree_shape_before_visit(task, &machines.set, current_state),
            spawn_record: plan.record.clone(),
            result: spawn_result,
        },
        progress,
    )
}
