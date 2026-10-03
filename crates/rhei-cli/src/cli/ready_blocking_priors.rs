// Which `**Prior:**` holds a ticket back, judged exactly as readiness judges it.

/// Every unsatisfied `**Prior:**` of `task` as `Task <id> (<state>)`. Judged
/// exactly as readiness judges it, so mutation commands and the scheduler
/// agree on what "blocked" means. §FS-rhei-panta.6.1
fn blocking_priors(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
) -> Vec<String> {
    task.prior
        .iter()
        .filter_map(|dep_id| match state_map.get(dep_id) {
            Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
                Some(format!("Task {} ({})", dep_id, state))
            }
            None => Some(format!("Task {} (missing)", dep_id)),
            _ => None,
        })
        .collect()
}

fn first_blocking_prior(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<String> {
    first_blocking_prior_reading(task, state_map, machines, scope).map(|blocker| blocker.prior)
}

/// The prior a blocked ticket is named as waiting on, and whether it can ever
/// finish.
struct BlockingPrior {
    /// Already formatted as `Task <id> (<state>)`.
    prior: String,
    /// The prior sits in a cancellation state under its own rhei's machine.
    cancelled: bool,
}

/// The first unsatisfied `**Prior:**` of `task`, except that a cancelled one
/// is named ahead of a live one whatever order `**Prior:**` lists them in: a
/// live prior may yet finish, a cancelled one never will.
/// §FS-rhei-run-report.3.1 §FS-rhei-states.1.4
fn first_blocking_prior_reading(
    task: &rhei_core::ast::Task,
    state_map: &std::collections::HashMap<&TaskId, String>,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<BlockingPrior> {
    let blockers = task.prior.iter().filter_map(|dep_id| match state_map.get(dep_id) {
        Some(state) if !dependency_is_satisfied(state, machines.for_task(dep_id)) => {
            // §FS-rhei-panta.6.1: `--rhei` narrows candidates, never prior
            // resolution, so name the prior that sits outside the scope
            // rather than leaving the operator to guess why nothing ran.
            let outside = if task_in_rhei_scope(scope, &dep_id.to_string()) {
                ""
            } else {
                ", outside the --rhei scope"
            };
            // Judged under the prior's own machine, so a custom
            // `role: cancellation` state counts. §FS-rhei-panta.6.1
            let machine = machines.for_task(dep_id);
            Some(BlockingPrior {
                prior: format!("Task {} ({}{})", dep_id, state, outside),
                cancelled: machine.is_cancellation(&normalized_state_name(state, machine)),
            })
        }
        None => Some(BlockingPrior { prior: format!("Task {} (missing)", dep_id), cancelled: false }),
        _ => None,
    });
    let mut first = None;
    for blocker in blockers {
        if blocker.cancelled {
            return Some(blocker);
        }
        first.get_or_insert(blocker);
    }
    first
}
