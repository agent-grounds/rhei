// The parallel worker pool: how a pass fills its slots, what it does with each
// result that comes back over the channel, and how it refills freed capacity
// from the live ready set until the pass has nothing left in flight.
//
// Its own part because the pool is where concurrency is decided — slots,
// channel, refill, and the join at the end. What a finished agent's exit means
// is the part next door, and it reads the same for one worker or eight.

// §AR-source-file-size.3 §FS-rhei-run.3

/// Runs this pass's batch through the worker pool and returns once every
/// spawned worker has been accounted for.
// §FS-rhei-run.3
#[allow(clippy::too_many_arguments)]
fn run_agent_worker_pool(
    batch: &[(String, String, String, ResolvedAgent)],
    program_tasks: &[(String, String, String, ResolvedProgram)],
    run_programs_in_worker_pool: bool,
    task_limit: usize,
    frontend_parallel: rhei_tui::Slot,
    pass: u32,
    input: &Path,
    live: &mut LiveRunContext,
    opts: &RunOptions,
    workspace_root: &Path,
    runtime_dir: &Path,
    run_id: &str,
    snapshot_override_selection: Option<&SnapshotOverrideRunSelection>,
    sink: &Arc<dyn rhei_tui::EventSink>,
    intervene: Option<&Arc<RunInterveneSink>>,
    progress: &mut AgentPassProgress<'_>,
    identity: &RunIdentity,
) -> MietteResult<()> {
    use rhei_tui::MessageLevel;
    macro_rules! run_message { ($level:expr, $($arg:tt)*) => {{ emit_run_message(sink, $level, format!($($arg)*)); }}; }
    macro_rules! run_warn { ($($arg:tt)*) => { run_message!(MessageLevel::Warn, $($arg)*); }; }
    macro_rules! run_error { ($($arg:tt)*) => { run_message!(MessageLevel::Error, $($arg)*); }; }
    // Parallel worker pool: each worker reports completion over a
    // channel, and the scheduler refills freed capacity after every
    // processed result. Re-reading the plan preserves dependency checks. §FS-rhei-run.3
    let (tx, rx) = std::sync::mpsc::channel::<ParallelAgentThreadMessage>();
    let mut handles = Vec::new();
    let mut free_slots: BTreeSet<rhei_tui::Slot> = (0..frontend_parallel).collect();
    let mut next_extra_slot = frontend_parallel;
    let mut active_invocation_counts: HashMap<String, usize> = HashMap::new();
    let mut active_state_counts: HashMap<String, usize> = HashMap::new();
    let initial_program_items = if run_programs_in_worker_pool {
        program_tasks
            .iter()
            .map(|(task_id_str, _current_state_raw, current_state, resolved)| ProgramWorkItem {
                task_id_str: task_id_str.clone(),
                current_state: current_state.clone(),
                resolved: resolved.clone(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let program_schedule_outcome = schedule_program_work_items(
        initial_program_items,
        task_limit,
        &tx,
        input,
        &live.machines,
        &live.settings,
        workspace_root,
        sink,
        &mut free_slots,
        &mut next_extra_slot,
        &mut active_invocation_counts,
        &mut active_state_counts,
        &mut handles,
    )?;
    *progress.advanced_any |= program_schedule_outcome.advanced;
    // Nothing started and nothing routed: without this the refill
    // re-attempts them on every freed slot. §FS-rhei-run.3
    progress.stalled_tasks.extend(program_schedule_outcome.skipped.iter().cloned());

    let agent_capacity = if task_limit == usize::MAX {
        usize::MAX
    } else {
        task_limit.saturating_sub(active_invocation_counts.len())
    };
    let initial_items = batch
        .iter()
        .map(|(task_id_str, _current_state_raw, current_state, resolved)| AgentWorkItem {
            task_id_str: task_id_str.clone(),
            current_state: current_state.clone(),
            resolved: resolved.clone(),
        })
        .collect::<Vec<_>>();
    let schedule_outcome = schedule_agent_work_items(
        initial_items,
        agent_capacity,
        &mut *progress.unpromptable_tasks,
        &tx,
        input,
        &live.machines,
        &live.settings,
        opts,
        workspace_root,
        runtime_dir,
        run_id,
        snapshot_override_selection,
        sink,
        intervene,
        &mut free_slots,
        &mut next_extra_slot,
        &mut active_invocation_counts,
        &mut active_state_counts,
        &mut handles,
    )?;
    *progress.advanced_any |= schedule_outcome.advanced;
    progress.stalled_tasks.extend(schedule_outcome.skipped.iter().cloned());
    let mut active_worker_count =
        program_schedule_outcome.spawned + schedule_outcome.spawned;

    while active_worker_count > 0 {
        // A provider wait must refill free capacity without waiting for an
        // unrelated worker to exit. Re-read in bounded slices, as the outer
        // sleep does, so persisted deadline edits also wake us. §FS-rhei-run.3.3
        let provider_wait = active_invocation_counts.len() < task_limit
            && has_pending_provider_wait(&load_plan(input)?.rhei, &live.machines.set);
        let message = if provider_wait {
            rx.recv_timeout(Duration::from_secs(1))
        } else {
            rx.recv().map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
        };
        let completion = match message {
            Ok(ParallelAgentThreadMessage::Completed(completion)) => completion,
            Ok(ParallelAgentThreadMessage::ProgramCompleted(completion)) => {
                active_worker_count = active_worker_count.saturating_sub(1);
                release_parallel_worker(
                    &completion.task_id_str,
                    &completion.state_name,
                    completion.slot,
                    &mut free_slots,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                );
                let completed_task_id = completion.task_id_str.clone();
                let effect = handle_parallel_program_completion(
                    input,
                    &live.machines,
                    opts,
                    workspace_root,
                    sink,
                    completion,
                )?;
                if effect.program_spawned {
                    *progress.programs_spawned += 1;
                }
                if !effect.advanced {
                    progress.stalled_tasks.insert(completed_task_id);
                }
                *progress.advanced_any |= effect.advanced;
                let refill_outcome = refill_parallel_worker_pool(
                    &mut *progress.unpromptable_tasks,
                    &*progress.stalled_tasks,
                    pass,
                    task_limit,
                    &tx,
                    input,
                    live,
                    opts,
                    workspace_root,
                    runtime_dir,
                    run_id,
                    snapshot_override_selection,
                    sink,
                    intervene,
                    &mut free_slots,
                    &mut next_extra_slot,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                    &mut handles,
                    identity,
                )?;
                active_worker_count += refill_outcome.spawned;
                *progress.advanced_any |= refill_outcome.advanced;
                progress.stalled_tasks.extend(refill_outcome.skipped.iter().cloned());
                continue;
            }
            Ok(ParallelAgentThreadMessage::Panicked {
                task_id_str,
                state_name,
                slot,
            }) => {
                active_worker_count = active_worker_count.saturating_sub(1);
                release_parallel_worker(
                    &task_id_str,
                    &state_name,
                    slot,
                    &mut free_slots,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                );
                let err = miette!(
                    help = internal_error_help(),
                    "agent thread panicked"
                );
                run_error!("  error for task {}: {}", task_id_str, err);
                if !opts.continue_on_error() {
                    return Err(err);
                }
                // The ticket is untouched and still ready; without this
                // the refill re-spawns the thread that just panicked.
                // §FS-rhei-run.3
                progress.stalled_tasks.insert(task_id_str.clone());
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The ordinary refill preserves capacity, authored readiness,
                // active workers, and effective poll/provider eligibility.
                // §FS-rhei-run.3.3 §FS-rhei-run.5.1
                let refill_outcome = refill_parallel_worker_pool(
                    &mut *progress.unpromptable_tasks,
                    &*progress.stalled_tasks,
                    pass,
                    task_limit,
                    &tx,
                    input,
                    live,
                    opts,
                    workspace_root,
                    runtime_dir,
                    run_id,
                    snapshot_override_selection,
                    sink,
                    intervene,
                    &mut free_slots,
                    &mut next_extra_slot,
                    &mut active_invocation_counts,
                    &mut active_state_counts,
                    &mut handles,
                    identity,
                )?;
                active_worker_count += refill_outcome.spawned;
                *progress.advanced_any |= refill_outcome.advanced;
                progress.stalled_tasks.extend(refill_outcome.skipped.iter().cloned());
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };

        active_worker_count = active_worker_count.saturating_sub(1);
        release_parallel_worker(
            &completion.task_id_str,
            &completion.state_name,
            completion.slot,
            &mut free_slots,
            &mut active_invocation_counts,
            &mut active_state_counts,
        );

        let ParallelAgentCompletion {
            task_id_str,
            state_name,
            started_at,
            // Read off the finished agent by the worker, left to the thread
            // that learns whether the attempt was a handled wait, and emitted
            // by whoever owns it last. §FS-rhei-states.2.2
            release,
            resolved,
            log,
            snapshot_preload,
            visit_count,
            retry_outlook,
            subtree_before,
            spawn_record,
            budget_lease,
            result,
            accounting_recorded,
            accounting_warning,
            slot: _,
        } = completion;
        // §FS-rhei-cost-accounting.11: Parallel accounting failures still warn.
        if let Some(warning) = accounting_warning {
            run_warn!("  warning: failed to record accounting: {}", warning);
        }
        match result {
            // §FS-rhei-run.3.2: interrupted, so no transition fires and
            // the ticket keeps the state it was worked in.
            Ok(AgentSpawnOutcome { interrupted: true, .. }) => {
                // Nothing below can change what this release says, and it
                // must go out before the refill takes the slot back.
                // §FS-rhei-run-tui.1.7
                drop(release);
                *progress.agents_spawned += 1;
                run_warn!(
                    "{}",
                    interrupted_task_warning(&task_id_str, &state_name, Some(&log))
                );
            }
            Ok(outcome) => {
                handle_parallel_agent_exit(
                    ParallelAgentExit {
                        task_id_str,
                        state_name,
                        started_at,
                        release,
                        resolved,
                        log,
                        snapshot_preload,
                        visit_count,
                        retry_outlook,
                        subtree_before,
                        spawn_record,
                        budget_lease,
                        accounting_recorded,
                        outcome,
                    },
                    input,
                    &live.machines,
                    &live.settings,
                    opts,
                    workspace_root,
                    sink,
                    &active_invocation_counts,
                    progress,
                )?;
            }
            Err(err) => {
                // The spawn itself failed, so there is no exit to route and
                // no verdict left to wait for. Out before the refill, as
                // above. §FS-rhei-run-tui.1.7
                drop(release);
                if accounting_recorded {
                    let reloaded = load_plan(input)?;
                    if let Err(rollup_err) =
                        regenerate_accounting_indexes(workspace_root, &reloaded.rhei)
                    {
                        run_warn!(
                            "  warning: failed to update accounting rollups: {}",
                            rollup_err
                        );
                    }
                }
                run_error!("  error for task {}: {}", task_id_str, err);
                if !opts.continue_on_error() {
                    return Err(err);
                }
                // Spawning failed and the ticket has not moved; refilling
                // from the live ready set would just spawn it again.
                // §FS-rhei-run.3
                progress.stalled_tasks.insert(task_id_str.clone());
            }
        }

        let refill_outcome = refill_parallel_worker_pool(
            &mut *progress.unpromptable_tasks,
            &*progress.stalled_tasks,
            pass,
            task_limit,
            &tx,
            input,
            live,
            opts,
            workspace_root,
            runtime_dir,
            run_id,
            snapshot_override_selection,
            sink,
            intervene,
            &mut free_slots,
            &mut next_extra_slot,
            &mut active_invocation_counts,
            &mut active_state_counts,
            &mut handles,
            identity,
        )?;
        active_worker_count += refill_outcome.spawned;
        *progress.advanced_any |= refill_outcome.advanced;
        progress.stalled_tasks.extend(refill_outcome.skipped.iter().cloned());
    }

    for handle in handles {
        if handle.join().is_err() {
            let err = miette!(
                help = internal_error_help(),
                "agent thread panicked"
            );
            run_error!("  error: {}", err);
            if !opts.continue_on_error() {
                return Err(err);
            }
        }
    }

    Ok(())
}
