// How much of the pass runs at once and which items get the capacity: the
// initial fill from the pass batch, the release of a finished worker's slot,
// and the refill that re-reads the live ready set when capacity frees up —
// and the non-concurrent key all three, and the pass's first fill, admit by.
//
// Its own part because capacity is the one decision every parallel invocation
// shares, while spawning next door concerns a single item.

// §AR-source-file-size.3 §FS-rhei-run.3 §FS-rhei-run.5.2.1

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
    // §FS-rhei-budgets.7.2: initial fills and refills retain the run's entitlement.
    budget_run: &BudgetRun,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    spawn_refusals: &mut SpawnRefusals,
    intervene: Option<&Arc<RunInterveneSink>>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    nonconcurrent_in_flight: &mut NonConcurrentInFlight,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let mut advanced = false;
    let mut skipped = Vec::new();

    for item in items {
        // §FS-rhei-seal.8.2: before a slot or bound admission; a refused start is a stall.
        if strict_refuses_start(&item.task_id_str, &item.current_state)? {
            skipped.push(item.task_id_str.clone());
            continue;
        }
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
            budget_run,
            snapshot_override_selection,
            sink,
            spawn_refusals,
            intervene,
            &run_in_flight,
        )? {
            ParallelAgentSpawnOutcome::Spawned(spawned_agent) => {
                *active_invocation_counts.entry(spawned_agent.task_id_str.clone()).or_insert(0) += 1;
                nonconcurrent_in_flight.hold(machines, &spawned_agent.task_id_str, &spawned_agent.state_name);
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
    sink: &Arc<dyn rhei_tui::EventSink>,
    spawn_refusals: &mut SpawnRefusals,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    nonconcurrent_in_flight: &mut NonConcurrentInFlight,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
) -> MietteResult<ParallelScheduleOutcome> {
    let mut selected_task_ids = HashSet::new();
    let mut spawned = 0usize;
    let advanced = false;
    let mut skipped = Vec::new();

    for item in items {
        // §FS-rhei-seal.8.2: before a slot or bound admission; a refused start is a stall.
        if strict_refuses_start(&item.task_id_str, &item.current_state)? {
            skipped.push(item.task_id_str.clone());
            continue;
        }
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
            sink,
            spawn_refusals,
        )? {
            ParallelProgramSpawnOutcome::Spawned(spawned_program) => {
                *active_invocation_counts
                    .entry(spawned_program.task_id_str.clone())
                    .or_insert(0) += 1;
                nonconcurrent_in_flight.hold(machines, &spawned_program.task_id_str, &spawned_program.state_name);
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
    nonconcurrent_in_flight: &mut NonConcurrentInFlight,
) {
    free_slots.insert(slot);
    if let Some(count) = active_invocation_counts.get_mut(task_id_str) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            active_invocation_counts.remove(task_id_str);
        }
    }
    nonconcurrent_in_flight.release(task_id_str, state_name);
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
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    spawn_refusals: &mut SpawnRefusals,
    intervene: Option<&Arc<RunInterveneSink>>,
    free_slots: &mut BTreeSet<rhei_tui::Slot>,
    next_extra_slot: &mut rhei_tui::Slot,
    active_invocation_counts: &mut HashMap<String, usize>,
    nonconcurrent_in_flight: &mut NonConcurrentInFlight,
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
    let (mut program_items, program_deferred) = collect_ready_program_work_items(
        &reloaded,
        machines,
        settings,
        opts,
        workspace_root,
        runtime_dir,
        &active_task_ids,
        nonconcurrent_in_flight,
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
        sink,
        spawn_refusals,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        nonconcurrent_in_flight,
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

    let reloaded = load_run_plan(input)?;
    let active_task_ids = active_invocation_counts.keys().cloned().collect::<HashSet<_>>();
    let (mut agent_items, agent_deferred) = collect_ready_agent_work_items(
        &reloaded,
        machines,
        settings,
        opts,
        workspace_root,
        runtime_dir,
        &active_task_ids,
        nonconcurrent_in_flight,
        sink,
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

    // Refills share the run's binding, including whether preload consumed it.
    // §FS-rhei-snapshot-operations.2
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
        &identity.budget,
        snapshot_override_selection,
        sink,
        spawn_refusals,
        intervene,
        free_slots,
        next_extra_slot,
        active_invocation_counts,
        nonconcurrent_in_flight,
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


/// One non-concurrent state of one machine source. Two tickets share a limit
/// only when both parts match: the state's name alone is not the state, since
/// a same-named state of another source is another state. The source is the
/// canonical path of the file the ticket's machine resolved from (`None` for
/// the built-in default), so two spellings of one file are one source and two
/// identical files are two. §FS-rhei-run.5.2.1
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NonConcurrentState {
    source: Option<PathBuf>,
    state: String,
}

impl ExecutionMachines {
    /// The non-concurrent state a ticket in `state` occupies, or `None` when
    /// its own machine marks the state `concurrent`. The flag and the source
    /// both come from the ticket's own machine. §FS-rhei-run.5.2.1 §FS-rhei-states.1.2
    fn nonconcurrent_state(&self, task_id: &str, state: &str) -> Option<NonConcurrentState> {
        let concurrent = self
            .for_task_str(task_id)
            .states
            .get(state)
            .map(|def| def.concurrent)
            .unwrap_or(false);
        (!concurrent).then(|| NonConcurrentState {
            // The callback paths carry the canonical machine path each rhei
            // resolved, kept for a drifted member like its machine.
            source: self.callbacks_for_str(task_id).state_machine_path.clone(),
            state: state.to_string(),
        })
    }
}

/// One admission round's claims: the first ticket into a non-concurrent state
/// holds it for the round, and a state already in flight admits nobody.
/// Fan-out invocations of the claimant stay together. §FS-rhei-run.5.2 §FS-rhei-run.5.2.1
#[derive(Default)]
struct NonConcurrentClaims {
    in_flight: HashSet<NonConcurrentState>,
    claimant: HashMap<NonConcurrentState, String>,
}

impl NonConcurrentClaims {
    /// Claims for a refill, which also waits behind the work still running.
    fn behind(in_flight: &NonConcurrentInFlight) -> Self {
        Self { in_flight: in_flight.states(), claimant: HashMap::new() }
    }

    /// Whether `task_id` in `state` may start in this round; `false` defers it.
    fn admit(&mut self, machines: &ExecutionMachines, task_id: &str, state: &str) -> bool {
        let Some(key) = machines.nonconcurrent_state(task_id, state) else {
            return true;
        };
        if self.in_flight.contains(&key) {
            return false;
        }
        match self.claimant.get(&key) {
            Some(claimant) => claimant == task_id,
            None => {
                self.claimant.insert(key, task_id.to_string());
                true
            }
        }
    }
}

/// The non-concurrent work in flight. Each spawn is recorded under the key it
/// was admitted with, so its release frees exactly that state even if the
/// machines are re-resolved meanwhile. §FS-rhei-run.5.2.1
#[derive(Default)]
struct NonConcurrentInFlight {
    /// (ticket, state) → the key it holds and how many of its invocations run.
    held: HashMap<(String, String), (NonConcurrentState, usize)>,
}

impl NonConcurrentInFlight {
    /// Record one spawn of `task_id` in `state`, if that state is non-concurrent.
    fn hold(&mut self, machines: &ExecutionMachines, task_id: &str, state: &str) {
        if let Some(key) = machines.nonconcurrent_state(task_id, state) {
            self.held
                .entry((task_id.to_string(), state.to_string()))
                .or_insert((key, 0))
                .1 += 1;
        }
    }

    /// Release one finished spawn of `task_id` in `state`.
    fn release(&mut self, task_id: &str, state: &str) {
        let at = (task_id.to_string(), state.to_string());
        if let Some((_, count)) = self.held.get_mut(&at) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.held.remove(&at);
            }
        }
    }

    fn states(&self) -> HashSet<NonConcurrentState> {
        self.held.values().map(|(key, _)| key.clone()).collect()
    }
}
