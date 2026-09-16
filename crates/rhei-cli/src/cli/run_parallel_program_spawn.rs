// Putting one program work item on a worker thread: the log and record it
// writes, the slot it holds, and the completion it sends back down the channel.
//
// Its own part because a program invocation shares nothing with an agent's but
// the channel it answers on — no prompt, no tooling gate, no snapshot staging,
// no checkout — and the two together outgrew one file.

// §AR-source-file-size.3 §FS-rhei-programs.6.3 §FS-rhei-run.3

#[allow(clippy::too_many_arguments)]
fn spawn_parallel_program_work_item(
    item: &ProgramWorkItem,
    slot: rhei_tui::Slot,
    tx: std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    workspace_root: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<ParallelProgramSpawnOutcome> {
    // As for agents: a slot was reserved for this item before the interrupt,
    // and the shutdown starts nothing further. §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(ParallelProgramSpawnOutcome::Skipped);
    }
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(&item.task_id_str);
    // The item's owning rhei supplies its machine and callback base.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&item.task_id_str);
    let callback_paths = machines.callbacks_for_str(&item.task_id_str);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
    let Some(task) = task else { return Ok(ParallelProgramSpawnOutcome::Skipped) };

    // Programs run against the owning rhei's execution root. §FS-rhei-panta.6.2
    let task_workspace_root = loaded.task_root(&item.task_id_str, workspace_root);
    // §AR-rhei-panta.5: program transcripts and spawn records share the member runtime.
    let task_runtime_dir = task_workspace_root.join("runtime");
    // Same attempt log and same per-visit budget as an agent: a program state
    // is never skipped at scheduling either. §FS-rhei-agents.8.1
    let plan = plan_spawn_attempt(
        &task_runtime_dir,
        &task_workspace_root,
        &item.task_id_str,
        &item.current_state,
        None,
    );
    let budget =
        resolve_attempt_budget(machine.states.get(item.current_state.as_str()), settings);
    if let Some(spent_budget) = plan.budget_spent(budget) {
        let owed = collect_missing_required_outputs(
            workspace_root,
            &task_workspace_root,
            machine,
            loaded.rhei.metadata.as_ref(),
            task,
            &item.current_state,
            selected_forward_transition(&loaded.rhei, machine, task).as_deref(),
        );
        // `Skipped` is the pool's stall. §FS-rhei-run.3 §FS-rhei-agents.3.2.3
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
        return Ok(ParallelProgramSpawnOutcome::Skipped);
    }
    let workspace_root = task_workspace_root.as_path();

    let log = plan.log.clone();
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Info,
        format!("\nSpawning program for Task {}: {} (parallel)", item.task_id_str, task.title),
    );
    emit_run_message(sink, rhei_tui::MessageLevel::Info, format!("  Log: {}", log.display()));
    // §FS-rhei-agents.3.2.1: a retry says it is one, and what it is retrying.
    if let Some(note) = plan.respawn_note(&item.task_id_str, &item.current_state, budget) {
        emit_run_message(sink, rhei_tui::MessageLevel::Info, note);
    }

    let from_state = task.state.as_str().to_string();
    let started_at = std::time::Instant::now();
    let started_wall = std::time::SystemTime::now();
    sink.emit(rhei_tui::RunEvent::SlotAssigned {
        slot,
        task: item.task_id_str.clone(),
        from: from_state.clone(),
        to: item.current_state.clone(),
        agent: None,
        template_context: None,
        log_path: log.clone(),
        started_at,
        wall_clock: started_wall,
    });

    // Read before the plan moves into the worker: only here are the plan and
    // the resolved budget both in hand. §FS-rhei-agents.3.2.1
    let outlook_for_result = plan.retry_outlook(budget);
    let plan_for_thread = plan;
    let resolved_for_thread = item.resolved.clone();
    let workspace_root_for_thread = workspace_root.to_path_buf();
    let task_roots_for_thread = loaded.task_roots.clone();
    let callback_paths_for_thread = callback_paths.clone();
    let plan_title_for_thread = loaded.rhei.title.clone();
    let task_for_thread = task.clone();
    let state_name_for_thread = item.current_state.clone();
    let current_state_raw_for_thread = task.state.as_str().to_string();
    let machine_for_thread = machine.clone();
    let metadata_for_thread = loaded.rhei.metadata.clone();
    let log_for_thread = log.clone();
    let sink_for_thread = sink.clone();
    let task_id_for_result = item.task_id_str.clone();
    let state_name_for_result = item.current_state.clone();
    let task_id_for_panic = item.task_id_str.clone();
    let state_for_panic = item.current_state.clone();
    // §FS-rhei-run.3.2: the program's group belongs to this run.
    let run_owner = current_run_owner();

    let handle = std::thread::spawn(move || {
        inherit_run_owner(run_owner);
        let thread_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let render_context = RuntimeTemplateContext {
                workspace_root: &workspace_root_for_thread,
                task_roots: Some(&task_roots_for_thread),
                // A program state renders no supervisor brief, and the task
                // tree does not cross into the worker thread.
                plan_tasks: None,
                checkout_root: &workspace_root_for_thread,
                plan_path: &callback_paths_for_thread.plan_path,
                state_machine_path: callback_paths_for_thread.state_machine_path.as_deref(),
                plan_title: &plan_title_for_thread,
                task: &task_for_thread,
                state_name: &state_name_for_thread,
                current_state_raw: &current_state_raw_for_thread,
                machine: &machine_for_thread,
                metadata: metadata_for_thread.as_ref(),
                target: None,
                model: None,
                model_provider: None,
                model_name: None,
                agent: None,
                agent_mode: None,
                tooling: None,
                memory: None,
            };
            let result = spawn_and_wait_program(
                &resolved_for_thread,
                &render_context,
                &log_for_thread,
                // §FS-rhei-agents.8.4: written when this command ends.
                &plan_for_thread,
                &sink_for_thread,
            );
            let duration_ms = started_at.elapsed().as_millis() as u64;
            let (outcome, exit_code) = slot_outcome(&result);
            // Read here, where the process was reaped, and emitted by the main
            // thread once it knows whether this attempt was a poll state's
            // handled wait. §FS-rhei-states.2.2
            let release = PendingSlotRelease::hold(
                sink_for_thread,
                SlotRelease {
                    slot,
                    task: task_id_for_result.clone(),
                    from: from_state,
                    to: state_name_for_result.clone(),
                    log_path: log_for_thread,
                    outcome,
                    finished_at: std::time::Instant::now(),
                    wall_clock: std::time::SystemTime::now(),
                    exit_code,
                    duration_ms,
                },
            );
            ParallelAgentThreadMessage::ProgramCompleted(ParallelProgramCompletion {
                task_id_str: task_id_for_result,
                state_name: state_name_for_result,
                retry_outlook: outlook_for_result,
                release,
                result,
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

    Ok(ParallelProgramSpawnOutcome::Spawned(ParallelProgramSpawned {
        task_id_str: item.task_id_str.clone(),
        state_name: item.current_state.clone(),
        handle,
    }))
}
