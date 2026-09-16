// Putting one agent work item on a worker thread: the slot it takes, the prompt
// and snapshot staging it needs before the process starts, and the completion it
// sends back down the channel.
//
// Its own part because spawning is where an invocation stops being schedulable
// data and becomes a live subprocess; the scheduler next door only decides
// which items get this far, and how many at a time. A program work item is
// spawned by the part after this one.

// §AR-source-file-size.3 §FS-rhei-run.3

fn take_parallel_slot(free_slots: &mut BTreeSet<rhei_tui::Slot>, next_extra_slot: &mut rhei_tui::Slot) -> rhei_tui::Slot {
    if let Some(slot) = free_slots.pop_first() {
        return slot;
    }
    let slot = *next_extra_slot;
    *next_extra_slot = next_extra_slot.saturating_add(1);
    slot
}

#[allow(clippy::too_many_arguments)]
fn spawn_parallel_agent_work_item(
    item: &AgentWorkItem,
    slot: rhei_tui::Slot,
    tx: std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
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
    // Tickets this pass already spawned and has not reaped: what `### In
    // Flight` names, since `rhei run` claims by spawning rather than by writing
    // an `**Assignee:**`. §FS-rhei-memory.4.3
    run_in_flight: &BTreeSet<String>,
) -> MietteResult<ParallelAgentSpawnOutcome> {
    // The work item was chosen before the interrupt arrived; starting it now
    // would be new work the shutdown promised not to schedule.
    // §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(ParallelAgentSpawnOutcome::Skipped);
    }
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    // The item's owning rhei supplies its machine and callback base.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&item.task_id_str);
    let callback_paths = machines.callbacks_for_str(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelAgentSpawnOutcome::Skipped) };

    // Prompts, logs, and spawn records use the member's execution root.
    // §FS-rhei-panta.6.2 §AR-rhei-panta.5
    let task_workspace_root = loaded.task_root(&item.task_id_str, workspace_root);
    let task_runtime_dir = task_workspace_root.join("runtime");
    let visit_count = render_visit_count(
        loaded.rhei.metadata.as_ref(),
        &task.id,
        &item.current_state,
        task.state.as_str(),
        machine,
    );
    // Settled before anything is composed or staged, as in the sequential path:
    // a spawn this visit may not have costs nothing to decline.
    // §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.1
    let plan = plan_agent_spawn_attempt(
        &task_runtime_dir,
        &task_workspace_root,
        &item.task_id_str,
        &item.current_state,
        resolved_agent_log_suffix(&item.resolved, Some(visit_count)).as_deref(),
        &item.resolved,
        visit_count,
        run_id,
    );
    let budget =
        resolve_attempt_budget(machine.states.get(item.current_state.as_str()), settings);
    if let Some(spent_budget) = plan.budget_spent(budget) {
        // `Skipped` is the pool's stall: the scheduler records it in
        // `stalled_tasks`, so the ticket keeps its state and is out of the
        // running for the rest of the run. §FS-rhei-run.3 §FS-rhei-agents.3.2.3
        let owed = collect_missing_required_outputs(
            workspace_root,
            &task_workspace_root,
            machine,
            loaded.rhei.metadata.as_ref(),
            task,
            &item.current_state,
            selected_forward_transition(&loaded.rhei, machine, task).as_deref(),
        );
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Warn,
            budget_spent_halt_line(
                &item.task_id_str,
                &item.current_state,
                spent_budget,
                &completion_debt_label(&owed),
            ),
        );
        return Ok(ParallelAgentSpawnOutcome::Skipped);
    }
    // Bound while both roots still have their own names: the rebind below
    // shadows the project root, and the two differ only in a Panta project.
    // §FS-rhei-snapshots.7
    let snapshot_roots = SnapshotPreloadRoots {
        project: workspace_root,
        execution: task_workspace_root.as_path(),
    };
    let workspace_root = task_workspace_root.as_path();

    let tooling = resolve_tooling(machine, &item.current_state, settings);
    let gate = gate_tooling_for_agent(&item.resolved, &tooling);
    for warning in &gate.warnings {
        emit_run_message(sink, rhei_tui::MessageLevel::Warn, warning.clone());
    }
    if !gate.required.is_empty() {
        let mcp_unavailable = unavailable_ids(&gate.required, ToolingKind::Mcp);
        let skill_unavailable = unavailable_ids(&gate.required, ToolingKind::Skill);
        let mut fired = false;
        if !mcp_unavailable.is_empty() {
            match fire_tooling_unavailable_transition(
                input,
                machines,
                &item.task_id_str,
                &item.current_state,
                ToolingKind::Mcp,
                &mcp_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => fired = true,
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired && !skill_unavailable.is_empty() {
            match fire_tooling_unavailable_transition(
                input,
                machines,
                &item.task_id_str,
                &item.current_state,
                ToolingKind::Skill,
                &skill_unavailable,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => fired = true,
                TimeoutTransitionOutcome::NoRule | TimeoutTransitionOutcome::Failed => {}
            }
        }
        if !fired {
            let message =
                format_required_tooling_error(&item.task_id_str, &item.current_state, &gate.required);
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {message}"));
            if !opts.continue_on_error() {
                return Err(miette!(
                    help = run_report_help(),
                    "{message}"
                ));
            }
        }
        return Ok(if fired {
            ParallelAgentSpawnOutcome::Advanced
        } else {
            ParallelAgentSpawnOutcome::Skipped
        });
    }
    let tooling = gate.tooling;
    let checkout_root = resolve_agent_checkout_root(workspace_root, &item.task_id_str)?;
    let memory = prompt_memory(&loaded, input, runtime_dir, run_in_flight.clone());
    let render_context = RuntimeTemplateContext {
        workspace_root,
        task_roots: Some(&loaded.task_roots),
        plan_tasks: Some(&loaded.rhei.tasks),
        checkout_root: &checkout_root.path,
        plan_path: &callback_paths.plan_path,
        state_machine_path: callback_paths.state_machine_path.as_deref(),
        plan_title: &loaded.rhei.title,
        task,
        state_name: &item.current_state,
        current_state_raw: task.state.as_str(),
        machine,
        metadata: loaded.rhei.metadata.as_ref(),
        target: item.resolved.target.as_ref(),
        model: item.resolved.model.as_deref(),
        model_provider: item.resolved.model_provider.as_deref(),
        model_name: item.resolved.model_name.as_deref(),
        agent: Some(item.resolved.agent.id()),
        agent_mode: item.resolved.mode.as_deref(),
        tooling: Some(&tooling),
        memory: Some(&memory),
    };
    // Failing the pass would take every healthy sibling down with it.
    // §FS-rhei-run.3: an uncomposable prompt fails its task, not the run.
    let prompt = match compose_agent_prompt(&render_context) {
        Ok(prompt) => prompt,
        Err(err) => {
            let message = format!("Task {} cannot be prompted: {err}", item.task_id_str);
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {message}"));
            if !opts.continue_on_error() {
                return Err(err);
            }
            return Ok(ParallelAgentSpawnOutcome::Unpromptable(item.task_id_str.clone()));
        }
    };
    // A retry gets its own attempt log rather than truncating the transcript
    // that explains the miss it is retrying. §FS-rhei-agents.8.1
    let log = plan.log.clone();
    let working_dir = checkout_root.path.clone();
    let worktree_root = checkout_root.worktree_root.clone();
    let plan_path = callback_paths.plan_path.clone();
    let state_machine_path = callback_paths.state_machine_path.clone();
    let tid = item.task_id_str.clone();
    let sname = item.current_state.clone();
    // A fanned-out invocation writes its own result fragment. §FS-rhei-states.3.3
    let result_identity = fanout_result_identity(
        machine.states.get(item.current_state.as_str()),
        item.resolved.target.as_ref(),
        item.resolved.model.as_deref(),
    );

    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!(
            "\nSpawning agent '{}' for Task {}: {} (parallel)",
            item.resolved.agent.id(),
            item.task_id_str,
            task.title
        ),
    );
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("  Checkout: {}", working_dir.display()),
    );
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("  Log: {}", log.display()),
    );
    // Names the rule, the attempt, and the budget it comes out of, so a loop is
    // visible while it spends rather than at the halt.
    // §FS-rhei-agents.3.2.1 §FS-rhei-run.3
    if let Some(note) = plan.respawn_note(&item.task_id_str, &item.current_state, budget) {
        emit_run_message(sink, rhei_tui::MessageLevel::Info, note);
    }

    let snapshot_preload = preload_snapshot_inherit_before_spawn(
        input,
        snapshot_roots,
        &working_dir,
        machine,
        task,
        &item.current_state,
        &item.resolved,
        settings,
        visit_count,
        snapshot_override_selection,
        opts,
    )?;

    let from_state = task.state.as_str().to_string();
    let started_at = std::time::Instant::now();
    let started_wall = std::time::SystemTime::now();
    sink.emit(rhei_tui::RunEvent::SlotAssigned {
        slot,
        task: item.task_id_str.clone(),
        from: from_state.clone(),
        to: item.current_state.clone(),
        agent: Some(item.resolved.agent.id().to_string()),
        template_context: Some(agent_template_context(&item.resolved)),
        log_path: log.clone(),
        started_at,
        wall_clock: started_wall,
    });

    let resolved_for_thread = item.resolved.clone();
    let tooling_for_thread = tooling.clone();
    let sink_for_thread = sink.clone();
    let intervene_for_thread = intervene.cloned();
    let log_for_thread = log.clone();
    let log_for_result = log.clone();
    let from_for_thread = from_state;
    let to_for_thread = item.current_state.clone();
    let tid_for_event = item.task_id_str.clone();
    let runtime_dir_for_thread = task_runtime_dir;
    // Read before the plan moves into the worker: only here are the plan and
    // the resolved budget both in hand. §FS-rhei-agents.3.2.1
    let outlook_for_result = plan.retry_outlook(budget);
    // §FS-rhei-supervision.3.6: what the visit is about to be judged against,
    // read from the same pre-spawn plan the sequential path reads it from.
    let subtree_before =
        subtree_shape_before_visit(task, &machines.set, &item.current_state);
    // Read before the plan moves into the worker, as the outlook above is.
    // §FS-rhei-agents.3.2.3
    let spawn_record = plan.record.clone();
    let plan_for_thread = plan;
    let snapshot_preload_for_thread = snapshot_preload.clone();
    let snapshot_preload_for_result = snapshot_preload.clone();
    let visit_for_result = visit_count;
    let resolved_for_result = item.resolved.clone();
    let workspace_root_for_thread = workspace_root.to_path_buf();
    let rhei_root_for_thread = workspace_root.to_path_buf();
    let worktree_root_for_thread = worktree_root.clone();
    let task_for_accounting = task.clone();
    let price_book_for_thread = opts.price_book().clone();
    let task_id_for_panic = tid.clone();
    let state_for_panic = sname.clone();
    // The worker spawns this run's subprocess, so the run's shutdown guard —
    // and no other — owns the group it leads. §FS-rhei-run.3.2
    let run_owner = current_run_owner();

    let handle = std::thread::spawn(move || {
        inherit_run_owner(run_owner);
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let resolved = resolved_for_thread;
            let result = spawn_and_wait_agent(
                &resolved,
                &price_book_for_thread,
                &prompt,
                &rhei_root_for_thread,
                &working_dir,
                worktree_root_for_thread.as_deref(),
                &plan_path,
                state_machine_path.as_deref(),
                &tid,
                &sname,
                visit_count,
                &tooling_for_thread,
                &log_for_thread,
                &runtime_dir_for_thread,
                Some(&snapshot_preload_for_thread),
                slot,
                sink_for_thread.clone(),
                intervene_for_thread.as_ref(),
                // Written when this spawn ends, so its presence proves one ran.
                // §FS-rhei-agents.8.4
                &plan_for_thread,
                result_identity.as_deref(),
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = slot_outcome(&result);
            let finished_wall = std::time::SystemTime::now();
            // Read here, where the process was reaped, and emitted by the main
            // thread once it knows whether this attempt was a poll state's
            // handled wait. §FS-rhei-states.2.2
            let release = PendingSlotRelease::hold(
                sink_for_thread.clone(),
                SlotRelease {
                    slot,
                    task: tid_for_event,
                    from: from_for_thread,
                    to: to_for_thread,
                    log_path: log_for_thread.clone(),
                    outcome,
                    finished_at: std::time::Instant::now(),
                    wall_clock: finished_wall,
                    exit_code,
                    duration_ms,
                },
            );
            let usage_capture_path =
                result.as_ref().ok().and_then(|outcome| outcome.usage_capture_path.as_ref());
            let accounting_result = record_agent_accounting_attempt(
                AgentAccountingInvocation {
                    workspace_root: &workspace_root_for_thread,
                    task: &task_for_accounting,
                    state: &sname,
                    resolved: &resolved,
                    visit: visit_count,
                    started_at: started_wall,
                    ended_at: finished_wall,
                    slot: Some(slot),
                    usage_capture_path: usage_capture_path.map(PathBuf::as_path),
                    cli_session: result
                        .as_ref()
                        .ok()
                        .and_then(|outcome| outcome.cli_session.as_ref()),
                    log_path: Some(&log_for_thread),
                    price_book: &price_book_for_thread,
                    sink: &sink_for_thread,
                },
                plan_for_thread
                    .accounting
                    .as_ref()
                    .expect("agent spawn plans carry accounting identity"),
            );
            let (accounting_recorded, accounting_warning) = match accounting_result {
                Ok(Some(_)) => (true, None),
                Ok(None) => (false, None),
                Err(err) => (false, Some(err.to_string())),
            };
            ParallelAgentThreadMessage::Completed(ParallelAgentCompletion {
                task_id_str: tid,
                state_name: sname,
                started_at: started_wall,
                release,
                resolved: resolved_for_result,
                log: log_for_result,
                snapshot_preload: snapshot_preload_for_result,
                visit_count: visit_for_result,
                retry_outlook: outlook_for_result,
                subtree_before,
                spawn_record,
                result,
                accounting_recorded,
                accounting_warning,
                slot,
            })
        }));
        let message = thread_result.unwrap_or(ParallelAgentThreadMessage::Panicked {
            task_id_str: task_id_for_panic,
            state_name: state_for_panic,
            slot,
        });
        let _ = tx.send(message);
    });

    Ok(ParallelAgentSpawnOutcome::Spawned(ParallelAgentSpawned {
        task_id_str: item.task_id_str.clone(),
        state_name: item.current_state.clone(),
        handle,
    }))
}
