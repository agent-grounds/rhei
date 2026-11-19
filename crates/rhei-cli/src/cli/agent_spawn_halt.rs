// Why a ticket's every next spawn is halted before it starts, read as its spawn
// site would read it: a bound that is spent, or a plan refused outright. The
// deadline scan's spent budget and the deadline sleep's kept stalls both ask
// this one reading.
//
// Apart from the plan because a plan is one spawn's next attempt, or the refusal
// of it, while a halt is what every plan the ticket's state would make says
// together: whether the run may start the ticket at all.

// §AR-source-file-size.3 §FS-rhei-run.3.6.1 §FS-rhei-run.5.1.1

/// Why a ticket's every next spawn is halted before it starts. Ordered so a
/// fan-out with one arm spent and another refused reads as refused.
/// §FS-rhei-run.3.6.1
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SpawnHalt {
    /// Every spawn is refused for the bound its site holds it to. §FS-rhei-run.3.7.4
    Spent,
    /// Some spawn's planning is refused outright, such as for a log name no
    /// spawn record accounts for. §FS-rhei-agents.8.1
    Refused,
}

/// The active invocations' next plans, read once from their current visit.
/// A missing plan is a planning refusal, never evidence of exhaustion.
/// Both clock eligibility and the halt explanation use this reading, including
/// effective retry grants, rather than inspecting historical records themselves.
/// §FS-rhei-run-report.3.1 §FS-rhei-run.5.1.1
struct TicketSpawnPlans {
    budget: AttemptBudget,
    invocations: Vec<(Option<ResolvedAgent>, Option<SpawnPlan>)>,
}

impl TicketSpawnPlans {
    /// Every pending spawn must be halted; a planning refusal outranks spend.
    /// §FS-rhei-run.3.6.1 §FS-rhei-run-report.3.1
    fn halt(&self) -> Option<SpawnHalt> {
        if self.invocations.is_empty() {
            return None;
        }
        self.invocations.iter().try_fold(SpawnHalt::Spent, |all, (_, plan)| {
            let one = match plan {
                Some(plan) => (plan.budget_spent(self.budget).is_some()
                    || plan.kills_spent(self.budget).is_some())
                .then_some(SpawnHalt::Spent)?,
                None => SpawnHalt::Refused,
            };
            Some(all.max(one))
        })
    }
}

impl SpawnSiteRoots<'_> {
    /// Why every spawn this ticket's state would make next is halted, if each
    /// is: the bound its spawn site holds it to is spent - `poll.max_attempts`
    /// in a poll state, the visit's `attempts:` with any `rhei retry` grant
    /// elsewhere - or its planning is refused, such as a log name that cannot
    /// be allocated. Read afresh from the spawn records and the logs each time,
    /// so a retry, an advance or a freed name answers `None` the moment it
    /// lands. A state, agents or settings that do not resolve answer `None`.
    ///
    /// Settings are read only where the answer needs them: to resolve an agent
    /// state's agents, or a visit's configured `attempts:`. A program poll's
    /// budget is its own `max_attempts`, so it answers from its spawn record
    /// alone. §FS-rhei-run.3.7.4 §FS-rhei-run.3.6.1
    fn next_spawns_halt(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
    ) -> Option<SpawnHalt> {
        self.next_spawn_plans(rhei, machine, task, state_name)?.halt()
    }

    /// Plan only invocations the spawn site still owes, under its active
    /// settings, roots, state and visit. Completed arms and dormant identities
    /// cannot establish or hide a spent current visit.
    /// §FS-rhei-run-report.3.1 §FS-rhei-run.3.6.1
    fn next_spawn_plans(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
    ) -> Option<TicketSpawnPlans> {
        let state_def = machine.states.get(state_name)?;
        let poll = poll_attempt_budget(state_def);
        let mut read = None;
        let settings = if state_def.program.is_none() || poll.is_none() {
            Some(self.settings_or_merged(&mut read)?)
        } else {
            None
        };
        let budget = match (poll, settings) {
            (Some(budget), _) => budget,
            // The budget the spawn site resolves, through the same chain. §FS-rhei-agents.3.2.1
            (None, Some(settings)) => resolve_attempt_budget(Some(state_def), settings),
            (None, None) => return None,
        };
        let agents = match (state_def.program.is_some(), settings) {
            (true, _) => None,
            (false, Some(settings)) => {
                let resolved = resolve_agent_invocations_for_task(
                    machine,
                    state_name,
                    settings,
                    self.opts,
                    Some(task),
                );
                Some(self.invocations_still_to_spawn(
                    rhei, task, machine, state_name, state_def, resolved.ok()?,
                ))
            }
            (false, None) => return None,
        };
        let task_id = task.id.to_string();
        let task_root = self.tasks.artifact_root(&task_id);
        let visit_count = render_visit_count(
            rhei.metadata.as_ref(),
            &task.id,
            state_name,
            task.state.as_str(),
            machine,
        );
        let plan = |runtime_dir: &Path, identity: Option<&str>| {
            plan_named_spawn(
                machine,
                task_root,
                runtime_dir,
                &task_id,
                state_name,
                visit_count,
                identity,
                &self.machines.reset,
            ).ok().map(|(plan, _)| plan)
        };
        let invocations = match agents {
            // A program records under its ticket's own root. §FS-rhei-programs.5.1
            None => vec![(None, plan(&task_root.join("runtime"), None))],
            Some(agents) => {
                let runtime_dir =
                    self.machines.agent_runtime_dir(&task_id, task_root, self.runtime_dir);
                agents.into_iter().map(|agent| {
                    let planned = plan(&runtime_dir, resolved_agent_log_identity(&agent).as_deref());
                    (Some(agent), planned)
                }).collect()
            }
        };
        Some(TicketSpawnPlans { budget, invocations })
    }
}
