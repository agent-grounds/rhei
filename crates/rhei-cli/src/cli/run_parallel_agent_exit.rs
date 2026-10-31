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
        // Emitted when this function returns, or just before its settle.
        // §FS-rhei-states.2.2
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
    let AgentSpawnOutcome {
        status, timed_out, timeout_secs, provider_limit, provider_unavailable, ..
    } = outcome;
    // The completed ticket's own machine drives its post-exit
    // handling; callbacks resolve inside each helper from the
    // same set. §DA-per-rhei-state-machines
    let machine = machines.for_task_str(&task_id_str);
    // The process returned, so the start that was ambiguous is now confirmed.
    // §FS-rhei-budgets.6.2
    budget_record_start(workspace_root, &task_id_str, true, sink);
    *progress.agents_spawned += 1;
    let target_id = parse_task_id(&task_id_str);
    let reloaded = reload_after_worker_exit(input, &mut release, &spawn_record)?;
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
    // A reverted edit spends the attempt and routes nothing; no edge, so the
    // travel unit goes back. §FS-rhei-run.3.7.4 §FS-rhei-budgets.4.1
    if attempt_was_reverted(&spawn_record) && !timed_out {
        let (task, state) = (task_id_str.as_str(), state_name.as_str());
        charge_reverted_attempt(
            &mut release,
            &reloaded,
            input,
            machine,
            task,
            state,
            &spawn_record,
        )?;
        progress.stalled_tasks.insert(task_id_str.clone());
        drop(release);
        budget_settle_visit(workspace_root, &task_id_str, sink);
        return Ok(());
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
        let at = (task_id_str.as_str(), state_name.as_str());
        park_refusal(input, &reloaded, at, stayed_in_state, observed, &mut release, sink)?;
        // The slot goes first so the settle's account is the visit's last
        // record, as on the sequential path. §FS-rhei-run-json.2.1
        drop(release);

        // A provider limit gives the *attempt* back and never the invocation:
        // the process ran. The travel unit goes back because no edge was
        // applied. §FS-rhei-budgets.4.2 §FS-rhei-agents.3.2.3
        budget_settle_visit(workspace_root, &task_id_str, sink);
        return Ok(());
    }
    let mut missing_required_outputs = Vec::new();
    let mut snapshot_completion_for_emit = None;
    let mut failure_selected_to_state = None;
    // Which rule the selection chose, where a selection ran at all: an exhausted
    // poll's edge is picked by evaluating candidates' conditions, and the
    // `(from, to)` pair cannot recover it. §FS-rhei-transitions.4.4
    let mut failure_selected_rule = None;
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
        (failure_selected_to_state, failure_selected_rule) = if timed_out {
            (find_timeout_transition(machine, &state_name), None)
        } else if !status.success() {
            // Load refuses an `exit_code:` edge from a state with no `program:`,
            // so an agent reaches this only by a poll exhaustion edge, whose
            // classification the caller never reads. §FS-rhei-run.5.1
            let route = find_program_exit_transition(
                machine,
                reloaded.rhei.metadata.as_ref(),
                task_for_snapshot,
                &state_name,
                status.code().unwrap_or(-1),
            )?;
            (
                route.as_ref().map(|route| route.to.clone()),
                route.map(|route| route.selected_rule),
            )
        } else {
            (None, None)
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
    // Cleared only once this exit is known not to be a wait. §FS-rhei-run.3.3
    let resumed = resumed_provider(provider_unavailable.as_ref(), &missing_required_outputs);
    if status.success() && stayed_in_state && resumed {
        clear_persisted_provider_limit(input, &reloaded, &task_id_str, &state_name, &resolved, started_at)?;
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
                let stall = AgentStall {
                    input,
                    reloaded: &reloaded,
                    task_id: &task_id_str,
                    state: &state_name,
                    spawn_record: &spawn_record,
                    missing: &missing_required_outputs,
                    outlook: retry_outlook,
                    verdict: provider_unavailable.as_ref(),
                };
                stall_agent_exit(stall, &mut release, sink)?;
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
                                let agent_runtime_dir = if machines
                                    .uses_member_local_runtime(&task_id_str)
                                {
                                    task_root.join("runtime")
                                } else {
                                    workspace_root.join("runtime")
                                };
                                task_has_pending_agent_invocations(
                                    &task_root,
                                    &agent_runtime_dir,
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
            let refused = leave_refused_exit_unrouted(&auto_advance_result, &mut release, &spawn_record);
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
                        // Held until something advances after now. §FS-rhei-run.3.6
                        if empty_visit.is_some() {
                            progress.empty_visit_holds.hold(
                                &task_id_str,
                                workspace_root,
                                &reloaded.task_roots,
                            );
                        }
                    } else {
                        run_warn!(
                            "  warning: agent exited 0 but task {} did not advance from '{}'",
                            task_id_str, state_name
                        );
                    }
                }
                Err(err) => {
                    // A refused callback already said why. §FS-rhei-seal.8.5
                    if !refused {
                        run_warn!(
                            "  warning: agent exited 0 but task {} could not auto-advance from '{}': {}",
                            task_id_str, state_name, err
                        );
                    }
                    // Did not move: without this the refill
                    // re-spawns it, and the error repeats.
                    // §FS-rhei-run.3
                    progress.stalled_tasks.insert(task_id_str.clone());
                }
            }
        }
    } else {
        let failure = ParallelAgentFailure {
            task_id_str: &task_id_str,
            state_name: &state_name,
            timed_out,
            timeout_secs,
            code: status.code().unwrap_or(-1),
            to_state: failure_selected_to_state.as_deref(),
            selected_rule: failure_selected_rule,
            spawn_record: &spawn_record,
        };
        route_parallel_agent_failure(failure, input, machines, opts, sink, &mut release, progress)?;
    }

    // The slot goes first so the settle's account is the visit's last
    // record, as on the sequential path. §FS-rhei-run-json.2.1
    drop(release);

    // The visit is over and whatever edge it was going to apply has been
    // applied: a travel unit still held is one nothing moved against, and it
    // goes back. §FS-rhei-budgets.4.1
    budget_settle_visit(workspace_root, &task_id_str, sink);
    Ok(())
}
