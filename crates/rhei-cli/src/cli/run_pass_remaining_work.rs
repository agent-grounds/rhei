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
    !machine.is_cancellation(&normalized_state_name(state, machine))
        && is_terminal_state(state, machine)
}

fn current_unix_secs() -> u64 {
    // Test harness only: the built binary always reads the system clock.
    #[cfg(test)]
    if let Some(held) = HELD_UNIX_SECS.with(std::cell::Cell::get) {
        return held;
    }
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
    spawns: &SpawnSiteRoots<'_>,
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
            let person = || {
                let label = machine.states.get(&state).and_then(|def| def.waiting_on_person());
                label.map(str::to_string)
            };
            // A blocker no clock releases answers before the person too: the poll
            // no more resumes itself out of it than out of a prior. §FS-rhei-run.5.1.1
            let waiting_on = first_blocking_prior(task, &state_map, machines, scope).or_else(|| {
                person().filter(|_| !blocker_no_clock_releases(task, rhei, machines, spawns))
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

/// The callback mode's deadline: the earliest pending poll among the tickets
/// [`agent_deadline_since`] reads, so a run has one deadline scan whatever mode
/// it executes in. §FS-rhei-run.5.1 §FS-rhei-run.5.1.1
fn earliest_pending_poll_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<u64> {
    tickets_only_a_clock_holds(rhei, machines, spawns)
        .into_iter()
        // §FS-rhei-panta.6.1: a narrowed run never advances out-of-scope
        // tickets, so their poll deadlines must not keep it alive.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter_map(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            // Pending as of the reading the loop's ready scan held it at. §FS-rhei-run.5.1
            spawns.pending_poll_deadline(rhei, machine, task, &state, spawns.pending_since())
        })
        .min()
}

/// Earliest effective eligibility instant in agent mode. A task subject to
/// both polling and a provider limit waits for the later one; the run wakes for
/// the earliest task that can actually become runnable, and only a task whose
/// deadline is its sole remaining blocker contributes one at all. Pending means
/// pending as of the instant `spawns` was read, which is when the ready scan
/// held it back. §FS-rhei-run.5.1
fn earliest_pending_agent_deadline(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    scope: &RheiScope,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<u64> {
    agent_deadline_since(rhei, machines, settings, scope, spawns, spawns.pending_since())
}

/// The earliest agent deadline at or after `since`, even one already passed.
/// A pass that read its ready set at `since` has not tried a poll that fell
/// due while it ran, so the pass loop must not halt as if nothing were
/// pending. §FS-rhei-run.3.6 item 3 §FS-rhei-run.5.1
///
/// A poll whose next spawn its site would refuse for a spent bound is stalled,
/// not pending, so it contributes nothing, and `spawns` is where that spawn's
/// record is read and its agents resolved (§FS-rhei-run.3.7.4). A visit whose
/// attempt budget is spent is the same refusal, so no provider window it
/// carries adds anything either (§FS-rhei-agents.3.2.3).
///
/// A provider window is judged at the instant `spawns` was read, as its poll
/// is. §FS-rhei-run.5.1
///
/// Only a ticket whose deadline is the *only* thing between it and this
/// invocation contributes — the filter that turns a schedule hint into a
/// published contract. A deadline on a ticket something else holds is real, but
/// its expiry releases nothing: a timer woken for it finds the same gate, the
/// same claim, the same unfinished prior, the same missing input. So the
/// candidates are the tickets only a clock can be holding, the run's own ready
/// set with the poll clock set aside and the one blocker question asked of it,
/// never a second list of the scan's reasons. The scope filter is the same idea
/// one step out: a deadline on work this command may not run wakes it for
/// nothing either. §FS-rhei-run.5.1.1 §FS-rhei-panta.6.1
fn agent_deadline_since(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    scope: &RheiScope,
    spawns: &SpawnSiteRoots<'_>,
    since: u64,
) -> Option<u64> {
    tickets_only_a_clock_holds(rhei, machines, spawns)
        .into_iter()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter_map(|task| {
            let machine = machines.for_task(&task.id);
            let state = normalized_state_name(task.state.as_str(), machine);
            let state_def = machine.states.get(&state)?;
            let poll = spawns.pending_poll_deadline(rhei, machine, task, &state, since);
            let provider = if state_def.program.is_some() {
                None
            } else {
                let invocations = resolve_agent_invocations_for_task(
                    machine,
                    &state,
                    settings,
                    spawns.opts,
                    Some(task),
                )
                .ok()?;
                let held = |resolved: &ResolvedAgent| {
                    resolved_provider_eligibility_deadline(rhei, machines, resolved, spawns.read_at)
                        .is_some()
                };
                // No arm waits, no completion read: this scan runs on every pass.
                if invocations.iter().any(held) {
                    // A free arm leaves no deadline, else the earliest held one. §FS-rhei-run.5.1
                    spawns
                        .invocations_still_to_spawn(rhei, task, machine, &state, state_def, invocations)
                        .iter()
                        .map(|resolved| {
                            provider_wait_of(rhei, machines, spawns, (machine, task, &state), resolved)
                        })
                        .collect::<Option<Vec<_>>>()
                        .and_then(|deadlines| deadlines.into_iter().min())
                } else {
                    None
                }
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
    spawns: &SpawnSiteRoots<'_>,
) -> bool {
    // The gate itself may sit in any rhei — the TUI shows the whole project —
    // but only in-scope work decides whether waiting can still bear fruit.
    has_pending_human_gate(rhei, machines)
        && remaining_work_is_only_gating_or_poll_blocked(rhei, machines, scope, spawns)
}

/// One "is this ticket deliberately waiting?" judgment over a whole plan.
///
/// A ticket is deliberately waiting rather than stuck when it is held open by
/// its own subtree, parked in a gating state, inside a poll backoff window,
/// parked by a recognized provider limit, or waiting on a `**Prior:**` that is
/// itself deliberately waiting.
///
/// Three rungs of §FS-rhei-run-report.3.1's order answer the other way and so
/// must be walked before the clocks they outrank: a live `**Assignee:**`,
/// which is not deliberate at all, and a supervisor's hold and an unsatisfied
/// `**Prior:**`, which answer with the blocker at the other end — a ticket a
/// cancelled prior holds is not waiting, whatever its own clock says. Only the
/// two clocks after them form a disjunction, so only their order leaves this
/// answer alone — unlike `IdleBlockerScan`, which asks *which* wait and is
/// order-dependent throughout. §FS-rhei-run.5.1.1
///
/// One rung is read out of the published order's turn, and §FS-rhei-run-report.3.1
/// says so: a supervising ticket is classified by its own gate and its own
/// `**Assignee:**` before its subtree, because a supervisor is ready *while*
/// that subtree is open (§FS-rhei-supervision.3.1 rule 1) and its claim is
/// therefore the only thing stopping it. An ordinary parent is never dispatched
/// while its subtree is open, so its own claim is moot and it keeps answering
/// through the subtree.
/// An owed visit skips that subtree rung entirely and reads its own blockers,
/// as the ready scan does. §FS-rhei-run.5.1
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
    index: HashMap<TaskId, &'a rhei_core::ast::Task>,
    state_map: HashMap<&'a TaskId, String>,
    machines: &'a rhei_validator::MachineSet,
    /// Where a poll's spawn record is read, to tell its wait from a spent bound.
    spawns: &'a SpawnSiteRoots<'a>,
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
        spawns: &'a SpawnSiteRoots<'a>,
    ) -> Self {
        let mut tasks = Vec::new();
        collect_plan_tasks(&rhei.tasks, &mut tasks);
        let index = task_index(&tasks);
        let state_map: HashMap<&'a TaskId, String> = tasks
            .iter()
            .map(|task| {
                (&task.id, normalized_state_name(task.state.as_str(), machines.for_task(&task.id)))
            })
            .collect();
        let (memo, stack) = (HashMap::new(), HashSet::new());
        Self { rhei, tasks, index, state_map, machines, spawns, memo, stack }
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
        // A held ticket answers with the blocker at the other end of the hold,
        // and a supervisor parked at a gate has no next visit to release it on.
        // §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.1
        let supervision = supervision_verdict_for(
            task, &self.index, self.machines, self.rhei.metadata.as_ref(), &HashSet::new(),
        );
        if let SupervisionVerdict::Held { supervisor, state } = &supervision {
            if self.machines.for_task(supervisor).states.get(state).is_some_and(|def| def.gating) {
                return true;
            }
            return self.index.get(supervisor).copied()
                .is_some_and(|holder| self.judge(holder) == Some(true));
        }
        let machine = self.machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        // A terminal gate is a decision already taken, not one still pending —
        // the same reading `has_pending_human_gate` uses.
        let own_gate_pending =
            machine.states.get(&state).map(|def| def.gating && !def.terminal).unwrap_or(false);
        // A supervisor is ready *while* its subtree is open, so its own gate
        // and claim are read ahead of the subtree, as `classify_halt` reads
        // them. §FS-rhei-supervision.3.1 §FS-rhei-run-report.3.1
        if task_is_supervising(task, machine) {
            if own_gate_pending {
                return true;
            }
            if task.assignee.is_some() {
                return false;
            }
        }
        // An owed supervisor answers for its own visit despite open children. §FS-rhei-run.5.1
        // Other parents inherit their open descendants' blockers, each judged
        // by this same walk. §FS-rhei-plan-language.3
        let open = open_descendant_tasks(task, self.machines);
        if supervision != SupervisionVerdict::SupervisorReady && !open.is_empty() {
            return open.iter().copied().all(|child| self.judge(child).unwrap_or(true));
        }
        if own_gate_pending {
            return true;
        }
        // Behind the gate — gated *and* claimed is gated, because releasing the
        // claim moves nothing while the gate holds — and ahead of the clocks and
        // the prior, which a claim stops from ever coming back. §FS-rhei-run.3
        if task.assignee.is_some() {
            return false;
        }
        // Ahead of both clocks, as `classify_halt` reads it: a ticket an unsatisfied
        // prior holds waits only when that prior does, never on a cancelled one.
        // §FS-rhei-run.5.1.1
        let priors = live_unsatisfied_priors(task, &self.tasks, &self.state_map, self.machines);
        let Some(priors) = priors else { return false };
        if !priors.is_empty() {
            return priors.into_iter().any(|prior| self.judge(prior) == Some(true));
        }
        // §FS-rhei-run.5.1: a poll whose bound is spent waits for nothing, and every clock
        // here is judged at the instant the plan was read, as the ready scan judged it.
        let (now, since) = (self.spawns.read_at, self.spawns.pending_since());
        // A clock is the ticket's wait only while nothing a clock cannot release holds it.
        // §FS-rhei-run.5.1.1
        let clock_can_wait = || !blocker_no_clock_releases(task, self.rhei, self.machines, self.spawns);
        if self.spawns.pending_poll_deadline(self.rhei, machine, task, &state, since).is_some()
            && clock_can_wait()
        {
            return true;
        }
        // The fourth deliberate wait, belonging with the other three: the
        // report already places a parked ticket in Waiting and never in
        // Attention. §FS-rhei-run.3.3 §FS-rhei-run-report.3.1
        if task_provider_limit(self.rhei, self.machines, task)
            .and_then(|limit| limit.deadline_epoch())
            .is_some_and(|deadline| deadline > now)
            && clock_can_wait()
        {
            return true;
        }
        false
    }
}

fn remaining_work_is_only_gating_or_poll_blocked(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    scope: &RheiScope,
    spawns: &SpawnSiteRoots<'_>,
) -> bool {
    let mut judgment = DeliberateWaitJudgment::new(rhei, machines, spawns);
    let tasks = judgment.tasks.clone();
    tasks
        .into_iter()
        // §FS-rhei-panta.6.1: "remaining work" is in-scope work; priors below
        // still resolve project-wide.
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .all(|task| judgment.judge(task).unwrap_or(true))
}
