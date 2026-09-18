// What a finished parallel agent's exit means for its ticket: whether it met
// the completion condition, which snapshots the outcome emits, and which edge —
// advance, auto-advance, timeout, or exit code — the run fires next.
//
// Its own part because the decision is made from the completion alone: it does
// not touch slots, the channel, or the refill next door, and a ticket that
// stalls here ends its own turn rather than the pass.

// §AR-source-file-size.3 §FS-rhei-run.3 §FS-rhei-agents.3.2

#[allow(clippy::too_many_arguments)]
fn handle_parallel_agent_exit(
    exit: ParallelAgentExit,
    input: &Path,
    machines: &ExecutionMachines,
    settings: &RheiSettings,
    opts: &RunOptions,
    workspace_root: &Path,
    sink: &Arc<dyn rhei_tui::EventSink>,
    active_invocation_counts: &HashMap<String, usize>,
    progress: &mut AgentPassProgress<'_>,
) -> MietteResult<()> {
    use rhei_tui::MessageLevel;
    macro_rules! run_message { ($level:expr, $($arg:tt)*) => {{ emit_run_message(sink, $level, format!($($arg)*)); }}; }
    macro_rules! run_info { ($($arg:tt)*) => { run_message!(MessageLevel::Info, $($arg)*); }; }
    macro_rules! run_warn { ($($arg:tt)*) => { run_message!(MessageLevel::Warn, $($arg)*); }; }
    macro_rules! run_error { ($($arg:tt)*) => { run_message!(MessageLevel::Error, $($arg)*); }; }

    let ParallelAgentExit {
        task_id_str,
        state_name,
        started_at,
        // Emitted when this function returns. §FS-rhei-states.2.2
        mut release,
        resolved,
        log,
        snapshot_preload,
        visit_count,
        retry_outlook,
        subtree_before,
        spawn_record,
        accounting_recorded,
        outcome,
    } = exit;
    let AgentSpawnOutcome { status, timed_out, timeout_secs, provider_limit, .. } = outcome;
    // The completed ticket's own machine drives its post-exit
    // handling; callbacks resolve inside each helper from the
    // same set. §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&task_id_str);
    *progress.agents_spawned += 1;
    let target_id = parse_task_id(&task_id_str);
    let reloaded = load_plan(input)?;
    // §FS-rhei-agents.3.2 condition (2): declared `outputs:` and the terminal
    // result resolve against the owning rhei's root, not the run-level one.
    let task_root = reloaded.task_root(&task_id_str, workspace_root);
    if accounting_recorded {
        if let Err(err) =
            regenerate_accounting_indexes(workspace_root, &reloaded.rhei)
        {
            run_warn!(
                "  warning: failed to update accounting rollups: {}",
                err
            );
        }
    }
    let task_after = find_task_by_id(&reloaded.rhei.tasks, &target_id);
    let stayed_in_state = task_after.is_some_and(|task| {
        normalized_state_name(task.state.as_str(), machine)
            == normalized_state_name(&state_name, machine)
    });
    // The worker has released its slot; classify and persist the wait before
    // any generic failure transition, callback, or snapshot path can run.
    // §FS-rhei-agents.5.2.2 §FS-rhei-run.3.3
    if let Some(observed) = provider_limit {
        let effective = if stayed_in_state {
            persist_provider_limit(input, &reloaded, &task_id_str, &state_name, &observed)?
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
        clear_persisted_provider_limit(input, &reloaded, &task_id_str, &state_name, &resolved, started_at)?;
    }
    let mut missing_required_outputs = Vec::new();
    let mut snapshot_completion_for_emit = None;
    let mut failure_selected_to_state = None;
    // Hoisted out of the block below so the pending-invocation
    // check can ask the same question about the same edge.
    // §FS-rhei-agents.3.2
    let mut selected_to: Option<String> = None;
    if let (Some(task_for_snapshot), Some(state_def)) =
        (task_after, machine.states.get(state_name.as_str()))
    {
        // §FS-rhei-agents.3.2: the completion condition is
        // exit 0 + declared outputs + the terminal result
        // when the edge this exit selects is terminal.
        selected_to = selected_forward_transition(
            &reloaded.rhei,
            machine,
            task_for_snapshot,
        );
        let outputs_ok = status.success()
            && state_outputs_exist_for_resolved_invocation(
                &task_root,
                task_for_snapshot,
                &state_name,
                &state_name,
                machine,
                reloaded.rhei.metadata.as_ref(),
                state_def,
                &resolved,
            )
            && missing_terminal_result_output(
                &task_root,
                machine,
                task_for_snapshot,
                selected_to.as_deref(),
                // A fanned-out invocation answers for its
                // own fragment. §FS-rhei-states.3.3
                ResultInvocation {
                    state: &state_name,
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
        if status.success() && !outputs_ok {
            missing_required_outputs =
                collect_missing_required_outputs_for_resolved_invocation(
                    &task_root,
                    machine,
                    reloaded.rhei.metadata.as_ref(),
                    task_for_snapshot,
                    &state_name,
                    selected_to.as_deref(),
                    &resolved,
                );
        }
        let snapshot_completion = if timed_out {
            SnapshotCompletion::Timeout
        } else if outputs_ok {
            SnapshotCompletion::Success
        } else {
            SnapshotCompletion::Failure
        };
        failure_selected_to_state = if timed_out {
            find_timeout_transition(machine, &state_name)
        } else if !status.success() {
            // Load refuses an `exit_code:` edge from a state with no `program:`,
            // so an agent reaches this only by a poll exhaustion edge, whose
            // classification the caller never reads. §FS-rhei-run.5.1
            find_program_exit_transition(
                machine,
                reloaded.rhei.metadata.as_ref(),
                task_for_snapshot,
                &state_name,
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
                task_for_snapshot,
                &state_name,
                failure_selected_to_state.as_deref(),
                &resolved,
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
        snapshot_completion_for_emit = Some(snapshot_completion);
    }
    let state_after = task_after.map(|t| t.state.as_str()).unwrap_or("unknown");
    if normalized_state_name(state_after, machine)
        != normalized_state_name(&state_name, machine)
    {
        if status.success() {
            if let (Some(task_for_snapshot), Some(snapshot_completion)) =
                (task_after, snapshot_completion_for_emit)
            {
                if let Err(err) = emit_snapshots_after_agent_exit(
                    workspace_root,
                    machine,
                    settings,
                    task_for_snapshot,
                    &state_name,
                    Some(state_after),
                    &resolved,
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
            state_name,
            state_after
        );
        *progress.advanced_any = true;
    } else if status.success() {
        // Every exit falls through to the refill below: a
        // `continue` left the freed slot idle all pass.
        // §FS-rhei-run.3
        'exit_zero: {
            if !missing_required_outputs.is_empty() {
                if let (
                    Some(task_for_snapshot),
                    Some(snapshot_completion),
                ) = (task_after, snapshot_completion_for_emit)
                {
                    if let Err(err) = emit_snapshots_after_agent_exit(
                        workspace_root,
                        machine,
                        settings,
                        task_for_snapshot,
                        &state_name,
                        None,
                        &resolved,
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
                    &task_id_str,
                    &state_name,
                    0,
                    &missing_required_outputs,
                    retry_outlook,
                    sink,
                );
                progress.stalled_tasks.insert(task_id_str.clone());
                break 'exit_zero;
            }
            // A sibling invocation is still running: the
            // state is not finished, and advancing now
            // strands it. §FS-rhei-states.3.3
            let sibling_in_flight =
                active_invocation_counts.contains_key(&task_id_str);
            let pending_more = sibling_in_flight
                || reloaded
                    .rhei
                    .tasks
                    .iter()
                    .find(|t| t.id == target_id)
                    .and_then(|task| {
                        machine.states.get(state_name.as_str()).map(
                            |state_def| {
                                task_has_pending_agent_invocations(
                                    &task_root,
                                    task,
                                    &state_name,
                                    task.state.as_str(),
                                    machine,
                                    reloaded.rhei.metadata.as_ref(),
                                    state_def,
                                    settings,
                                    opts,
                                    selected_to.as_deref(),
                                )
                            },
                        )
                    })
                    .transpose()?
                    .unwrap_or(false);
            if pending_more {
                if let (
                    Some(task_for_snapshot),
                    Some(snapshot_completion),
                ) = (task_after, snapshot_completion_for_emit)
                {
                    if let Err(err) = emit_snapshots_after_agent_exit(
                        workspace_root,
                        machine,
                        settings,
                        task_for_snapshot,
                        &state_name,
                        None,
                        &resolved,
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
            // §FS-rhei-supervision.3.6: a visit that released nothing must
            // not spend the self-loop that releases the subtree.
            let empty_visit = withhold_empty_supervising_visit(FinishedVisit {
                workspace_root,
                plan: &reloaded,
                machines: &machines.set,
                task_id: &target_id,
                state: &state_name,
                before: subtree_before.as_ref(),
                spawn_record: Some(&spawn_record),
            });
            if let Some(warning) = &empty_visit {
                run_warn!("{}", warning);
            }
            let mut emit_before_transition =
                |task_for_snapshot: &rhei_core::ast::Task,
                 to_state: &str|
                 -> MietteResult<()> {
                    // Nothing to emit when the exit produced no completion to
                    // emit for: the callback answers, rather than the caller
                    // branching around it. §FS-rhei-snapshots.10.2
                    let Some(snapshot_completion) = snapshot_completion_for_emit else {
                        return Ok(());
                    };
                    emit_snapshots_after_agent_exit(
                        workspace_root,
                        machine,
                        settings,
                        task_for_snapshot,
                        &state_name,
                        Some(to_state),
                        &resolved,
                        &log,
                        visit_count,
                        snapshot_completion,
                        &snapshot_preload,
                    )
                };
            let auto_advance_result = match empty_visit {
                Some(_) => Ok(None),
                None => try_auto_advance_task(
                    input,
                    machines,
                    &task_id_str,
                    &state_name,
                    opts,
                    Some(&mut emit_before_transition),
                ),
            };
            match auto_advance_result {
                Ok(Some(advance)) => {
                    // A scheduled next attempt is not a move: the attempt
                    // that selected the edge is a wait. §FS-rhei-states.2.2
                    if advance.poll_wait { release.waiting(); }
                    run_info!(
                        "  Task {} auto-advanced: '{}' -> '{}'",
                        task_id_str,
                        state_name,
                        advance.to
                    );
                    *progress.advanced_any = true;
                }
                Ok(None) => {
                    if let Some(task) =
                        find_task_by_id(&reloaded.rhei.tasks, &target_id)
                    {
                        if let Some(snapshot_completion) =
                            snapshot_completion_for_emit
                        {
                            if let Err(err) = emit_snapshots_after_agent_exit(
                                workspace_root,
                                machine,
                                settings,
                                task,
                                &state_name,
                                None,
                                &resolved,
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
                        // A withheld release edge already said so above.
                        // §FS-rhei-supervision.3.6
                        if empty_visit.is_none() {
                            emit_exit_zero_warnings(
                                workspace_root,
                                &task_root,
                                machine,
                                reloaded.rhei.metadata.as_ref(),
                                task,
                                &task_id_str,
                                &state_name,
                                selected_forward_transition(&reloaded.rhei, machine, task)
                                    .as_deref(),
                                retry_outlook,
                                sink,
                            );
                        }
                        progress.stalled_tasks.insert(task_id_str.clone());
                    } else {
                        run_warn!(
                            "  warning: agent exited 0 but task {} did not advance from '{}'",
                            task_id_str, state_name
                        );
                    }
                }
                Err(err) => {
                    run_warn!(
                        "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                        task_id_str, state_name, err
                    );
                    // Did not move: without this the refill
                    // re-spawns it, and the error repeats.
                    // §FS-rhei-run.3
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
            }
        }
    } else if timed_out {
        run_warn!(
            "  agent timed out for task {} in '{}'",
            task_id_str,
            state_name
        );
        if let Some(to_state) = failure_selected_to_state.as_deref() {
            match fire_selected_timeout_transition(
                input,
                machines,
                &task_id_str,
                &state_name,
                to_state,
                timeout_secs,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => *progress.advanced_any = true,
                // Did not move: the pool must not re-spawn
                // it from the live ready set this pass.
                // §FS-rhei-run.3
                TimeoutTransitionOutcome::NoRule
                | TimeoutTransitionOutcome::Failed => {
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
            }
        } else {
            {
                run_warn!(
                    "  warning: agent for task {} timed out from '{}' but no timeout transition is declared; task remains in state",
                    task_id_str, state_name
                );
                // §FS-rhei-run.3: a ticket that timed out
                // with nowhere to go would time out again,
                // once per free slot, for the whole pass.
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
                &task_id_str,
                &state_name,
                to_state,
                code,
                opts.no_callbacks(),
            ) {
                TimeoutTransitionOutcome::Fired => *progress.advanced_any = true,
                // §FS-rhei-run.3: the exit routed nowhere,
                // so the ticket is still ready and would be
                // re-spawned into every slot that frees up.
                TimeoutTransitionOutcome::NoRule
                | TimeoutTransitionOutcome::Failed => {
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
            }
        } else if !opts.continue_on_error() {
            return Err(miette!(
                help = run_report_help(),
                "agent exited with code {code} for Task {task_id_str}. \
                 Use --continue-on-error to skip failures."
            ));
        } else {
            // `--continue-on-error` skips the failure; it
            // must not turn it into an unbounded respawn
            // loop within the pass. §FS-rhei-run.3
            progress.stalled_tasks.insert(task_id_str.clone());
        }
    }

    Ok(())
}
