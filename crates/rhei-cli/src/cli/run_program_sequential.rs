// Running this pass's programs one after another, the way a run with a single
// worker does it: spawn each, wait for it, and route the ticket on the exit
// code, the outputs it owes, or the timeout it hit.
//
// Its own part because a program in the worker pool is spawned and completed by
// the parts next door; only the single-worker path runs one inline and owns the
// whole of it, from the slot events to the transition it fires.

// §AR-source-file-size.3 §FS-rhei-run.3

/// Runs every program work item this pass claimed, in order.
///
/// An interrupt stops the loop where it is: the programs not yet started stay
/// unstarted, and the ones that ran keep the state they were worked in.
// §FS-rhei-run.3.2
#[allow(clippy::too_many_arguments)]
fn run_sequential_program_work_items(
    program_tasks: &[(String, String, String, ResolvedProgram)],
    plan_title: &str,
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    progress: &mut AgentPassProgress<'_>,
) -> MietteResult<()> {
    use rhei_tui::{MessageLevel, RunEvent};
    use std::time::{Instant as TuiInstant, SystemTime};
    macro_rules! run_message { ($level:expr, $($arg:tt)*) => {{ emit_run_message(sink, $level, format!($($arg)*)); }}; }
    macro_rules! run_info { ($($arg:tt)*) => { run_message!(MessageLevel::Info, $($arg)*); }; }
    macro_rules! run_warn { ($($arg:tt)*) => { run_message!(MessageLevel::Warn, $($arg)*); }; }
    macro_rules! run_error { ($($arg:tt)*) => { run_message!(MessageLevel::Error, $($arg)*); }; }
    for (task_id_str, _current_state_raw, current_state, resolved) in program_tasks {
        // The pass collected every ready program before the interrupt;
        // the ones not yet started stay unstarted. §FS-rhei-run.3.2
        if interrupt_requested() {
            break;
        }
        let loaded = load_plan(input)?;
        let target_id = parse_task_id(task_id_str);
        let machine = machines.for_task_str(task_id_str);
        let callback_paths = machines.callbacks_for_str(task_id_str);
        let task = find_task_by_id(&loaded.rhei.tasks, &target_id);
        let Some(task) = task else { continue };
        // §FS-rhei-panta.6.2: programs run against the owning rhei's root.
        let task_workspace_root = loaded.task_root(task_id_str, workspace_root);
        // §AR-rhei-panta.5: program transcripts and spawn records share the member runtime.
        let task_runtime_dir = task_workspace_root.join("runtime");
        let render_context = RuntimeTemplateContext {
            workspace_root: &task_workspace_root,
            task_roots: Some(&loaded.task_roots),
            plan_tasks: Some(&loaded.rhei.tasks),
            checkout_root: &task_workspace_root,
            plan_path: &callback_paths.plan_path,
            state_machine_path: callback_paths.state_machine_path.as_deref(),
            plan_title,
            task,
            state_name: current_state,
            current_state_raw: task.state.as_str(),
            machine,
            metadata: loaded.rhei.metadata.as_ref(),
            target: None,
            model: None,
            model_provider: None,
            model_name: None,
            agent: None,
            agent_mode: None,
            tooling: None,
            memory: None,
        };
        // A program is never skipped at scheduling, so it re-spawns for the
        // same reason an agent does — and gets the same attempt log and the
        // same per-visit budget. §FS-rhei-agents.8.1 §FS-rhei-agents.3.2.3
        let plan = plan_spawn_attempt(
            &task_runtime_dir,
            &task_workspace_root,
            task_id_str,
            current_state,
            None,
        );
        let budget = resolve_attempt_budget(machine.states.get(current_state.as_str()), settings);
        if let Some(spent_budget) = plan.budget_spent(budget) {
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
            continue;
        }
        let log = plan.log.clone();

        run_info!("\nSpawning program for Task {}: {}", task_id_str, task.title);
        run_info!("  Log: {}", log.display());
        // §FS-rhei-agents.3.2.1: a retry says it is one, and what it is retrying.
        if let Some(note) = plan.respawn_note(task_id_str, current_state, budget) {
            run_info!("{note}");
        }

        let started_at = std::time::Instant::now();
        let started_wall = std::time::SystemTime::now();
        sink.emit(RunEvent::SlotAssigned {
            slot: 0,
            task: task_id_str.clone(),
            from: task.state.as_str().to_string(),
            to: current_state.clone(),
            agent: None,
            template_context: None,
            log_path: log.clone(),
            started_at,
            wall_clock: started_wall,
        });

        let spawn_result =
            spawn_and_wait_program(resolved, &render_context, &log, &plan, sink);
        let duration_ms = started_at.elapsed().as_millis() as u64;
        let finished_wall = SystemTime::now();
        let (outcome, exit_code) = slot_outcome(&spawn_result);
        // Held rather than emitted: whether this attempt was a handled wait is
        // known only once the transition below is selected. §FS-rhei-states.2.2
        let mut release = PendingSlotRelease::hold(
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

        match spawn_result {
            // §FS-rhei-run.3.2: interrupted, so no transition fires.
            Ok(program_outcome) if program_outcome.interrupted => {
                *progress.programs_spawned += 1;
                run_warn!(
                    "{}",
                    interrupted_task_warning(task_id_str, current_state, Some(&log))
                );
            }
            Ok(program_outcome) => {
                *progress.programs_spawned += 1;
                let mut reloaded = load_plan(input)?;
                let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
                let mut state_after =
                    task_after.map(|t| t.state.as_str()).unwrap_or("unknown").to_string();

                if normalized_state_name(&state_after, machine)
                    != normalized_state_name(current_state, machine)
                {
                    run_info!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str,
                        current_state,
                        state_after
                    );
                    *progress.advanced_any = true;
                    continue;
                }

                if program_outcome.timed_out {
                    match fire_timeout_transition(
                        input,
                        machines,
                        task_id_str,
                        current_state,
                        program_outcome.timeout_secs,
                        opts,
                    ) {
                        TimeoutTransitionOutcome::Fired => {}
                        TimeoutTransitionOutcome::NoRule => {
                            run_warn!(
                                "  warning: program for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                task_id_str,
                                current_state
                            );
                        }
                        TimeoutTransitionOutcome::Failed => {}
                    }
                    reloaded = load_plan(input)?;
                    state_after = reloaded
                        .rhei
                        .tasks
                        .iter()
                        .find(|t| t.id == target_id)
                        .map(|t| t.state.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if normalized_state_name(&state_after, machine)
                        != normalized_state_name(current_state, machine)
                    {
                        run_info!(
                            "  Task {} advanced: '{}' -> '{}'",
                            task_id_str,
                            current_state,
                            state_after
                        );
                        *progress.advanced_any = true;
                        continue;
                    }
                    // Timed out and did not move: out of this pass.
                    // §FS-rhei-run.3
                    progress.stalled_tasks.insert(task_id_str.clone());
                    continue;
                }

                let exit_code = program_outcome.status.code().unwrap_or(-1);
                if let Some(exit_route) = find_program_exit_transition(
                    machine,
                    loaded.rhei.metadata.as_ref(),
                    task,
                    current_state,
                    exit_code,
                )? {
                    let to_state = exit_route.to.as_str();
                    // A declared route into a terminal state is judged on the
                    // ticket's result alone: the engine writes none in its
                    // place. §FS-rhei-run.3 §FS-rhei-programs.3.2
                    let missing_required_outputs = missing_program_exit_outputs(
                        workspace_root,
                        &reloaded.task_root(task_id_str, workspace_root),
                        machine,
                        reloaded.rhei.metadata.as_ref(),
                        task_after.unwrap_or(task),
                        current_state,
                        &exit_route,
                        exit_code,
                    );
                    if !missing_required_outputs.is_empty() {
                        // A program is a worker: its stall reaches
                        // the report as the artifacts it owes.
                        // §FS-rhei-run-report.3.1
                        emit_missing_required_outputs_warning(
                            "program",
                            task_id_str,
                            current_state,
                            exit_code,
                            &missing_required_outputs,
                            plan.retry_outlook(budget),
                            sink,
                        );
                        progress.stalled_tasks.insert(task_id_str.clone());
                        continue;
                    }
                    if record_poll_self_loop_if_needed(
                        &loaded,
                        input,
                        machine,
                        task,
                        current_state,
                        to_state,
                    )? {
                        // Not done yet, so the attempt releases as a wait.
                        // §FS-rhei-states.2.2
                        release.waiting();
                        run_info!(
                            "  Task {} poll self-loop scheduled next attempt from '{}'",
                            task_id_str,
                            current_state
                        );
                        *progress.advanced_any = true;
                        continue;
                    }
                    let route = loaded.task_route(task_id_str, input);
                    let effective_to = execute_system_program_exit_transition(
                        TransitionFiles {
                            task_file: &route.task_file,
                            metadata_file: &route.metadata_file,
                            metadata_id: &route.metadata_id,
                            artifact_root: &route.execution_root,
                            artifact_id: task_id_str,
                        },
                        callback_paths,
                        machine,
                        &route.local_id,
                        current_state,
                        to_state,
                        exit_code,
                        exit_route.matched,
                        opts.no_callbacks(),
                    )?;
                    run_info!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str,
                        current_state,
                        effective_to
                    );
                    *progress.advanced_any = true;
                } else if program_outcome.status.success() {
                    run_warn!(
                        "  warning: program exited 0 but task {} did not advance from '{}'",
                        task_id_str,
                        current_state
                    );
                    progress.stalled_tasks.insert(task_id_str.clone());
                } else {
                    run_error!(
                        "  error: program exited with code {} for task {}",
                        exit_code,
                        task_id_str
                    );
                    if !opts.continue_on_error() {
                        return Err(miette!(
                            help = program_state_failed_help(),
                            "program exited with code {} for Task {}. Use --continue-on-error to skip failures.",
                            exit_code,
                            task_id_str
                        ));
                    }
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
            }
            Err(err) => {
                run_error!("  error: {}", err);
                if !opts.continue_on_error() {
                    return Err(err);
                }
                progress.stalled_tasks.insert(task_id_str.clone());
            }
        }
    }

    Ok(())
}
