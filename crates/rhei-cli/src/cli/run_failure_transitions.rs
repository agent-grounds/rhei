
/// Result of [`fire_timeout_transition`]. The caller uses this to decide
/// whether to count the task as advanced and whether to emit the
/// "no timeout transition is declared" warning required by timeout behavior.
// §FS-rhei-agents.7.3: Timeout transition outcome handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeoutTransitionOutcome {
    /// A matching timeout transition fired successfully.
    Fired,
    /// No timeout transition is declared from the current state.
    NoRule,
    /// A matching rule existed but execution failed; details have already
    /// been logged.
    Failed,
}

fn tooling_trigger_matches(value: &serde_yaml::Value, unavailable: &[String]) -> bool {
    match value {
        serde_yaml::Value::Bool(true) => true,
        serde_yaml::Value::Sequence(items) => items.iter().any(|item| {
            item.as_str().map(|id| unavailable.iter().any(|u| u == id)).unwrap_or(false)
        }),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_tooling_unavailable_transition(
    input: &Path,
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    kind: ToolingKind,
    unavailable: &[String],
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let matching_rule = machine.transitions.iter().find(|rule| {
        let trigger = match kind {
            ToolingKind::Mcp => rule.mcp_unavailable.as_ref(),
            ToolingKind::Skill => rule.skill_unavailable.as_ref(),
        };
        machine.transition_matches_source(rule, from_state)
            && trigger.map(|value| tooling_trigger_matches(value, unavailable)).unwrap_or(false)
    });
    let Some(rule) = matching_rule else {
        return TimeoutTransitionOutcome::NoRule;
    };

    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let route = loaded.task_route(task_id_str, input);
    match execute_system_tooling_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        &rule.to.0,
        kind,
        unavailable,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Tooling-unavailable transition: Task {} '{}' -> '{}' ({} unavailable: {})",
                task_id_str,
                from_state,
                effective_to,
                kind.as_str(),
                unavailable.join(", ")
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
                "  warning: failed to fire tooling-unavailable transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

fn find_timeout_transition(
    machine: &rhei_validator::StateMachine,
    from_state: &str,
) -> Option<String> {
    machine
        .transitions
        .iter()
        .find(|rule| machine.transition_matches_source(rule, from_state) && rule.timeout.is_some())
        .map(|rule| rule.to.0.clone())
}

/// Try to fire a timeout transition for a task after an agent was killed by
/// the watchdog. Returns whether a rule existed and whether it fired.
///
/// Sets `triggeredBy: 'system'` and `transitionData.timeout = <duration>`
/// on the resulting transition context (the duration is the agent's
/// resolved timeout, when known), matching timeout callback behavior.
// §FS-rhei-agents.7.5: Timeout callback context payload.
fn fire_timeout_transition(
    input: &Path,
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    let machine = machines.for_task_str(task_id_str);
    let Some(to_state) = find_timeout_transition(machine, from_state) else {
        return TimeoutTransitionOutcome::NoRule;
    };
    fire_selected_timeout_transition(
        input,
        machines,
        task_id_str,
        from_state,
        &to_state,
        timeout_secs,
        no_callbacks,
    )
}

#[allow(clippy::too_many_arguments)]
fn fire_selected_timeout_transition(
    input: &Path,
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    timeout_secs: Option<u64>,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let route = loaded.task_route(task_id_str, input);
    let timeout_label = timeout_secs
        .map(format_duration_human)
        .or_else(|| {
            machine
                .transitions
                .iter()
                .find(|rule| {
                    machine.transition_matches_source(rule, from_state)
                        && rule.timeout.is_some()
                        && rule.to.0 == to_state
                })
                .and_then(|rule| rule.timeout.clone())
        })
        .unwrap_or_default();
    match execute_system_timeout_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        to_state,
        &timeout_label,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Timeout transition: Task {} '{}' -> '{}' (timeout {})",
                task_id_str, from_state, effective_to, timeout_label
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
                "  warning: failed to fire timeout transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fire_agent_exit_transition(
    input: &Path,
    machines: &ExecutionMachines,
    task_id_str: &str,
    from_state: &str,
    to_state: &str,
    exit_code: i32,
    no_callbacks: bool,
) -> TimeoutTransitionOutcome {
    // The failing ticket's own machine and callback base fire the transition.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    let loaded = match load_plan(input) {
        Ok(l) => l,
        Err(_) => return TimeoutTransitionOutcome::Failed,
    };
    let route = loaded.task_route(task_id_str, input);
    match execute_system_program_exit_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from_state,
        to_state,
        exit_code,
        // Every edge that reaches here is one the engine chose for a worker it
        // ended — a timeout, an agent's failure, a spent poll budget — so none
        // of them is a declared route. §FS-rhei-programs.3.2
        ExitCodeMatch::None,
        no_callbacks,
    ) {
        Ok(effective_to) => {
            diag_info!(
                "  Error transition: Task {} '{}' -> '{}' (exit {})",
                task_id_str, from_state, effective_to, exit_code
            );
            TimeoutTransitionOutcome::Fired
        }
        Err(err) => {
            diag_warn!(
                "  warning: failed to fire error transition for Task {}: {}",
                task_id_str, err
            );
            TimeoutTransitionOutcome::Failed
        }
    }
}

fn format_task_label(task: &rhei_core::ast::Task) -> String {
    format!("Task {}: {}", task.id, task.title)
}

fn format_ready_tasks(tasks: &[&rhei_core::ast::Task]) -> String {
    tasks.iter().map(|task| format_task_label(task)).collect::<Vec<_>>().join(", ")
}

fn format_dry_run_transition(
    task_id: &str,
    from: &str,
    to: &str,
    machine: &rhei_validator::StateMachine,
) -> String {
    // A supervisor's self-loop is the release edge, and rendered bare it reads
    // as a no-op — the one line in a dry run that decides whether the subtree
    // beneath it moves. §FS-rhei-supervision.3.1
    let release = from == to && execute_on_of(machine, &normalized_state_name(from, machine)).is_some();
    let suffix = if release { " (release)" } else { "" };
    format!("would transition: Task {task_id}  {from} -> {to}{suffix}")
}

/// A dry run reports the manual-only condition instead of aborting on the
/// first task that hits it, so one invocation lists every blocked task
/// alongside the transitions that would run. §FS-rhei-run.4
fn format_dry_run_manual_only(task_id: &str, from: &str, to: &str) -> String {
    format!(
        "manual-only: Task {task_id}  {from} -> {to} \
         (claim with `rhei next`, finish with `rhei complete`)"
    )
}

/// The error a dry run ends with once it has reported every manual-only task.
///
/// The individual tasks were already streamed as `manual-only:` lines above,
/// so this only carries the count and the fix.
fn manual_only_dry_run_error(reported: &[String]) -> miette::Report {
    miette!(
help = "claim one with: rhei next <plan>",

        "{} task(s) reported above are in a manual-only initial state and cannot be advanced \
         by `rhei run`. Claim each with `rhei next`, do the work, then finish with \
         `rhei complete`.",
        reported.len()
    )
}

/// The error a dry run ends with when it found nothing to schedule and the
/// remaining tickets need a human.
///
/// The per-ticket causes were streamed above. The exit status matters as much
/// as the lines: a dry run that reported success while `rhei run` on the same
/// state halts is not a prediction, and the difference showed up as a wedged
/// queue nobody was warned about.
// §FS-rhei-run.4
fn dry_run_halt_error() -> miette::Report {
    miette!(
help = nothing_claimable_help(),

        "`rhei run` would halt here: nothing is schedulable and the tickets reported above \
         need a human. This is the same outcome the real run reaches."
    )
}

fn format_state_counts(rhei: &rhei_core::ast::Rhei) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    for task in tasks {
        *counts.entry(task.state.as_str()).or_default() += 1;
    }

    counts
        .into_iter()
        .map(|(state, count)| format!("{state}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}
