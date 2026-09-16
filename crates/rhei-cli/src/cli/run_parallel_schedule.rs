// How much of the pass runs at once and which items get the capacity: the
// initial fill from the pass batch, the release of a finished worker's slot,
// and the refill that re-reads the live ready set when capacity frees up.
//
// Its own part because capacity is the one decision every parallel invocation
// shares, while spawning next door concerns a single item.

// §AR-source-file-size.3 §FS-rhei-run.3

struct ParallelScheduleOutcome {
    spawned: usize,
    advanced: bool,
    /// Work items the scheduler could not start and that nothing routed
    /// elsewhere — unavailable required tooling with no rule, a ticket that left
    /// the plan. Still "ready", so unrecorded they are re-attempted on every
    /// freed slot for the rest of the pass.
    // §FS-rhei-run.3
    skipped: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
fn schedule_agent_work_items(
    items: Vec<AgentWorkItem>,
    max_new_tasks: usize,
    unpromptable: &mut HashSet<String>,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
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
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let mut advanced = false;
    let mut skipped = Vec::new();

    for item in items {
        if !selected_task_ids.contains(&item.task_id_str) {
            if selected_task_ids.len() >= max_new_tasks {
                continue;
            }
            selected_task_ids.insert(item.task_id_str.clone());
        }

        let slot = take_parallel_slot(free_slots, next_extra_slot);
        // Read fresh for each spawn: the set is what is running *now*, and the
        // loop adds to it as it goes. §FS-rhei-memory.4.3
        let run_in_flight: BTreeSet<String> =
            active_invocation_counts.keys().cloned().collect();
        match spawn_parallel_agent_work_item(
            &item,
            slot,
            tx.clone(),
            input,
            machines,
            settings,
            opts,
            workspace_root,
            runtime_dir,
            run_id,
            snapshot_override_selection,
            sink,
            intervene,
            &run_in_flight,
        )? {
            ParallelAgentSpawnOutcome::Spawned(spawned_agent) => {
                *active_invocation_counts.entry(spawned_agent.task_id_str.clone()).or_insert(0) += 1;
                if !machines
                    .for_task_str(&spawned_agent.task_id_str)
                    .states
                    .get(&spawned_agent.state_name)
                    .map(|state| state.concurrent)
                    .unwrap_or(false)
                {
                    *active_state_counts.entry(spawned_agent.state_name.clone()).or_insert(0) += 1;
                }
                handles.push(spawned_agent.handle);
                spawned += 1;
            }
            ParallelAgentSpawnOutcome::Advanced => {
                free_slots.insert(slot);
                advanced = true;
            }
            ParallelAgentSpawnOutcome::Skipped => {
                free_slots.insert(slot);
                skipped.push(item.task_id_str.clone());
            }
            ParallelAgentSpawnOutcome::Unpromptable(task_id) => {
                free_slots.insert(slot);
                unpromptable.insert(task_id);
            }
        }
    }

    Ok(ParallelScheduleOutcome { spawned, advanced, skipped })
}

#[allow(clippy::too_many_arguments)]
fn schedule_program_work_items(
    items: Vec<ProgramWorkItem>,
    max_new_tasks: usize,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    workspace_root: &Path,
    runtime_dir: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let advanced = false;
    let mut skipped = Vec::new();

    for item in items {
        if !selected_task_ids.contains(&item.task_id_str) {
            if selected_task_ids.len() >= max_new_tasks {
                continue;
            }
            selected_task_ids.insert(item.task_id_str.clone());
        }

        let slot = take_parallel_slot(free_slots, next_extra_slot);
        match spawn_parallel_program_work_item(
            &item,
            slot,
            tx.clone(),
            input,
            machines,
            settings,
            workspace_root,
            runtime_dir,
            sink,
        )? {
            ParallelProgramSpawnOutcome::Spawned(spawned_program) => {
                *active_invocation_counts
                    .entry(spawned_program.task_id_str.clone())
                    .or_insert(0) += 1;
                if !machines
                    .for_task_str(&spawned_program.task_id_str)
                    .states
                    .get(&spawned_program.state_name)
                    .map(|state| state.concurrent)
                    .unwrap_or(false)
                {
                    *active_state_counts.entry(spawned_program.state_name.clone()).or_insert(0) += 1;
                }
                handles.push(spawned_program.handle);
                spawned += 1;
            }
            ParallelProgramSpawnOutcome::Skipped => {
                free_slots.insert(slot);
                skipped.push(item.task_id_str.clone());
            }
        }
    }

    Ok(ParallelScheduleOutcome { spawned, advanced, skipped })
}

fn release_parallel_worker(
    task_id_str: &str,
    state_name: &str,
    slot: rhei_tui::Slot,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
) {
    free_slots.insert(slot);
    if let Some(count) = active_invocation_counts.get_mut(task_id_str) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_invocation_counts.remove(task_id_str);
        }
    }
    if let Some(count) = active_state_counts.get_mut(state_name) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_state_counts.remove(state_name);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refill_parallel_worker_pool(
    unpromptable: &mut HashSet<String>,
    stalled: &HashSet<String>,
    pass: u32,
    task_limit: usize,
    tx: &std::sync::mpsc::Sender<ParallelAgentThreadMessage>,
    input: &Path,
    live: &mut LiveRunContext,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    run_id: &str,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    active_state_counts: &mut HashMap<String, usize>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
    identity: &RunIdentity,
) -> MietteResult<ParallelScheduleOutcome> {
    // A freed slot is not refilled once the run is interrupted: the shutdown
    // drains what is in flight, it does not start more. §FS-rhei-run.3.2
    if interrupt_requested() {
        return Ok(ParallelScheduleOutcome { spawned: 0, advanced: false, skipped: Vec::new() });
    }
    // Program and agent work share live capacity.
    // Each completion reloads the ready set. §FS-rhei-run.3 §FS-rhei-programs.6.3
    let task_capacity = if task_limit == usize::MAX {
        usize::MAX
    } else {
        task_limit.saturating_sub(active_invocation_counts.len())
    };
    if task_capacity == 0 {
        return Ok(ParallelScheduleOutcome { spawned: 0, advanced: false, skipped: Vec::new() });
    }

    // Refresh and initialize new members before reusing a freed slot;
    // admission never enlarges the pool. §FS-rhei-run.3 §FS-rhei-run.5
    let (reloaded, admitted) = live.checkpoint(input, workspace_root, opts, identity)?;
    if !admitted.is_empty() {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format!("Admitted {} new rhei member(s): {}", admitted.len(), admitted.join(", ")),
        );
    }
    let machines = &live.machines;
    let settings = &live.settings;
    let active_task_ids = active_invocation_counts.keys().cloned().collect::<HashSet<_>>();
    let active_nonconcurrent_states = active_state_counts.keys().cloned().collect::<HashSet<_>>();
    let (mut program_items, program_deferred) = collect_ready_program_work_items(
        &reloaded,
        machines,
        settings,
        opts,
        workspace_root,
        &active_task_ids,
        &active_nonconcurrent_states,
    )?;
    // A ticket whose worker finished this pass without moving it is still
    // "ready", so refilling from the ready set alone re-spawns it forever.
    // §FS-rhei-run.3
    program_items.retain(|item| !stalled.contains(&item.task_id_str));
    if !program_deferred.is_empty() {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format!(
                "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                program_deferred.len(),
                program_deferred.join(", ")
            ),
        );
        sink.emit(rhei_tui::RunEvent::TasksDeferred { pass, tasks: program_deferred });
    }

    let program_outcome = schedule_program_work_items(
        program_items,
        task_capacity,
        tx,
        input,
        machines,
        settings,
        workspace_root,
        runtime_dir,
        sink,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        active_state_counts,
        handles,
    )?;

    let task_capacity = if task_limit == usize::MAX {
        usize::MAX
    } else {
        task_limit.saturating_sub(active_invocation_counts.len())
    };
    if task_capacity == 0 {
        return Ok(program_outcome);
    }

    let reloaded = load_plan(input)?;
    let active_task_ids = active_invocation_counts.keys().cloned().collect::<HashSet<_>>();
    let active_nonconcurrent_states = active_state_counts.keys().cloned().collect::<HashSet<_>>();
    let (mut agent_items, agent_deferred) = collect_ready_agent_work_items(
        &reloaded,
        machines,
        settings,
        opts,
        workspace_root,
        &active_task_ids,
        &active_nonconcurrent_states,
    )?;
    agent_items.retain(|item| !stalled.contains(&item.task_id_str));
    if !agent_deferred.is_empty() {
        emit_run_message(
            sink,
            rhei_tui::MessageLevel::Info,
            format!(
                "Deferred {} task(s) in non-concurrent states to a later pass: {}",
                agent_deferred.len(),
                agent_deferred.join(", ")
            ),
        );
        sink.emit(rhei_tui::RunEvent::TasksDeferred { pass, tasks: agent_deferred });
    }

    let refill_candidates = agent_items
        .iter()
        .map(|item| {
            (
                item.task_id_str.clone(),
                item.current_state_raw.clone(),
                item.current_state.clone(),
                item.resolved.clone(),
            )
        })
        .collect::<Vec<_>>();
    let snapshot_override_selection =
        select_snapshot_override_run_invocation(machines, opts, &refill_candidates)?;
    let agent_outcome = schedule_agent_work_items(
        agent_items,
        task_capacity,
        unpromptable,
        tx,
        input,
        machines,
        settings,
        opts,
        workspace_root,
        runtime_dir,
        run_id,
        snapshot_override_selection.as_ref(),
        sink,
        intervene,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        active_state_counts,
        handles,
    )?;

    let mut skipped = program_outcome.skipped;
    skipped.extend(agent_outcome.skipped);
    Ok(ParallelScheduleOutcome {
        spawned: program_outcome.spawned + agent_outcome.spawned,
        advanced: program_outcome.advanced || agent_outcome.advanced,
        skipped,
    })
}
