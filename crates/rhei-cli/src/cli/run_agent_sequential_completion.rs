// What a sequential agent's exit means for its ticket: whether it met the
// completion condition, which snapshots its outcome emits, and which edge —
// advance, timeout, or exit code — the run fires next.
//
// Its own part because the decision is made from the finished invocation alone;
// nothing here spawns anything. The parallel pool reaches the same decision
// from a completion that crossed a channel, which is why that path is separate.

// §AR-source-file-size.3 §FS-rhei-run.3 §FS-rhei-agents.3.2

/// One finished sequential agent invocation, with the facts its post-exit
/// handling reads. Mirrors `ParallelAgentCompletion`, which carries the same
/// facts back from a worker thread.
struct SequentialAgentCompletion<'a> {
    task_id_str: String,
    state_name: String,
    task: &'a rhei_core::ast::Task,
    /// The slot release read off the finished agent, not emitted yet: a poll
    /// state's self-loop makes the attempt a wait, and the transition that says
    /// so is selected below. §FS-rhei-states.2.2
    release: PendingSlotRelease,
    task_workspace_root: PathBuf,
    resolved: &'a ResolvedAgent,
    log: PathBuf,
    snapshot_preload: SnapshotPreload,
    visit_count: u64,
    /// Whether the visit this invocation belongs to has an attempt left after
    /// it. Decided at the spawn, because that is where the budget is resolved,
    /// and read here, where the run says what it will do next.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
    retry_outlook: RetryOutlook,
    /// The supervisor's subtree as this spawn found it, for a supervising
    /// state; `None` for every other. §FS-rhei-supervision.3.6
    subtree_before: Option<SubtreeShape>,
    /// Where this spawn recorded itself, so a withheld release edge can give
    /// back the attempt it was charged. §FS-rhei-agents.3.2.3
    spawn_record: PathBuf,
    result: MietteResult<AgentSpawnOutcome>,
}

#[allow(clippy::too_many_arguments)]
fn handle_sequential_agent_completion(
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    loaded: &LoadedPlan,
    sink: &Arc<dyn rhei_tui::EventSink>,
    completion: SequentialAgentCompletion<'_>,
    progress: &mut AgentPassProgress<'_>,
) -> MietteResult<()> {
    use rhei_tui::MessageLevel;
    macro_rules! run_message { ($level:expr, $($arg:tt)*) => {{ emit_run_message(sink, $level, format!($($arg)*)); }}; }
    macro_rules! run_info { ($($arg:tt)*) => { run_message!(MessageLevel::Info, $($arg)*); }; }
    macro_rules! run_warn { ($($arg:tt)*) => { run_message!(MessageLevel::Warn, $($arg)*); }; }
    macro_rules! run_error { ($($arg:tt)*) => { run_message!(MessageLevel::Error, $($arg)*); }; }

    let SequentialAgentCompletion {
        task_id_str,
        state_name,
        task,
        mut release,
        task_workspace_root,
        resolved,
        log,
        snapshot_preload,
        visit_count,
        retry_outlook,
        subtree_before,
        spawn_record,
        result: spawn_result,
    } = completion;
    let task_id_str = &task_id_str;
    let current_state = &state_name;
    let target_id = parse_task_id(task_id_str);
    // The ticket's own machine drives its post-exit handling; callbacks resolve
    // inside each helper from the same set. §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    match spawn_result {
        // The run is shutting down: no transition fires, the ticket
        // keeps its state, and the next `rhei run` re-executes it.
        // §FS-rhei-run.3.2
        Ok(AgentSpawnOutcome { interrupted: true, .. }) => {
            *progress.agents_spawned += 1;
            run_warn!(
                "{}",
                interrupted_task_warning(task_id_str, current_state, Some(&log))
            );
        }
        Ok(AgentSpawnOutcome {
            status,
            timed_out,
            timeout_secs,
            provider_limit,
            ..
        }) => {
            *progress.agents_spawned += 1;
            let state_def = machine.states.get(current_state).ok_or_else(|| {
                miette!(
                    help = internal_error_help(),
                    "state '{}' missing from loaded machine", current_state
                )
            })?;
            // §FS-rhei-agents.3.2: the completion condition is exit 0 +
            // declared outputs + the terminal result when the edge this
            // exit selects lands on a `final: true` state.
            let reloaded = load_plan(input)?;
            let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
            let stayed_in_state = task_after.is_some_and(|task| {
                normalized_state_name(task.state.as_str(), machine)
                    == normalized_state_name(current_state, machine)
            });
            // Provider classification outranks generic failure and snapshots; persist only while
            // the task occupies the reporting state. §FS-rhei-agents.5.2.1 §FS-rhei-run.3.3
            if let Some(observed) = provider_limit {
                let effective = if stayed_in_state {
                    persist_provider_limit(
                        input,
                        &reloaded,
                        task_id_str,
                        current_state,
                        &observed,
                    )?
                } else {
                    observed
                };
                release.provider_limited(&effective);
                run_info!(
                    "  Task {} provider-limited by {} until {}; state unchanged.",
                    task_id_str,
                    effective.identity.provider,
                    effective.next_attempt_at
                );
                return Ok(());
            }
            if status.success() && stayed_in_state {
                clear_persisted_provider_limit(input, &reloaded, task_id_str, current_state)?;
            }
            // Condition (3) selects that edge against the plan as re-read here,
            // so a child this invocation appended is an open descendant of it.
            // §FS-rhei-agents.3.2 §FS-rhei-supervision.4.1
            let selected_to = selected_forward_transition_from(
                &reloaded.rhei,
                machine,
                // A ticket the invocation deleted has no post-exit shape to
                // judge; fall back rather than invent one. §FS-rhei-agents.3.2
                task_after.unwrap_or(task),
                // The re-read plan supplies the operands, never the from-state:
                // this exit leaves the state the invocation ran in, whatever the
                // invocation moved its own ticket to. §FS-rhei-agents.3.2
                task.state.as_str(),
            );
            let outputs_ok = status.success()
                && state_outputs_exist_for_resolved_invocation(
                    &task_workspace_root,
                    task,
                    current_state,
                    task.state.as_str(),
                    machine,
                    loaded.rhei.metadata.as_ref(),
                    state_def,
                    resolved,
                )
                && missing_terminal_result_output(
                    &task_workspace_root,
                    machine,
                    task,
                    selected_to.as_deref(),
                    // A fanned-out invocation answers for its own
                    // fragment. §FS-rhei-states.3.3
                    ResultInvocation {
                        state: current_state,
                        visit_count,
                        identity: fanout_result_identity(
                            Some(state_def),
                            resolved.target.as_ref(),
                            resolved.model.as_deref(),
                        )
                        .as_deref(),
                    },
                )
                .is_none();
            let missing_required_outputs = if status.success() && !outputs_ok {
                collect_missing_required_outputs_for_resolved_invocation(
                    &task_workspace_root,
                    machine,
                    loaded.rhei.metadata.as_ref(),
                    task,
                    current_state,
                    selected_to.as_deref(),
                    resolved,
                )
            } else {
                Vec::new()
            };
            let snapshot_completion = if timed_out {
                SnapshotCompletion::Timeout
            } else if outputs_ok {
                SnapshotCompletion::Success
            } else {
                SnapshotCompletion::Failure
            };
            let failure_selected_to_state = if timed_out {
                find_timeout_transition(machine, current_state)
            } else if !status.success() {
                // Load refuses an `exit_code:` edge from a state with no `program:`,
                // so an agent reaches this only by a poll exhaustion edge, whose
                // classification the caller never reads. §FS-rhei-run.5.1
                find_program_exit_transition(
                    machine,
                    loaded.rhei.metadata.as_ref(),
                    task,
                    current_state,
                    status.code().unwrap_or(-1),
                )?
                .map(|route| route.to)
            } else {
                None
            };
            if !status.success() {
                if let Err(err) = emit_snapshots_after_agent_exit(
                    workspace_root,
                    machine,
                    settings,
                    task,
                    current_state,
                    failure_selected_to_state.as_deref(),
                    resolved,
                    &log,
                    visit_count,
                    snapshot_completion,
                    &snapshot_preload,
                ) {
                    run_error!("  error: {}", err);
                    if !opts.continue_on_error() {
                        return Err(err);
                    }
                }
            }
            let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
            let state_before = current_state.as_str();

            // Compare normalized state names: a counted state and its
            // visit-suffixed form (e.g. `build` vs `build-2`) are the
            // same logical state. Comparing raw vs. normalized would
            // mistake a no-op re-entry for forward progress and skip
            // the real auto-advance, spinning the loop forever.
            if normalized_state_name(state_after, machine)
                != normalized_state_name(state_before, machine)
            {
                if status.success() {
                    if let Some(task_for_snapshot) = task_after {
                        if let Err(err) = emit_snapshots_after_agent_exit(
                            workspace_root,
                            machine,
                            settings,
                            task_for_snapshot,
                            state_before,
                            Some(state_after),
                            resolved,
                            &log,
                            visit_count,
                            snapshot_completion,
                            &snapshot_preload,
                        ) {
                            run_error!("  error: {}", err);
                            if !opts.continue_on_error() {
                                return Err(err);
                            }
                        }
                    }
                }
                run_info!(
                    "  Task {} advanced: '{}' -> '{}'",
                    task_id_str,
                    state_before,
                    state_after
                );
                *progress.advanced_any = true;
            } else if status.success() {
                // A stall ends this ticket's pass, never the run's: the
                // pass carries on with the tickets beside it, exactly as
                // the worker pool does. §FS-rhei-run.3
                'exit_zero: {
                    if !missing_required_outputs.is_empty() {
                        if let Some(task_for_snapshot) = task_after {
                            if let Err(err) = emit_snapshots_after_agent_exit(
                                workspace_root,
                                machine,
                                settings,
                                task_for_snapshot,
                                state_before,
                                None,
                                resolved,
                                &log,
                                visit_count,
                                snapshot_completion,
                                &snapshot_preload,
                            ) {
                                run_error!("  error: {}", err);
                                if !opts.continue_on_error() {
                                    return Err(err);
                                }
                            }
                        }
                        // An agent reaches this block only on a zero exit.
                        // §FS-rhei-agents.3.2
                        emit_missing_required_outputs_warning(
                            "agent",
                            task_id_str,
                            state_before,
                            0,
                            &missing_required_outputs,
                            retry_outlook,
                            sink,
                        );
                        progress.stalled_tasks.insert(task_id_str.clone());
                        break 'exit_zero;
                    }
                    let pending_more = machine
                        .states
                        .get(state_before)
                        .map(|state_def| {
                            task_has_pending_agent_invocations(
                                &task_workspace_root,
                                task,
                                state_before,
                                task.state.as_str(),
                                machine,
                                loaded.rhei.metadata.as_ref(),
                                state_def,
                                settings,
                                selected_to.as_deref(),
                            )
                        })
                        .transpose()?
                        .unwrap_or(false);
                    if pending_more {
                        if let Some(task_for_snapshot) = task_after {
                            if let Err(err) = emit_snapshots_after_agent_exit(
                                workspace_root,
                                machine,
                                settings,
                                task_for_snapshot,
                                state_before,
                                None,
                                resolved,
                                &log,
                                visit_count,
                                snapshot_completion,
                                &snapshot_preload,
                            ) {
                                run_error!("  error: {}", err);
                                if !opts.continue_on_error() {
                                    return Err(err);
                                }
                            }
                        }
                        break 'exit_zero;
                    }
                    let mut emit_before_transition =
                        |task_for_snapshot: &rhei_core::ast::Task,
                         to_state: &str|
                         -> MietteResult<()> {
                            emit_snapshots_after_agent_exit(
                                workspace_root,
                                machine,
                                settings,
                                task_for_snapshot,
                                state_before,
                                Some(to_state),
                                resolved,
                                &log,
                                visit_count,
                                snapshot_completion,
                                &snapshot_preload,
                            )
                        };
                    // §FS-rhei-supervision.3.6: a visit that released nothing
                    // must not spend the self-loop that releases the subtree.
                    let empty_visit = withhold_empty_supervising_visit(FinishedVisit {
                        workspace_root,
                        plan: &reloaded,
                        machines: &machines.set,
                        task_id: &target_id,
                        state: state_before,
                        before: subtree_before.as_ref(),
                        spawn_record: Some(&spawn_record),
                    });
                    let advance = match empty_visit {
                        Some(_) => Ok(None),
                        None => try_auto_advance_task(
                            input,
                            machines,
                            task_id_str,
                            state_before,
                            opts.no_callbacks(),
                            Some(&mut emit_before_transition),
                        ),
                    };
                    match advance {
                        Ok(Some(advance)) => {
                            // The engine scheduled another attempt rather than
                            // moving the ticket, so the attempt that selected
                            // the edge is a wait. §FS-rhei-states.2.2
                            if advance.poll_wait {
                                release.waiting();
                            }
                            run_info!(
                                "  Task {} auto-advanced: '{}' -> '{}'",
                                task_id_str,
                                state_before,
                                advance.to
                            );
                            *progress.advanced_any = true;
                        }
                        Ok(None) => {
                            if let Some(task_for_snapshot) = task_after {
                                if let Err(err) = emit_snapshots_after_agent_exit(
                                    workspace_root,
                                    machine,
                                    settings,
                                    task_for_snapshot,
                                    state_before,
                                    None,
                                    resolved,
                                    &log,
                                    visit_count,
                                    snapshot_completion,
                                    &snapshot_preload,
                                ) {
                                    run_error!("  error: {}", err);
                                    if !opts.continue_on_error() {
                                        return Err(err);
                                    }
                                }
                            }
                            match &empty_visit {
                                // §FS-rhei-supervision.3.6: the edge was
                                // withheld, so say that, not "did not advance".
                                Some(warning) => {
                                    run_warn!("{}", warning);
                                }
                                None => emit_exit_zero_warnings(
                                    workspace_root,
                                    &task_workspace_root,
                                    machine,
                                    loaded.rhei.metadata.as_ref(),
                                    task,
                                    task_id_str,
                                    state_before,
                                    // The same post-exit edge the completion
                                    // condition read. §FS-rhei-agents.3.2
                                    selected_to.as_deref(),
                                    retry_outlook,
                                    sink,
                                ),
                            }
                            // Did not move; the rest of the pass must look
                            // elsewhere. §FS-rhei-run.3
                            progress.stalled_tasks.insert(task_id_str.clone());
                        }
                        Err(err) => {
                            run_warn!(
                                "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                                task_id_str, state_before, err
                            );
                            progress.stalled_tasks.insert(task_id_str.clone());
                        }
                    }
                }
            } else if timed_out {
                let duration = timeout_secs.map(format_duration_human).unwrap_or_default();
                run_warn!("  agent timed out after {} for task {}", duration, task_id_str);
                if let Some(to_state) = failure_selected_to_state.as_deref() {
                    match fire_selected_timeout_transition(
                        input,
                        machines,
                        task_id_str,
                        state_before,
                        to_state,
                        timeout_secs,
                        opts.no_callbacks(),
                    ) {
                        TimeoutTransitionOutcome::Fired => *progress.advanced_any = true,
                        // Nowhere to go: the ticket is out of this
                        // pass, not out of the run. §FS-rhei-run.3
                        TimeoutTransitionOutcome::NoRule
                        | TimeoutTransitionOutcome::Failed => {
                            progress.stalled_tasks.insert(task_id_str.clone());
                        }
                    }
                } else {
                    {
                        run_warn!(
                            "  warning: agent for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                            task_id_str, state_before
                        );
                        progress.stalled_tasks.insert(task_id_str.clone());
                    }
                }
            } else {
                let code = status.code().unwrap_or(-1);
                run_error!(
                    "  error: agent exited with code {} for task {}",
                    code,
                    task_id_str
                );
                if let Some(to_state) = failure_selected_to_state.as_deref() {
                    match fire_agent_exit_transition(
                        input,
                        machines,
                        task_id_str,
                        state_before,
                        to_state,
                        code,
                        opts.no_callbacks(),
                    ) {
                        TimeoutTransitionOutcome::Fired => *progress.advanced_any = true,
                        TimeoutTransitionOutcome::NoRule
                        | TimeoutTransitionOutcome::Failed => {
                            progress.stalled_tasks.insert(task_id_str.clone());
                        }
                    }
                } else if !opts.continue_on_error() {
                    return Err(miette!(
                        help = run_report_help(),
                        "agent '{}' exited with code {} for Task {}. \
                         Use --continue-on-error to skip failures.",
                        resolved.agent.id(),
                        code,
                        task_id_str
                    ));
                } else {
                    // `--continue-on-error` skips the failure; it must
                    // not re-pick the same ticket next. §FS-rhei-run.3
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
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

    Ok(())
}
