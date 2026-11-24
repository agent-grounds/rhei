// One explanation of successful work whose edge its destination refused (spent `visits:`,
// missing input), shared by warnings, halt classifications, spent-visit halt lines and
// `rhei retry`. §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2

/// How a destination refused the edge a successful completion selected, as the
/// entry check measured it. §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
#[derive(Debug, Clone, PartialEq, Eq)]
enum DestinationRefusal {
    /// The destination's `visits:` budget is spent. §FS-rhei-errors.1.5.1
    Visits(SpentLoopBudget),
    /// Required `inputs:` of `to` are not on disk, each `name (<absolute path>)`.
    /// §FS-rhei-errors.1.5.2
    MissingInputs { to: String, entries: Vec<String> },
}

impl DestinationRefusal {
    /// The state whose entry was refused. §FS-rhei-errors.1.5.2
    fn destination(&self) -> &str {
        match self {
            Self::Visits(budget) => &budget.state,
            Self::MissingInputs { to, .. } => to,
        }
    }

    /// The refusal in the words of the check that made it; a spent budget keeps
    /// the warning's exact wording. §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
    fn subject(&self) -> String {
        match self {
            Self::Visits(budget) => budget.to_string(),
            Self::MissingInputs { entries, .. } if entries.len() == 1 => {
                format!("required input {} is missing", entries[0])
            }
            Self::MissingInputs { entries, .. } => {
                format!("required inputs {} are missing", entries.join(", "))
            }
        }
    }

    /// The refused edge, named by its destination and the refusal.
    /// §FS-rhei-errors.1.5.2
    fn edge(&self) -> String {
        format!("its edge into '{}' is refused: {}", self.destination(), self.subject())
    }

    /// What releases the edge, before a rerun or a grant can matter.
    /// §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
    fn release(&self) -> String {
        match self {
            Self::Visits(budget) => format!("raise `visits:` on state '{}'", budget.state),
            Self::MissingInputs { entries, .. } if entries.len() == 1 => "write that file".into(),
            Self::MissingInputs { .. } => "write those files".into(),
        }
    }

    /// The Attention reason and next action of a finished worker held only by
    /// this refusal; a spent budget keeps #764's row. §FS-rhei-errors.1.5.1
    /// §FS-rhei-errors.1.5.2 §FS-rhei-run-report.3.1
    fn attention(&self) -> (String, String) {
        match self {
            Self::Visits(budget) => (
                budget.to_string(),
                format!("raise `visits:` on state '{}', or cancel the ticket", budget.state),
            ),
            Self::MissingInputs { to, .. } => (
                format!("finished, but cannot enter '{to}': {}", self.subject()),
                format!("{} and rerun, or cancel the ticket", self.release()),
            ),
        }
    }

    /// What a spent visit's halt line says after its attempts in place of an
    /// unmet completion condition: the refused edge and what to do before the
    /// grant the line ends with. §FS-rhei-errors.1.5.2 §FS-rhei-agents.3.2.1
    fn spent_visit_hold(&self) -> String {
        let release = match self {
            Self::MissingInputs { entries, .. } if entries.len() == 1 => "write it".into(),
            Self::MissingInputs { .. } => "write them".into(),
            Self::Visits(_) => self.release(),
        };
        let edge = self.edge();
        format!("the last one met its completion condition, but {edge}; {release} before retrying")
    }
}

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
/// §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
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
    /// Ask real selection first. An edge it selects is the one the completion
    /// would take, so only its destination's inputs can refuse it; selection is
    /// never changed to find another. With none selected, ask the very same
    /// selector with only visits relaxed. Either way the outputs owed on that
    /// candidate, including a terminal result, must be on disk.
    /// §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
    fn refused_entry(&self) -> Option<DestinationRefusal> {
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
        let (route, spent) = match select(EntryBudgetCheck::Enforce).ok()? {
            Some(route) => (route, None),
            None => {
                let route = select(EntryBudgetCheck::ExplainVisits).ok()??;
                let budget = spent_loop_budget(
                    self.machine,
                    self.metadata,
                    &self.task.id,
                    self.state,
                    self.task.state.as_str(),
                    route.to(),
                )?;
                (budget.kind == LoopBudgetKind::Visits).then_some(())?;
                (route, Some(budget))
            }
        };
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
        if !missing.is_empty() {
            return None;
        }
        match spent {
            Some(budget) => Some(DestinationRefusal::Visits(budget)),
            None => self.missing_destination_inputs(route.to()),
        }
    }

    /// The destination's required inputs the entry would refuse, resolved as
    /// the transition resolves them: against this ticket's artifact root, at
    /// the visit entering would make, and made absolute. Optional inputs never
    /// refuse entry and are never named. §FS-rhei-errors.1.5.2 §FS-rhei-states.3.1
    fn missing_destination_inputs(&self, to: &str) -> Option<DestinationRefusal> {
        let definition = self.machine.states.get(to)?;
        if definition.inputs.iter().all(|input| input.optional) {
            return None;
        }
        let settings = load_merged_settings(self.workspace_root).ok()?;
        let visit = update_metadata_for_transition(self.metadata, &self.task.id, to, self.machine)
            .map(|entered| task_visit_count(Some(&entered), &self.task.id, to))
            .filter(|count| *count > 0);
        let entries = missing_state_inputs_for_transition(
            self.artifact_root,
            Some(self.task),
            &self.task.id.to_string(),
            to,
            definition,
            visit,
            self.machine,
            &settings,
        );
        (!entries.is_empty()).then(|| DestinationRefusal::MissingInputs { to: to.into(), entries })
    }
}

/// Every successful-worker no-advance warning uses the same wording and event
/// path, including a fallback whose task disappeared during the invocation.
/// Only an unselected edge reaches it, so only a spent budget is appended: a
/// selected edge refused for an input warns with the refusal itself.
/// §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
fn emit_no_advance_warning(
    worker: &str,
    task_id: &str,
    state: &str,
    context: Option<NoAdvanceContext<'_>>,
    sink: &Arc<dyn rhei_tui::EventSink>,
) {
    let refusal = context
        .and_then(|context| context.refused_entry())
        .map(|refusal| format!(": {}", refusal.subject()))
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
/// by the classifier. §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
fn refused_destination_halt(
    task: &rhei_core::ast::Task,
    rhei: &rhei_core::ast::Rhei,
    machine: &rhei_validator::StateMachine,
    state: &str,
    spawns: &SpawnSiteRoots<'_>,
) -> Option<DestinationRefusal> {
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
    // §FS-rhei-errors.1.5.1 §FS-rhei-errors.1.5.2
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
    .refused_entry()
}

/// The halt line of a spent visit at every spawn site, agent or program, alone or
/// pooled. It names what the completion condition still owes, or, when the last
/// attempt met it and exited 0, the destination that refused the edge it selected.
/// The spent attempts and the ending stay as they are. §FS-rhei-agents.3.2.1
/// §FS-rhei-errors.1.5.2 §FS-rhei-retry.3
fn spent_visit_halt_line(
    task_id: &str,
    site: NoAdvanceContext<'_>,
    rhei: &rhei_core::ast::Rhei,
    plan: &SpawnPlan,
    spent: u64,
    budget: AttemptBudget,
) -> String {
    let is_program = site.machine.states.get(site.state).is_some_and(|def| def.program.is_some());
    let owed: Vec<String> = if is_program {
        missing_previous_program_outputs(
            site.artifact_root, site.machine, site.metadata, site.task, site.state, plan,
        )
        .iter()
        .map(MissingRequiredOutput::warning_entry)
        .collect()
    } else {
        collect_missing_required_outputs(
            site.workspace_root,
            site.artifact_root,
            site.machine,
            site.metadata,
            site.task,
            site.state,
            selected_forward_transition(rhei, site.machine, site.task).as_deref(),
        )
    };
    let recovery = visit_halt_recovery(budget, site.machine, site.metadata, site.task);
    // A reverted last attempt keeps its own hold. §FS-rhei-run.3.7.4
    let refused = plan
        .previous
        .as_ref()
        .filter(|last| owed.is_empty() && last.reverted.is_none())
        .filter(|last| last.proves_successful_work(&last.task, &last.state, plan.moves))
        .and_then(|_| site.refused_entry());
    let hold = match refused {
        Some(refusal) => refusal.spent_visit_hold(),
        None => spent_visit_unmet(&plan.spent_budget_owed(&owed)),
    };
    render_spent_visit_hold(task_id, site.state, spent, &hold, recovery)
}
