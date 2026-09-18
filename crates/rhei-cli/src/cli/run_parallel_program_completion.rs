// What a parallel program's exit means for its ticket: the transition its exit
// code selects, the required outputs it still owes, and the warning left behind
// when nothing routes it anywhere.
//
// Its own part because a program's completion is decided from its exit code and
// declared outputs alone — no prompt, no snapshot, no accounting — which is
// what separates it from the agent completion beside it.

// §AR-source-file-size.3 §FS-rhei-run.3

struct ParallelProgramCompletionEffect {
    advanced: bool,
    program_spawned: bool,
}

fn handle_parallel_program_completion(
    input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    workspace_root: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    completion: ParallelProgramCompletion,
) -> MietteResult<ParallelProgramCompletionEffect> {
    let ParallelProgramCompletion {
        task_id_str,
        state_name,
        retry_outlook,
        // Read off the finished process by the worker, left to the thread that
        // learns whether the attempt was a handled wait, and emitted when this
        // function returns by whichever of its paths. §FS-rhei-states.2.2
        mut release,
        result,
        slot: _,
    } = completion;
    // The completed item's owning rhei supplies its machine and callback base.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&task_id_str);
    let callback_paths = machines.callbacks_for_str(&task_id_str);

    match result {
        // §FS-rhei-run.3.2: the run ended this program; no transition fires and
        // the ticket keeps its state.
        Ok(program_outcome) if program_outcome.interrupted => {
            emit_run_message(
                sink,
                rhei_tui::MessageLevel::Warn,
                interrupted_task_warning(&task_id_str, &state_name, None),
            );
            Ok(ParallelProgramCompletionEffect { advanced: false, program_spawned: true })
        }
        Ok(program_outcome) => {
            let mut advanced = false;
            let target_id = parse_task_id(&task_id_str);
            let mut reloaded = load_plan(input)?;
            let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
            let mut state_after =
                task_after.map(|task| task.state.as_str()).unwrap_or("unknown").to_string();

            if normalized_state_name(&state_after, machine)
                != normalized_state_name(&state_name, machine)
            {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Info,
                    format!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str, state_name, state_after
                    ),
                );
                return Ok(ParallelProgramCompletionEffect {
                    advanced: true,
                    program_spawned: true,
                });
            }

            if program_outcome.timed_out {
                match fire_timeout_transition(
                    input,
                    machines,
                    &task_id_str,
                    &state_name,
                    program_outcome.timeout_secs,
                    opts,
                ) {
                    TimeoutTransitionOutcome::Fired => {}
                    TimeoutTransitionOutcome::NoRule => {
                        emit_run_message(
                            sink,
                            rhei_tui::MessageLevel::Warn,
                            format!(
                                "  warning: program for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                                task_id_str, state_name
                            ),
                        );
                    }
                    TimeoutTransitionOutcome::Failed => {}
                }
                reloaded = load_plan(input)?;
                state_after = reloaded
                    .rhei
                    .tasks
                    .iter()
                    .find(|task| task.id == target_id)
                    .map(|task| task.state.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                if normalized_state_name(&state_after, machine)
                    != normalized_state_name(&state_name, machine)
                {
                    emit_run_message(
                        sink,
                        rhei_tui::MessageLevel::Info,
                        format!(
                            "  Task {} advanced: '{}' -> '{}'",
                            task_id_str, state_name, state_after
                        ),
                    );
                    advanced = true;
                }
                return Ok(ParallelProgramCompletionEffect {
                    advanced,
                    program_spawned: true,
                });
            }

            let exit_code = program_outcome.status.code().unwrap_or(-1);
            let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
            let Some(task) = task_after else {
                return Ok(ParallelProgramCompletionEffect {
                    advanced,
                    program_spawned: true,
                });
            };

            if let Some(exit_route) = find_program_exit_transition(
                machine,
                reloaded.rhei.metadata.as_ref(),
                task,
                &state_name,
                exit_code,
            )? {
                let to_state = exit_route.to.as_str();
                // The worker pool judges a declared route exactly as the
                // single-worker path does, or the two drift on what a routed
                // exit leaves behind. §FS-rhei-run.3 §FS-rhei-programs.3.2
                let missing_required_outputs = missing_program_exit_outputs(
                    workspace_root,
                    &reloaded.task_root(&task_id_str, workspace_root),
                    machine,
                    reloaded.rhei.metadata.as_ref(),
                    task,
                    &state_name,
                    &exit_route,
                    exit_code,
                );
                if !missing_required_outputs.is_empty() {
                    // A program is a worker like any other: its stall must
                    // reach the run report as the artifacts it owes, not as
                    // a nameless one. §FS-rhei-run-report.3.1
                    emit_missing_required_outputs_warning(
                        "program",
                        &task_id_str,
                        &state_name,
                        exit_code,
                        &missing_required_outputs,
                        retry_outlook,
                        sink,
                    );
                    return Ok(ParallelProgramCompletionEffect {
                        advanced,
                        program_spawned: true,
                    });
                }
                if record_poll_self_loop_if_needed(
                    &reloaded,
                    input,
                    machine,
                    task,
                    &state_name,
                    to_state,
                )? {
                    // Not done yet, so the attempt releases as a wait.
                    // §FS-rhei-states.2.2
                    release.waiting();
                    emit_run_message(
                        sink,
                        rhei_tui::MessageLevel::Info,
                        format!(
                            "  Task {} poll self-loop scheduled next attempt from '{}'",
                            task_id_str, state_name
                        ),
                    );
                    return Ok(ParallelProgramCompletionEffect {
                        advanced: true,
                        program_spawned: true,
                    });
                }
                let route = reloaded.task_route(&task_id_str, input);
                execute_system_program_exit_transition(
                    TransitionFiles {
                        task_file: &route.task_file,
                        metadata_file: &route.metadata_file,
                        metadata_id: &route.metadata_id,
                        artifact_root: &route.execution_root,
                        artifact_id: &task_id_str,
                    },
                    callback_paths,
                    machine,
                    &route.local_id,
                    &state_name,
                    to_state,
                    exit_code,
                    exit_route.matched,
                    opts.no_callbacks(),
                )?;
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Info,
                    format!(
                        "  Task {} advanced: '{}' -> '{}'",
                        task_id_str, state_name, to_state
                    ),
                );
                advanced = true;
            } else if program_outcome.status.success() {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Warn,
                    format!(
                        "  warning: program exited 0 but task {} did not advance from '{}'",
                        task_id_str, state_name
                    ),
                );
            } else {
                emit_run_message(
                    sink,
                    rhei_tui::MessageLevel::Error,
                    format!(
                        "  error: program exited with code {} for task {}",
                        exit_code, task_id_str
                    ),
                );
                if !opts.continue_on_error() {
                    return Err(miette!(
                        help = program_state_failed_help(),
                        "program exited with code {} for Task {}. Use --continue-on-error to skip failures.",
                        exit_code,
                        task_id_str
                    ));
                }
            }

            Ok(ParallelProgramCompletionEffect {
                advanced,
                program_spawned: true,
            })
        }
        Err(err) => {
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  error: {}", err));
            if !opts.continue_on_error() {
                return Err(err);
            }
            Ok(ParallelProgramCompletionEffect {
                advanced: false,
                program_spawned: false,
            })
        }
    }
}

