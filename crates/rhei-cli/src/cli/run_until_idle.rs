// The stopping decision `--until-idle` adds, and nothing else.
//
// The option is sleep suppression at the sites that already consult the
// earliest effective deadline: selected, a run performs the same final
// live-member admission checkpoint and returns instead of sleeping. What has
// to be computed once it does is here — whether the in-scope plan is idle at
// all, which kind of wait dominates it, and what a caller should come back
// for — so the two run modes, the console line, the durable report and
// `run_finished` all read one answer.

// §AR-source-file-size.3 §FS-rhei-run.3

/// What a selected run found when it reached its stopping point.
///
/// Its existence *is* the idle verdict: `idle_return` yields one only when
/// in-scope work remains and every bit of it is deliberately waiting.
// §FS-rhei-run.3
struct IdleReturn {
    /// The dominant kind of wait across the in-scope plan.
    blocker: rhei_tui::IdleBlocker,
    /// How many in-scope tickets are waiting, for the console line's counts.
    waiting: usize,
    /// The earliest instant a next invocation could find work, already
    /// rendered; `None` when no in-scope task contributes one.
    next_attempt_at: Option<String>,
}

/// Merge two blocker readings into the one a caller is told about. More than
/// one kind at once is `mixed`; it is never a fifth kind of blocker.
/// §FS-rhei-run-report.3.1
fn merge_blockers(left: Option<rhei_tui::IdleBlocker>, right: Option<rhei_tui::IdleBlocker>) -> Option<rhei_tui::IdleBlocker> {
    match (left, right) {
        (None, other) | (other, None) => other,
        (Some(left), Some(right)) if left == right => Some(left),
        _ => Some(rhei_tui::IdleBlocker::Mixed),
    }
}

/// Which kind of wait the plan-wide classification selects for each ticket.
///
/// It walks the order §FS-rhei-run-report.3.1 publishes — a supervisor's hold,
/// an open descendant subtree, a gate, a live `**Assignee:**`, an unsatisfied
/// `**Prior:**`, a provider limit, then the ticket's own poll — because that is
/// where a blocker's idle-compatibility is decided, and here the order is
/// load-bearing: this walk answers *which* wait, and a ticket with two of them
/// must be named by the one that outranks. The deliberate-wait judgment answers
/// a different question — *is* this deliberate — and that is a disjunction over
/// the same rungs, so its order does not affect its answer and it does not walk
/// this one. For the three structural entries — a supervisor's hold, an open
/// descendant subtree and an unsatisfied `**Prior:**` — this walk follows the
/// classified blocker at the other end, so a ticket held behind a gate reports
/// the gate rather than a wait of its own.
///
/// The descendant rung carries the one exception the same point publishes: a
/// supervising ticket is classified by its own gate and its own `**Assignee:**`
/// before its subtree, because a supervisor is ready *while* that subtree is
/// open (§FS-rhei-supervision.3.1 rule 1), so its claim is the only thing
/// stopping it and `rhei release` the only thing that starts it again. An
/// ordinary parent is never dispatched while its subtree is open, so its own
/// claim is moot and it answers through the subtree as before.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
struct IdleBlockerScan<'a> {
    rhei: &'a rhei_core::ast::Rhei,
    tasks: Vec<&'a rhei_core::ast::Task>,
    state_map: HashMap<&'a TaskId, String>,
    machines: &'a rhei_validator::MachineSet,
    /// Tickets on the current walk, so a prior cycle contributes nothing
    /// rather than recursing forever.
    stack: HashSet<TaskId>,
}

impl<'a> IdleBlockerScan<'a> {
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
        Self { rhei, tasks, state_map, machines, stack: HashSet::new() }
    }

    fn classify(&mut self, task: &'a rhei_core::ast::Task) -> Option<rhei_tui::IdleBlocker> {
        if !self.stack.insert(task.id.clone()) {
            return None;
        }
        let answer = self.compute(task);
        self.stack.remove(&task.id);
        answer
    }

    fn compute(&mut self, task: &'a rhei_core::ast::Task) -> Option<rhei_tui::IdleBlocker> {
        // The head of the order: a held ticket reports the blocker at the
        // other end of the hold, and a gate-parked supervisor's is its gate.
        // §FS-rhei-run-report.3.1 §FS-rhei-supervision.3.1
        if held_by_supervisor(task, self.rhei, self.machines)
            .is_some_and(|hold| hold.awaiting_human)
        {
            return Some(rhei_tui::IdleBlocker::Gate);
        }
        let machine = self.machines.for_task(&task.id);
        let state = normalized_state_name(task.state.as_str(), machine);
        let own_gate_pending =
            machine.states.get(&state).map(|def| def.gating && !def.terminal).unwrap_or(false);
        // A supervisor is ready *while* its subtree is open, so its own gate
        // and claim are read ahead of the subtree, as `classify_halt` reads
        // them. §FS-rhei-supervision.3.1 §FS-rhei-run-report.3.1
        if task_is_supervising(task, machine) {
            if own_gate_pending {
                return Some(rhei_tui::IdleBlocker::Gate);
            }
            if task.assignee.is_some() {
                return None;
            }
        }
        // A parent is blocked by exactly whatever blocks its open descendants.
        // §FS-rhei-plan-language.3
        let open = open_descendant_tasks(task, self.machines);
        if !open.is_empty() {
            return open
                .iter()
                .copied()
                .fold(None, |seen, child| merge_blockers(seen, self.classify(child)));
        }
        if own_gate_pending {
            return Some(rhei_tui::IdleBlocker::Gate);
        }
        // Behind the gate, ahead of the prior: a claim names no idle-compatible
        // wait, and the clock under it would read `waiting on poll; next
        // attempt none`. §FS-rhei-run.3 §FS-rhei-run-report.3.1
        if task.assignee.is_some() {
            return None;
        }
        // Ahead of this ticket's own clock, because that is where the halt
        // classification puts it: a poll behind an unsatisfied prior is held
        // by the prior, not by its clock. §FS-rhei-run-report.3.1
        for dep_id in &task.prior {
            let Some(dep_state) = self.state_map.get(dep_id).cloned() else {
                continue;
            };
            if dependency_is_satisfied(&dep_state, self.machines.for_task(dep_id)) {
                continue;
            }
            let Some(dep_task) = self.tasks.iter().copied().find(|c| &c.id == dep_id) else {
                continue;
            };
            if let Some(blocker) = self.classify(dep_task) {
                return Some(blocker);
            }
        }
        let now = current_unix_secs();
        // §FS-rhei-run.3.3: the fourth deliberate wait, ranked where the halt
        // classification ranks it — above the poll, because a parked ticket's
        // own line already reads as the provider's.
        if task_provider_limit(self.rhei, self.machines, task)
            .and_then(|limit| limit.deadline_epoch())
            .is_some_and(|deadline| deadline > now)
        {
            return Some(rhei_tui::IdleBlocker::ProviderLimit);
        }
        if poll_next_attempt_at(self.rhei.metadata.as_ref(), &task.id, &state)
            .is_some_and(|deadline| deadline > now)
        {
            return Some(rhei_tui::IdleBlocker::Poll);
        }
        None
    }
}

/// The idle verdict for one invocation, or `None` when the run is not idle.
///
/// Three things have to hold and all three are read at the stopping point,
/// against one captured plan: the option is selected, in-scope work remains —
/// a finished plan is complete, not idle — and every in-scope non-terminal
/// ticket is idle-compatible. The third is the deliberate-wait judgment
/// itself, so a real problem beside a wait defeats idle however many tickets
/// wait beside it, and an interrupt is handled by its caller because it
/// outranks both.
// §FS-rhei-run.3 §FS-rhei-run-report.3.1
fn idle_return(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    settings: &RheiSettings,
    opts: &RunOptions,
    scope: &RheiScope,
) -> Option<IdleReturn> {
    if !opts.until_idle() {
        return None;
    }
    if !scoped_unfinished_task_exists(rhei, machines, scope) {
        return None;
    }
    if !remaining_work_is_only_gating_or_poll_blocked(rhei, machines, scope) {
        return None;
    }
    let mut scan = IdleBlockerScan::new(rhei, machines);
    let tasks = scan.tasks.clone();
    let waiting: Vec<&rhei_core::ast::Task> = tasks
        .into_iter()
        .filter(|task| task_in_rhei_scope(scope, &task.id.to_string()))
        .filter(|task| !is_terminal_state(task.state.as_str(), machines.for_task(&task.id)))
        .collect();
    let blocker = waiting
        .iter()
        .copied()
        .fold(None, |seen, task| merge_blockers(seen, scan.classify(task)))
        // Every ticket here is idle-compatible, so one that names no kind is a
        // shape the judgment accepted and this walk has no word for; `mixed`
        // says "more than one plain answer" rather than inventing a fifth.
        .unwrap_or(rhei_tui::IdleBlocker::Mixed);
    let next_attempt_at = earliest_pending_agent_deadline(rhei, machines, settings, opts, scope)
        .map(|deadline| {
            rhei_tui::format_rfc3339(std::time::UNIX_EPOCH + Duration::from_secs(deadline))
        });
    Some(IdleReturn { blocker, waiting: waiting.len(), next_attempt_at })
}

/// The console result line of an idle return.
///
/// `Run idle:` is the discriminator a script keys on, the blocker word and the
/// instant are the same values the stream carries, and the absence of a timed
/// retry is spelled `none` rather than omitted — a line that simply stopped
/// short would read as a truncated one. The line never contains `Run
/// complete:`, which is what makes the two distinguishable at all.
// §FS-rhei-run-report.3.1
fn idle_console_line(idle: &IdleReturn, terminal_count: usize, total_tasks: usize) -> String {
    format!(
        "\nRun idle: {}/{} tasks in terminal state; {} waiting on {}; next attempt {}.",
        terminal_count,
        total_tasks,
        idle.waiting,
        idle.blocker.name(),
        idle.next_attempt_at.as_deref().unwrap_or("none")
    )
}

/// The one informational notice the ruling permitted, taken up.
///
/// It reaches the operator who typed the flag at a terminal and got lines, and
/// nobody else: never on stdout, never in the `--json` stream, never in
/// `runtime/events.jsonl`, and not at all when the caller already said which
/// surface it wanted.
// §FS-rhei-run-tui.1.4
fn announce_line_output_for_until_idle(opts: &RunOptions) {
    if !opts.until_idle() || opts.json() || opts.explicit_frontend() {
        return;
    }
    if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        eprintln!("note: --until-idle implies line output; the TUI does not return on its own.");
    }
}

/// `run_finished.summary.stop`, which a selected run carries whatever it
/// decided — the option splits one collapsed exit code, so the split is
/// reported for every outcome and not only for the new one.
///
/// `interrupted` outranks everything, attention outranks idle, and a plan with
/// no in-scope work left is complete. The exit code beside each reason is the
/// one the process really leaves with (§FS-rhei-run-json.5), so a consumer
/// never has to re-derive it from the word.
// §FS-rhei-run-json.2.1
fn run_stop_payload(
    rhei: &rhei_core::ast::Rhei,
    machines: &rhei_validator::MachineSet,
    opts: &RunOptions,
    scope: &RheiScope,
    idle: Option<&IdleReturn>,
    interrupted: bool,
) -> Option<rhei_tui::RunStop> {
    if !opts.until_idle() {
        return None;
    }
    let (reason, exit_code) = if interrupted {
        (rhei_tui::StopReason::Interrupted, interrupt_exit_code().unwrap_or(1))
    } else if let Some(idle) = idle {
        return Some(rhei_tui::RunStop {
            reason: rhei_tui::StopReason::Idle,
            idle_blocker: Some(idle.blocker),
            next_attempt_at: idle.next_attempt_at.clone(),
            exit_code: EXIT_IDLE,
        });
    } else if scoped_unfinished_task_exists(rhei, machines, scope) {
        (rhei_tui::StopReason::Attention, 1)
    } else {
        (rhei_tui::StopReason::Complete, 0)
    };
    Some(rhei_tui::RunStop { reason, idle_blocker: None, next_attempt_at: None, exit_code })
}

/// The line-oriented tail an end-of-run summary prints once it has said how
/// the run ended: the state counts, then one line per ticket.
///
/// Returned rather than printed because each run mode's `run_info!` is a local
/// macro over that run's own sink.
// §FS-rhei-run-report.3.1
fn final_state_lines(rhei: &rhei_core::ast::Rhei) -> Vec<String> {
    let mut tasks = Vec::new();
    collect_plan_tasks(&rhei.tasks, &mut tasks);
    let mut lines = vec![format!("Final states: {}", format_state_counts(rhei))];
    lines.extend(
        tasks.into_iter().map(|task| format!("  - {} [{}]", format_task_label(task), task.state)),
    );
    lines
}
