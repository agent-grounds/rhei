// Why `rhei next` could not hand anyone a ticket: the category the plan falls
// into, in the order a worker cares about, and the explicit transitions a
// mid-workflow ticket offers instead.
//
// Its own part because the ready set answers "what may be scheduled" while this
// answers "what should I tell the person who asked and found nothing", which is
// a different walk over the same plan.

// §AR-source-file-size.3 §FS-rhei-next.3.4

/// The ` --state-machine=<path>` a suggested command needs to resolve the same
/// machine this invocation did, or nothing when the default was in force.
fn state_machine_flag(state_machine_path: Option<&Path>) -> String {
    state_machine_path
        .map(|path| format!(" --state-machine={}", shell_quote(&path.display().to_string())))
        .unwrap_or_default()
}

/// The next step for a worker whose whole scope is held: claim the supervisor's
/// visit, or — when someone already has — say who and how to take it back.
///
/// A held ticket is not a stall and not a blocker; it is someone else's turn,
/// and the row that reports it has to name whose. Returns `None` for a
/// supervisor that is not itself work right now, so the caller can look at the
/// next one up.
// §FS-rhei-supervision.3.4
fn supervisor_next_step(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    supervisor: &TaskId,
    plan_arg: &str,
    state_machine_path: Option<&Path>,
) -> Option<String> {
    let mut all = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut all);
    let task = all.iter().copied().find(|task| &task.id == supervisor)?;
    if let Some(assignee) = task.assignee.as_deref() {
        return Some(format!(
            " Task {supervisor} is the ticket to work and {assignee} holds it; hand it back \
             with: rhei release {plan_arg} --task {supervisor}"
        ));
    }
    let index = task_index(&all);
    let claimable = subtree_admits_to_ready_set(
        task,
        &index,
        machines,
        rhei.metadata.as_ref(),
        &HashSet::new(),
    );
    if !claimable {
        return None;
    }
    Some(format!(
        " Work the supervisor instead: rhei{} next {plan_arg} --task {supervisor}",
        state_machine_flag(state_machine_path)
    ))
}

fn transition_command_lines(
    task: &rhei_core::ast::Task,
    state_name: &str,
    machine: &rhei_validator::StateMachine,
    metadata: Option<&Metadata>,
    plan_arg: &str,
    state_machine_path: Option<&Path>,
) -> Vec<String> {
    let state_machine_arg = state_machine_flag(state_machine_path);
    let from_arg = shell_quote(state_name);
    machine
        .transitions()
        .iter()
        .filter(|rule| machine.transition_matches_source(rule, state_name))
        .filter(|rule| {
            task_profile_allows_state(
                machine,
                task.kind.as_str(),
                task.profile_level(),
                &rule.to.0,
            )
        })
        .filter(|rule| {
            transition_rule_is_applicable(
                rule,
                machine,
                metadata,
                &task.id,
                Some(task),
                state_name,
                task.state.as_str(),
            )
            .unwrap_or(false)
        })
        .map(|rule| {
            let to_arg = shell_quote(&rule.to.0);
            format!(
                "  rhei{} transition {} --task {} --from={} --to={}",
                state_machine_arg, plan_arg, task.id, from_arg, to_arg
            )
        })
        .collect()
}

/// Build an actionable error message for `rhei next` when no task can be
/// auto-claimed. Priors resolve project-wide even under `--rhei`, so only the
/// reported categories narrow — never the state map. §FS-rhei-panta.6.1
fn diagnose_no_claimable(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    plan_path: &Path,
    state_machine_path: Option<&Path>,
    scope: &RheiScope,
) -> String {
    let mut project = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut project);

    let state_map = plan_state_map(&project, machines);

    let all: Vec<&rhei_core::ast::Task> = project
        .iter()
        .copied()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .collect();

    let scope_suffix = match scope {
        Some(_) => format!(" in the --rhei scope ({})", scope_label(scope)),
        None => String::new(),
    };

    if all.is_empty() {
        return match scope {
            Some(_) => {
                format!("no tickets are ready to claim{scope_suffix} (no tickets in scope)")
            }
            // Match the vocabulary and the next step `rhei list` gives for the
            // same state; a bare "plan has no tasks" leaves a new user stuck.
            None if rhei_core::workspace::is_panta_project(plan_path) => {
                format!(
                    "no tickets are ready to claim — the project has none yet: {}",
                    add_a_rhei_hint()
                )
            }
            None => "no tickets are ready to claim — this rhei has none yet".to_string(),
        };
    }

    let non_terminal: Vec<&rhei_core::ast::Task> = all
        .iter()
        .copied()
        .filter(|t| !is_terminal_state(t.state.as_str(), machines.for_task(&t.id)))
        .collect();

    if non_terminal.is_empty() {
        return match scope {
            Some(_) => format!(
                "Scope complete. All {} task(s){scope_suffix} are in terminal states.",
                all.len()
            ),
            None => format!("Plan complete. All {} task(s) are in terminal states.", all.len()),
        };
    }

    // Every category below speaks about a task the caller can act on, so each
    // is computed over the *workable* set: leaves, plus non-leaf tasks whose
    // subtree is already terminal. A parent with open descendants is not work
    // anyone can be handed; it gets its own category last, so it can still
    // explain a stuck plan instead of falling through to a bare "nothing is
    // ready". The retired "Leaf work complete. <N> rollup task(s) …" message
    // existed only because a parent could never be claimed at all — under the
    // eligibility rule it simply becomes the next claimable ticket the moment
    // its own children finish.

    // §FS-rhei-next.5
    let non_terminal_workable: Vec<&rhei_core::ast::Task> = non_terminal
        .iter()
        .copied()
        .filter(|task| descendants_are_terminal(task, machines))
        .collect();

    let priors_satisfied = |task: &rhei_core::ast::Task| -> bool {
        task.prior.iter().all(|dep_id| {
            state_map
                .get(dep_id)
                .map(|s| dependency_is_satisfied(s, machines.for_task(dep_id)))
                .unwrap_or(false)
        })
    };

    let gating_ready: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).map(|def| def.gating).unwrap_or(false)
                && priors_satisfied(task)
        })
        .collect();

    if !gating_ready.is_empty() {
        let items: Vec<String> = gating_ready
            .iter()
            .take(3)
            .map(|task| {
                let state =
                    normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
                format!("Task {} ({})", task.id, state)
            })
            .collect();
        let suffix = if gating_ready.len() > 3 {
            format!(" (+{} more)", gating_ready.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "Blocked: {} task(s) waiting on human action: {}{}.",
            gating_ready.len(),
            items.join(", "),
            suffix
        );
    }

    let assigned_ready: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|t| {
            let machine = machines.for_task(&t.id);
            let s = normalized_state_name(t.state.as_str(), machine);
            let gating = machine.states.get(&s).map(|def| def.gating).unwrap_or(false);
            !gating && t.assignee.is_some() && priors_satisfied(t)
        })
        .collect();

    if !assigned_ready.is_empty() {
        let items: Vec<String> = assigned_ready
            .iter()
            .take(3)
            .map(|task| {
                let state =
                    normalized_state_name(task.state.as_str(), machines.for_task(&task.id));
                let assignee = task.assignee.as_deref().unwrap_or("unknown");
                format!("Task {} ({}, assignee {})", task.id, state, assignee)
            })
            .collect();
        let suffix = if assigned_ready.len() > 3 {
            format!(" (+{} more)", assigned_ready.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "No tasks available to claim{}. {} task(s) are currently in progress: {}{}.",
            scope_suffix,
            assigned_ready.len(),
            items.join(", "),
            suffix
        );
    }

    // §FS-rhei-supervision.3.4: a held descendant is not blocked and not
    // mid-workflow — its supervisor simply has not released it — so it gets a
    // row of its own beside the prerequisite row.
    let held: Vec<(&rhei_core::ast::Task, TaskId, String)> = non_terminal
        .iter()
        .copied()
        .filter_map(|task| {
            held_by_supervisor(task, rhei, machines)
                .map(|hold| (task, hold.supervisor, hold.state))
        })
        .collect();
    if !held.is_empty() {
        let items: Vec<String> = held
            .iter()
            .take(3)
            .map(|(task, supervisor, state)| {
                format!("Task {} held by supervisor Task {} ({})", task.id, supervisor, state)
            })
            .collect();
        let suffix =
            if held.len() > 3 { format!(" (+{} more)", held.len() - 3) } else { String::new() };
        // The supervisor is in no category above — its own subtree is open, so
        // the workable set excludes it — and naming it here is what keeps the
        // answer from being a dead end. §FS-rhei-supervision.3.4
        let mut supervisors: Vec<&TaskId> = Vec::new();
        for (_, supervisor, _) in &held {
            if !supervisors.contains(&supervisor) {
                supervisors.push(supervisor);
            }
        }
        let plan_arg = shell_quote(&plan_path.display().to_string());
        let next_step = supervisors
            .into_iter()
            .find_map(|id| {
                supervisor_next_step(rhei, machines, id, &plan_arg, state_machine_path)
            })
            .unwrap_or_default();
        return format!(
            "no tickets are ready to claim{}: {} ticket(s) held by a supervisor: {}{}.{}",
            scope_suffix,
            held.len(),
            items.join(", "),
            suffix,
            next_step
        );
    }

    let ready_non_initial: Vec<&rhei_core::ast::Task> = non_terminal_workable
        .iter()
        .copied()
        .filter(|t| {
            let machine = machines.for_task(&t.id);
            let s = normalized_state_name(t.state.as_str(), machine);
            let gating = machine.states.get(&s).map(|def| def.gating).unwrap_or(false);
            !gating && !task_is_in_initial_state(t, &s, machine) && priors_satisfied(t)
        })
        .collect();

    if let Some(task) = ready_non_initial.first() {
        let machine = machines.for_task(&task.id);
        let state_name = normalized_state_name(task.state.as_str(), machine);
        let plan_arg = shell_quote(&plan_path.display().to_string());
        let normalized_metadata = ensure_current_state_visit_count(
            rhei.metadata.as_ref(),
            &task.id,
            &state_name,
            task.state.as_str(),
            machine,
        );
        let metadata_for_checks = normalized_metadata.as_ref().or(rhei.metadata.as_ref());
        let commands = transition_command_lines(
            task,
            &state_name,
            machine,
            metadata_for_checks,
            &plan_arg,
            state_machine_path,
        );
        let guidance = if commands.is_empty() {
            "No outgoing transitions are currently applicable for this state.".to_string()
        } else {
            format!("Available transitions:\n{}", commands.join("\n"))
        };
        return format!(
            "No tasks can be auto-claimed: Task {} is mid-workflow in state '{}'. \
             Pick one of its outgoing transitions explicitly.\n{}",
            task.id, state_name, guidance
        );
    }

    let blocked: Vec<&rhei_core::ast::Task> =
        non_terminal_workable.iter().copied().filter(|t| !priors_satisfied(t)).collect();
    if !blocked.is_empty() {
        let ids: Vec<String> = blocked
            .iter()
            .take(3)
            .map(|task| {
                if let Some(prior) = first_blocking_prior(task, &state_map, machines, scope) {
                    format!("Task {} waiting on {}", task.id, prior)
                } else {
                    format!("Task {}", task.id)
                }
            })
            .collect();
        let suffix = if blocked.len() > 3 {
            format!(" (+{} more)", blocked.len() - 3)
        } else {
            String::new()
        };
        return format!(
            "no tickets are ready to claim{}: {} ticket(s) blocked by incomplete prerequisites: {}{}.",
            scope_suffix,
            blocked.len(),
            ids.join(", "),
            suffix
        );
    }

    // No branch for "only parents are left": a parent with an open descendant
    // always has a non-terminal leaf under it, and that leaf is workable, so
    // the categories above speak for the subtree. A worker whose dependent is
    // blocked on an unclaimed parent reads it in the prerequisite branch,
    // which names the parent and its state like any other prior.

    // Fallback: we found non-terminal tasks with priors satisfied but no
    // other category matched. Keep the legacy phrasing for this edge case.
    format!("no tickets are ready to claim{scope_suffix}")
}
