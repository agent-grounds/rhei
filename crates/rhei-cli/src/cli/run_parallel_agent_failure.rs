// What a finished parallel agent's failed exit means for its ticket: a timeout
// or a non-zero code fires the edge the failure selection chose, or the ticket
// stalls where it is.
//
// Its own part because a failed exit is decided from the failure selection
// alone, after every branch of the exit next door that reads a success is
// passed, and it never reads the completion condition.

// §AR-source-file-size.3 §FS-rhei-run.3 §FS-rhei-agents.7.3

/// The failed exit, as the exit next door read it.
struct ParallelAgentFailure<'a> {
    task_id_str: &'a str,
    state_name: &'a str,
    timed_out: bool,
    timeout_secs: Option<u64>,
    code: i32,
    /// The edge the failure selection chose, when it chose one.
    to_state: Option<&'a str>,
    /// Which rule it chose. §FS-rhei-transitions.4.4
    selected_rule: Option<usize>,
    spawn_record: &'a Path,
}

fn route_parallel_agent_failure(
    failure: ParallelAgentFailure<'_>,
    input: &Path,
    machines: &ExecutionMachines,
    opts: &RunOptions,
    sink: &Arc<dyn rhei_tui::EventSink>,
    release: &mut PendingSlotRelease,
    progress: &mut AgentPassProgress<'_>,
) -> MietteResult<()> {
    use rhei_tui::MessageLevel;
    let ParallelAgentFailure { task_id_str, state_name, code, .. } = failure;
    let fired = if failure.timed_out {
        emit_run_message(
            sink,
            MessageLevel::Warn,
            format!("  agent timed out for task {task_id_str} in '{state_name}'"),
        );
        failure.to_state.map(|to_state| {
            fire_selected_timeout_transition(
                input,
                machines,
                task_id_str,
                state_name,
                to_state,
                failure.timeout_secs,
                opts,
            )
        })
    } else {
        emit_run_message(
            sink,
            MessageLevel::Error,
            format!("  error: agent exited with code {code} for task {task_id_str}"),
        );
        failure.to_state.map(|to_state| {
            fire_agent_exit_transition(
                input,
                machines,
                task_id_str,
                state_name,
                to_state,
                code,
                failure.selected_rule,
                opts.no_callbacks(),
            )
        })
    };
    match fired {
        Some(fired) => {
            leave_refused_exit_unrouted(&fired, release, failure.spawn_record);
            match fired {
                TimeoutTransitionOutcome::Fired => *progress.advanced_any = true,
                // §FS-rhei-run.3: the exit routed nowhere, so the ticket is
                // still ready and would be re-spawned into every slot that
                // frees up this pass.
                TimeoutTransitionOutcome::NoRule
                | TimeoutTransitionOutcome::Failed
                | TimeoutTransitionOutcome::Refused => {
                    progress.stalled_tasks.insert(task_id_str.to_string());
                }
            }
        }
        None if failure.timed_out => {
            emit_run_message(
                sink,
                MessageLevel::Warn,
                format!(
                    "  warning: agent for task {task_id_str} timed out from '{state_name}' but no \
                     timeout transition is declared; task remains in state"
                ),
            );
            // §FS-rhei-run.3: a ticket that timed out with nowhere to go would
            // time out again, once per free slot, for the whole pass.
            progress.stalled_tasks.insert(task_id_str.to_string());
        }
        None if !opts.continue_on_error() => {
            return Err(miette!(
                help = run_report_help(),
                "agent exited with code {code} for Task {task_id_str}. \
                 Use --continue-on-error to skip failures."
            ));
        }
        // `--continue-on-error` skips the failure; it must not turn it into an
        // unbounded respawn loop within the pass. §FS-rhei-run.3
        None => {
            progress.stalled_tasks.insert(task_id_str.to_string());
        }
    }
    Ok(())
}
