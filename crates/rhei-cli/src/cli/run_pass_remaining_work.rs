// What the pass loop asks before it decides the run is finished: how many
// tickets there are and how many are terminal, which ones are newly discovered,
// when the next poll attempt comes due, and whether what is left is only
// waiting on a human or a gate.
//
// Its own part because none of it fires a transition — it is the reading of the
// plan that decides whether another pass is worth making at all.

// §AR-source-file-size.3 §FS-rhei-run.3

fn total_task_count(rhei: &rhei_core::ast::Rhei) -> usize {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.len()
}

fn terminal_task_count(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> usize {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        .filter(|task| is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .count()
}

fn newly_discovered_tasks(
    task_ids_before: &BTreeSet<String>,
    tasks_after: &[rhei_core::ast::Task],
) -> Vec<String> {
    tasks_after
        .iter()
        .filter(|task| !task_ids_before.contains(&task.id.to_string()))
        .map(format_task_label)
        .collect()
}

/// Check whether a dependency state satisfies a prerequisite edge.
///
/// Terminal cancellation does not satisfy dependencies: a cancelled task should
/// not unblock downstream work.
fn dependency_is_satisfied(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    // §FS-rhei-states.1.4: the reserved cancel name, in either spelling.
    !rhei_validator::is_cancelled_state_name(&normalized_state_name(state, machine))
        && is_terminal_state(state, machine)
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn yaml_value_to_epoch_secs(value: &YamlValue) -> Option<u64> {
    match value {
        YamlValue::Number(number) => number.as_u64(),
        YamlValue::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn poll_next_attempt_at(
    metadata: Option<&Metadata>,
    task_id: &TaskId,
    state_name: &str,
) -> Option<u64> {
    task_metadata_map(metadata, task_id)
        .and_then(|task_map| task_map.get(yaml_key("pollNextAttemptAt")))
        .and_then(YamlValue::as_mapping)
        .and_then(|poll_map| poll_map.get(yaml_key(state_name)))
        .and_then(yaml_value_to_epoch_secs)
}

/// Whether any in-scope task is still non-terminal. Drives the end-of-run halt
/// check: a narrowed run only answers for its own scope. §FS-rhei-panta.6.1
fn scoped_unfinished_task_exists(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.into_iter().any(|task| {
        task_in_rhei_scope(scope, &task.id.to_string())
            && !is_terminal_state(task.state.as_str(), machines.for_task(&task.id))
    })
}

/// One-line no-work summary for `rhei run`. Project-wide it keeps the legacy
/// phrasing; under `--rhei` it names the scope and the blocked in-scope
/// candidates, marking priors that sit outside the scope. §FS-rhei-panta.6.1
fn no_advancement_summary(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> String {
    if scope.is_none() {
        return "No tasks could be advanced.".to_string();
    }
    let mut project = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut project);
    let state_map = plan_state_map(&project, machines);
    let blocked: Vec<String> = project
        .iter()
        .copied()
        // A parent whose subtree is still open is held up by the subtree, not
        // by a prior; its descendants report for themselves.
        // §FS-rhei-plan-language.3
        .filter(|task| descendants_are_terminal(task, machines))
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .filter_map(|task| {
            // Claim, then prior, then the person — the halt classification's
            // order: a claimed ticket is named above with `rhei release`, and
            // the poll resumes itself out of neither. §FS-rhei-run.5.1
            if task.assignee.is_some() {
                return None;
            }
            // The prior answers before the person: it stops the poll from ever
            // coming back, so a label would name a wait nobody can end.
            // §FS-rhei-run-report.3.1 §FS-rhei-states.2.5
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            let waiting_on = first_blocking_prior(task, &state_map, machines, scope).or_else(|| {
                machine
                    .states
                    .get(&state)
                    .and_then(|def| def.waiting_on_person())
                    .map(str::to_string)
            })?;
            Some(format!("Task {} waiting on {}", task.id, waiting_on))
        })
        .collect();
    let detail = if blocked.is_empty() {
        String::new()
    } else {
        let suffix =
            if blocked.len() > 3 { format!(" (+{} more)", blocked.len() - 3) } else { String::new() };
        format!(
            ": {}{}",
            blocked.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
            suffix
        )
    };
    format!("No tasks could be advanced in the --rhei scope ({}){}.", scope_label(scope), detail)
}

fn earliest_pending_poll_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> Option<u64> {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        // §FS-rhei-panta.6.1: a narrowed run never advances out-of-scope
        // tickets, so their poll deadlines must not keep it alive.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter_map(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            machine.states.get(&state).and_then(|def| def.poll.as_ref())?;
            poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &state)
        })
        .filter(|deadline| *deadline > current_unix_secs())
        .min()
}

/// Earliest effective eligibility instant in agent mode. A task subject to
/// both polling and a provider limit waits for the later one; the run wakes for
/// the earliest task that can actually become runnable. §FS-rhei-run.5.1
fn earliest_pending_agent_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    opts: &RunOptions,
    scope: &RheiScope,
) -> Option<u64> {
    let now = current_unix_secs();
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks
        .into_iter()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter_map(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            let state_def = machine.states.get(&state)?;
            if state_def.terminal || state_def.gating {
                return None;
            }
            let poll = state_def
                .poll
                .as_ref()
                .and_then(|_| poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, &state))
                .filter(|deadline| *deadline > now);
            let provider = if state_def.program.is_some() {
                None
            } else {
                resolve_agent_invocations_for_task(machine, &state, settings, opts, Some(task))
                    .ok()?
                    .iter()
                    .filter_map(|resolved| {
                        resolved_provider_deadline(rhei, machines, resolved, now)
                    })
                    .max()
            };
            match (poll, provider) {
                (Some(poll), Some(provider)) => Some(poll.max(provider)),
                (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
                (None, None) => None,
            }
        })
        .min()
}

/// Whether any non-terminal task sits in a gating state — work the run cannot
/// advance without a human decision. Lets an interactive run stay alive so the
/// gate stays resolvable in the UI. §FS-rhei-run-tui.1.5.5
fn has_pending_human_gate(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
) -> bool {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    tasks.iter().any(|task| {
        let machine = machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        machine
            .states
            .get(&state)
            .map(|def| def.gating && !def.terminal)
            .unwrap_or(false)
    })
}

fn should_wait_for_human_gate(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    // The gate itself may sit in any rhei — the TUI shows the whole project —
    // but only in-scope work decides whether waiting can still bear fruit.
    has_pending_human_gate(rhei, machines)
        && remaining_work_is_only_gating_or_poll_blocked(rhei, machines, scope)
}

/// One "is this ticket deliberately waiting?" judgment over a whole plan.
///
/// A ticket is deliberately waiting rather than stuck when it is held open by
/// its own subtree, parked in a gating state, inside a poll backoff window, or
/// waiting on a `**Prior:**` that is itself deliberately waiting.
///
/// One judgment, reached two ways — the top-level scan applies it to every
/// in-scope ticket, and the prior walk applies it to every ticket it reaches.
/// Judging a prior by its own state alone was the first bug: a dependent whose
/// prior is a parent held open by a gated child saw a non-gating state with no
/// priors of its own and read the run as stalled. A parent answers with its
/// subtree, so the dependent inherits the gate.
///
/// Answers are memoized per ticket, and the cycle guard is a *separate*
/// on-stack set. Using one visited set for both was the second bug: a revisit
/// is neutral-true inside the descendant `.all()` and neutral-false inside the
/// prior `.any()`, so a parent over `[gate, sibling waiting on the gate]` read
/// as stuck while the same two children in the other order read as waiting.
// §FS-rhei-plan-language.3 §FS-rhei-run-tui.1.5.5
struct DeliberateWaitJudgment<'a> {
    rhei: &'a rhei_core::ast::Rhei,
    tasks: Vec<&'a rhei_core::ast::Task>,
    state_map: HashMap<&'a TaskId, String>,
    machines: &'a rhei_validator::MachineSet,
    /// Verdicts already reached, so a ticket answers the same whichever walk
    /// arrives at it.
    memo: HashMap<TaskId, bool>,
    /// Tickets on the current recursion path, for cycle detection only.
    stack: HashSet<TaskId>,
}

impl<'a> DeliberateWaitJudgment<'a> {
    fn new(
        rhei: &'a rhei_core::ast::Rhei,
        machines: &'a rhei_validator::MachineSet,
    ) -> Self {
        let mut tasks = Vec::new();
        collect_plan_tasks(&rhei.tasks, &mut tasks);
        let state_map: HashMap<&'a TaskId, String> = tasks
            .iter()
            .map(|task| {
                (&task.id, normalized_state_name(task.state.as_str(), machines.for_task(&task.id)))
            })
            .collect();
        Self { rhei, tasks, state_map, machines, memo: HashMap::new(), stack: HashSet::new() }
    }

    /// The verdict for `task`, or `None` when `task` is already on the current
    /// walk. A cycle contributes nothing to either combinator, so each caller
    /// substitutes its own neutral element: `true` for the descendant `.all()`,
    /// `false` for the prior `.any()`.
    fn judge(&mut self, task: &'a rhei_core::ast::Task) -> Option<bool> {
        if let Some(answer) = self.memo.get(&task.id) {
            return Some(*answer);
        }
        if !self.stack.insert(task.id.clone()) {
            return None;
        }
        let answer = self.compute(task);
        self.stack.remove(&task.id);
        self.memo.insert(task.id.clone(), answer);
        Some(answer)
    }

    fn compute(&mut self, task: &'a rhei_core::ast::Task) -> bool {
        // A parent is not workable until its subtree closes, so it is blocked
        // by exactly whatever blocks its open descendants, each judged by this
        // same walk. §FS-rhei-plan-language.3
        let open = open_descendant_tasks(task, self.machines);
        if !open.is_empty() {
            return open.iter().copied().all(|child| self.judge(child).unwrap_or(true));
        }
        let machine = self.machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        // A terminal gate is a decision already taken, not one still pending —
        // the same reading `has_pending_human_gate` uses.
        if machine.states.get(&state).map(|def| def.gating && !def.terminal).unwrap_or(false) {
            return true;
        }
        if poll_next_attempt_at(self.rhei.metadata.as_ref(), &task.id, &state)
            .is_some_and(|deadline| deadline > current_unix_secs())
        {
            return true;
        }
        for dep_id in &task.prior {
            let Some(dep_state) = self.state_map.get(dep_id).cloned() else {
                continue;
            };
            // The prior's own machine says whether it satisfies.
            // §FS-rhei-panta.6.1
            if dependency_is_satisfied(&dep_state, self.machines.for_task(dep_id)) {
                continue;
            }
            let Some(dep_task) = self.tasks.iter().copied().find(|c| &c.id == dep_id) else {
                continue;
            };
            if self.judge(dep_task) == Some(true) {
                return true;
            }
        }
        false
    }
}

fn remaining_work_is_only_gating_or_poll_blocked(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
) -> bool {
    let mut judgment = DeliberateWaitJudgment::new(rhei, machines);
    let tasks = judgment.tasks.clone();
    tasks
        .into_iter()
        // §FS-rhei-panta.6.1: "remaining work" is in-scope work; priors below
        // still resolve project-wide.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .all(|task| judgment.judge(task).unwrap_or(true))
}
