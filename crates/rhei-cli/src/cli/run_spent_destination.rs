// One explanation of successful work held by a counted destination, shared by
// completion warnings and both worked and unworked halt classifications.
// §FS-rhei-errors.1.5.1

/// Real selection enforces every allowance. Diagnostic selection relaxes only
/// `visits:` while keeping the same guards, operands and poll semantics.
/// §FS-rhei-errors.1.5.1
#[derive(Clone, Copy)]
enum EntryBudgetCheck {
    Enforce,
    ExplainVisits,
}

impl EntryBudgetCheck {
    /// Relaxing a visit check never changes what `visits` means in a guard.
    /// §FS-rhei-errors.1.5.1
    fn allows(
        self,
        machine: &rhei_validator::StateMachine,
        metadata: Option<&Metadata>,
        task_id: &TaskId,
        state: &str,
        raw_state: &str,
        to: &str,
    ) -> bool {
        match self {
            Self::Enforce => loop_reentry_allowed(machine, metadata, task_id, state, raw_state, to),
            Self::ExplainVisits => {
                spent_loop_budget(machine, metadata, task_id, state, raw_state, to)
                    .is_none_or(|budget| budget.kind == LoopBudgetKind::Visits)
            }
        }
    }
}

/// The same completion operands and artifact roots used by the worker's exit.
/// No execution or counter changes are made by this diagnostic probe.
/// §FS-rhei-errors.1.5.1
struct NoAdvanceContext<'a> {
    workspace_root: &'a Path,
    artifact_root: &'a Path,
    machine: &'a rhei_validator::StateMachine,
    metadata: Option<&'a Metadata>,
    task: &'a rhei_core::ast::Task,
    state: &'a str,
}

/// Keep the selected program rule so its completion-debt semantics survive the
/// diagnostic selection, just as they do real selection. §FS-rhei-errors.1.5.1
enum SuccessfulExitRoute {
    Agent(String),
    Program(ProgramExitRoute),
}

impl SuccessfulExitRoute {
    /// The destination whose entry allowance was measured. §FS-rhei-errors.1.5.1
    fn to(&self) -> &str {
        match self {
            Self::Agent(to) => to,
            Self::Program(route) => &route.to,
        }
    }
}

impl NoAdvanceContext<'_> {
    /// Ask real selection first so a viable alternative defeats attribution.
    /// Then ask the very same selector with only visits relaxed, and check the
    /// outputs owed on that candidate, including a terminal result.
    /// §FS-rhei-errors.1.5.1
    fn spent_visit(&self) -> Option<SpentLoopBudget> {
        if normalized_state_name(self.task.state.as_str(), self.machine) != self.state {
            return None;
        }
        let is_program = self.machine.states.get(self.state)?.program.is_some();
        let select = |check| {
            if is_program {
                find_program_exit_transition_with_budget(
                    self.machine,
                    self.metadata,
                    self.task,
                    self.state,
                    0,
                    check,
                )
                .map(|route| route.map(SuccessfulExitRoute::Program))
            } else {
                find_next_transition_with_budget(
                    self.task,
                    self.metadata,
                    self.machine,
                    self.task.state.as_str(),
                    check,
                )
                .map(|to| to.map(SuccessfulExitRoute::Agent))
            }
        };
        if select(EntryBudgetCheck::Enforce).ok()?.is_some() {
            return None;
        }
        let route = select(EntryBudgetCheck::ExplainVisits).ok()??;
        let budget = spent_loop_budget(
            self.machine,
            self.metadata,
            &self.task.id,
            self.state,
            self.task.state.as_str(),
            route.to(),
        )?;
        if budget.kind != LoopBudgetKind::Visits {
            return None;
        }
        let missing = match &route {
            SuccessfulExitRoute::Agent(to) => collect_missing_required_outputs(
                self.workspace_root,
                self.artifact_root,
                self.machine,
                self.metadata,
                self.task,
                self.state,
                Some(to),
            ),
            SuccessfulExitRoute::Program(route) => missing_program_exit_outputs(
                self.workspace_root,
                self.artifact_root,
                self.machine,
                self.metadata,
                self.task,
                self.state,
                route,
                0,
            ),
        };
        missing.is_empty().then_some(budget)
    }
}

/// Every successful-worker no-advance warning uses the same wording and event
/// path, including a fallback whose task disappeared during the invocation.
/// §FS-rhei-errors.1.5.1
fn emit_no_advance_warning(
    worker: &str,
    task_id: &str,
    state: &str,
    context: Option<NoAdvanceContext<'_>>,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let refusal = context
        .and_then(|context| context.spent_visit())
        .map(|budget| format!(": {budget}"))
        .unwrap_or_default();
    emit_run_message(
        sink,
        rhei_tui::MessageLevel::Warn,
        format!("  warning: {worker} exited 0 but task {task_id} did not advance from '{state}'{refusal}"),
    );
}

/// Reports explain the same successful completion even on a zero-spawn run.
/// A current-visit record is required: a failed exit or an earlier visit cannot
/// establish a successful-worker hold. Attempt/output causes are checked first
/// by the classifier. §FS-rhei-errors.1.5.1
fn spent_destination_halt(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    state: &str,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<SpentLoopBudget> {
    let id = task.id.to_string();
    let root = spawns.tasks.artifact_root(&id);
    let runtime = if machine.states.get(state)?.program.is_some() {
        root.join("runtime")
    } else {
        spawns.machines.agent_runtime_dir(&id, root, spawns.runtime_dir)
    };
    let moves = ticket_move_count(root, &runtime, &id);
    // End times have second precision; filter by visit before ordering, so a
    // fast loop cannot pick an earlier stay with the same timestamp.
    // §FS-rhei-errors.1.5.1
    let record = spawn_records_for_state(&runtime, &id, state)
        .into_iter()
        .map(|(_, record)| record)
        .filter(|record| record.moves == moves)
        .max_by(|left, right| (&left.ended, left.attempt).cmp(&(&right.ended, right.attempt)))?;
    if !record.proves_successful_work(&id, state, moves) || record.reverted.is_some() {
        return None;
    }
    NoAdvanceContext {
        workspace_root: spawns.tasks.workspace_root,
        artifact_root: root,
        machine,
        metadata: rhei.metadata.as_ref(),
        task,
        state,
    }
    .spent_visit()
}
