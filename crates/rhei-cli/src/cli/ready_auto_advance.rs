// Which edge a ready ticket takes next, and the auto-advance that applies it:
// forward-transition selection, the manual-only guard that keeps `rhei run`
// off a default plan, and the poll self-loop that schedules an attempt instead.
//
// Its own part because selecting and firing an edge is the step *after*
// readiness, and it loads the plan again to see what the subprocess left.

// §AR-source-file-size.3 §FS-rhei-run.3

fn state_declares_autonomous_execution(def: &rhei_validator::StateDef) -> bool {
    def.program.is_some()
        || def.agent.is_some()
        || def.model.is_some()
        || def.target.is_some()
        || !def.all_models.is_empty()
        || !def.all_targets.is_empty()
}

fn initial_state_has_non_terminal_forward_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<bool> {
    let Some(to_state) = find_next_transition(task, rhei, machine)? else {
        return Ok(false);
    };
    Ok(!machine.states.get(&to_state).map(|def| def.terminal).unwrap_or(false))
}

fn manual_initial_terminal_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    // §FS-rhei-run.3: default manual-only tasks must not be callback-completed by `rhei run`.
    if !is_builtin_simple_manual_machine(machine) {
        return Ok(None);
    }
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if !task_is_in_initial_state(task, &current_state, machine) {
        return Ok(None);
    }
    let Some(state_def) = machine.states.get(&current_state) else {
        return Ok(None);
    };
    if state_declares_autonomous_execution(state_def) {
        return Ok(None);
    }
    let Some(to_state) = find_next_transition(task, rhei, machine)? else {
        return Ok(None);
    };
    if machine.states.get(&to_state).map(|def| def.terminal).unwrap_or(false) {
        Ok(Some(to_state))
    } else {
        Ok(None)
    }
}

fn is_builtin_simple_manual_machine(machine: &rhei_validator::StateMachine) -> bool {
    machine.name == "rhei"
        && machine.states.len() == 2
        && machine.states.contains_key("pending")
        && machine.states.get("completed").map(|def| def.terminal).unwrap_or(false)
        && machine
            .transitions()
            .iter()
            .filter(|rule| rule.from.0 == "pending" && rule.to.0 == "completed")
            .count()
            == 1
}

/// Find the next forward transition from a given state.
///
/// Prefers exact `from` matches over wildcard (`*`) rules, and skips
/// transitions to terminal states via wildcards (those are escape hatches
/// like cancellation, not forward progress).
fn find_next_transition(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    find_next_transition_from(task, rhei, machine, task.state.as_str())
}

/// The same selection, with the state the edge leaves named apart from `task`.
///
/// `rhei` and `task` are only what a rule's condition is evaluated *against* —
/// the subtree `openDescendants` counts, the metadata a condition reads — while
/// `from_state_raw` is the state the edge leaves. They agree wherever `task`
/// still carries that state, and differ only for a caller judging an invocation
/// against the plan as re-read after it ran: the re-read graph supplies the
/// operands, and the state the invocation ran in supplies the edge, even where
/// the invocation moved its own ticket onward in between.
// §FS-rhei-agents.3.2
fn find_next_transition_from(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    from_state_raw: &str,
) -> MietteResult<Option<String>> {
    let current_state = normalized_state_name(from_state_raw, machine);

    // First, look for an exact from-state match.
    for rule in machine.transitions() {
        if rule.from.0 == current_state
            && task_profile_allows_state(
                machine,
                task.kind.as_str(),
                task.profile_level(),
                &rule.to.0,
            )
            && transition_rule_is_applicable(
                rule,
                machine,
                rhei.metadata.as_ref(),
                &task.id,
                Some(task),
                &current_state,
                from_state_raw,
            )?
        {
            return Ok(Some(rule.to.0.clone()));
        }
    }

    // Fall back to wildcard, but only to non-terminal states (forward progress).
    for rule in machine.transitions() {
        if rule.from.0 == "*" && machine.transition_matches_source(rule, current_state.as_str()) {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if !is_terminal
                && task_profile_allows_state(
                    machine,
                    task.kind.as_str(),
                    task.profile_level(),
                    &rule.to.0,
                )
                && transition_rule_is_applicable(
                    rule,
                    machine,
                    rhei.metadata.as_ref(),
                    &task.id,
                    Some(task),
                    &current_state,
                    from_state_raw,
                )?
            {
                return Ok(Some(rule.to.0.clone()));
            }
        }
    }

    Ok(None)
}

/// [`selected_forward_transition`] with the same state named apart from `task`.
// §FS-rhei-agents.3.2 §FS-rhei-run.3
fn selected_forward_transition_from(
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    from_state_raw: &str,
) -> Option<String> {
    find_next_transition_from(task, rhei, machine, from_state_raw).ok().flatten()
}

type BeforeTransitionCallback<'a> =
    &'a mut dyn FnMut(&rhei_core::ast::Task, &str) -> MietteResult<()>;

/// Where an auto-advance left a ticket, and whether the engine handled the
/// selected edge itself rather than applying it.
///
/// The two are one answer because only the selection knows both: a poll state's
/// self-loop schedules the next attempt instead of moving the ticket, which is
/// what makes the attempt that selected it a wait. A caller reads the verdict
/// here rather than deciding a second time from the state names.
// §FS-rhei-states.2.2
struct AutoAdvance {
    to: String,
    /// The selected edge was this state's poll self-loop, handled internally.
    // §FS-rhei-states.2.2
    poll_wait: bool,
}

fn try_auto_advance_task(
    input: &Path,
    machines: &ExecutionMachines,
    task_id_str: &str,
    current_state: &str,
    no_callbacks: bool,
    mut before_transition: Option<BeforeTransitionCallback<'_>>,
) -> MietteResult<Option<AutoAdvance>> {
    // The advancing ticket's own machine and callback base govern it.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);
    // The spec splits agent exit into:
    //   (5) select the outgoing transition without applying it,
    //   (6) emit snapshots after selection / before application,
    //   (7) apply the selected transition.
    // Step 6 is delegated to the snapshot module owned by impl-rhei-snapshots;
    // see `emit_snapshots_after_transition_selection` for the call site.

    // §FS-rhei-run.3: Select, emit, then apply transitions.
    let loaded = load_plan(input)?;
    let target_id = parse_task_id(task_id_str);
    let Some(task) = find_task_by_id(&loaded.rhei.tasks, &target_id) else {
        return Ok(None);
    };

    // Step 5: select the outgoing transition.
    let Some(to_state) = find_next_transition(task, &loaded.rhei, machine)? else {
        if machine.states.get(current_state).and_then(|def| def.poll.as_ref()).is_some()
            && task_visit_count(loaded.rhei.metadata.as_ref(), &task.id, current_state)
                >= machine
                    .states
                    .get(current_state)
                    .and_then(|def| def.poll.as_ref())
                    .map(|poll| u64::from(poll.max_attempts))
                    .unwrap_or(u64::MAX)
        {
            return Err(miette!(
                help = "the poll state ran out of attempts without a transition becoming applicable. Raise its `poll.max_attempts`, or fix the condition the poll waits on: rhei states",
                "polling exhausted with no matching non-self-loop transition for Task {} in state '{}'",
                task_id_str,
                current_state
            ));
        }
        return Ok(None);
    };

    if record_poll_self_loop_if_needed(
        &loaded,
        input,
        machine,
        task,
        current_state,
        &to_state,
    )? {
        return Ok(Some(AutoAdvance { to: to_state, poll_wait: true }));
    }

    // Step 6: emit auto- and named-snapshots for this state exit, before the
    // transition is applied. This is a no-op until impl-rhei-snapshots wires
    // the snapshot module in; the call site here pins the spec-mandated
    // ordering ("after transition selection and before the transition is
    // applied") so future wiring does not have to relitigate it.
    if let Some(before_transition) = before_transition.as_mut() {
        before_transition(task, &to_state)?;
    }
    emit_snapshots_after_transition_selection(machine, task, current_state, &to_state);

    // Step 7: apply the selected transition, routed to the owning rhei.
    let route = loaded.task_route(task_id_str, input);

    // A fanned-out state's fragments are folded here, before the shared path
    // looks for the result, and only on an edge that finishes the ticket.
    // §FS-rhei-states.3.3
    if machine.states.get(&to_state).map(|def| def.terminal).unwrap_or(false) {
        if let Some(state_def) = machine.states.get(current_state) {
            let workspace_root = execution_workspace_root(&callback_paths.plan_path);
            let settings = load_merged_settings(&workspace_root)?;
            let invocations = resolve_agent_invocations_for_task(
                machine,
                current_state,
                &settings,
                &default_run_options(),
                Some(task),
            )
            .unwrap_or_default();
            merge_fanout_result_fragments(
                &route.execution_root,
                task_id_str,
                current_state,
                render_visit_count(
                    loaded.rhei.metadata.as_ref(),
                    &task.id,
                    current_state,
                    task.state.as_str(),
                    machine,
                ),
                state_def,
                &invocations,
            )?;
        }
    }

    // No message: the subprocess that worked this state knows the outcome and
    // writes `runtime/results/<task-id>.md` itself. A terminal edge with
    // nothing written is caught by the completion condition. §FS-rhei-run.3
    let effective_to = execute_transition(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        current_state,
        &to_state,
        None,
        no_callbacks,
    )?;

    Ok(Some(AutoAdvance { to: effective_to, poll_wait: false }))
}
