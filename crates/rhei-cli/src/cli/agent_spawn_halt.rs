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
                Some(resolved.ok()?)
            }
            (false, None) => return None,
        };
        self.planned_spawns_halt(rhei, machine, task, state_name, agents.as_deref(), budget)
    }

    /// Why every spawn this ticket's state would make next is halted, asked as
    /// its spawn site asks it: the same plan, from the same roots and under the
    /// same name, held to the same gate. So a reader that asks this never waits
    /// for a spawn the spawner will refuse. `Spent` only when every one is
    /// refused for `budget` spent; `Refused` when each is halted and some plan
    /// is refused outright, such as for a taken log name or an unreadable ledger.
    ///
    /// `agents` is `None` for a program state.
    // §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4 §FS-rhei-run.3.7.4 §FS-rhei-run.3.6.1
    fn planned_spawns_halt(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
        agents: Option<&[ResolvedAgent]>,
        budget: AttemptBudget,
    ) -> Option<SpawnHalt> {
        let task_id = task.id.to_string();
        let task_root = self.tasks.artifact_root(&task_id);
        let visit_count = render_visit_count(
            rhei.metadata.as_ref(),
            &task.id,
            state_name,
            task.state.as_str(),
            machine,
        );
        let halt = |runtime_dir: &Path, identity: Option<&str>| {
            let planned = plan_named_spawn(
                machine,
                task_root,
                runtime_dir,
                &task_id,
                state_name,
                visit_count,
                identity,
                &self.machines.reset,
            );
            match planned {
                // A poll visit its kills have spent halts as one its budget has. §FS-rhei-agents.3.2.3
                Ok((plan, _)) => (plan.budget_spent(budget).is_some()
                    || plan.kills_spent(budget).is_some())
                .then_some(SpawnHalt::Spent),
                // A name §FS-rhei-agents.8.1 cannot allocate halts until a fresh read finds it free.
                Err(_) => Some(SpawnHalt::Refused),
            }
        };
        match agents {
            // A program records under its ticket's own root. §FS-rhei-programs.5.1
            None => halt(&task_root.join("runtime"), None),
            Some([]) => None,
            Some(agents) => {
                let runtime_dir =
                    self.machines.agent_runtime_dir(&task_id, task_root, self.runtime_dir);
                agents.iter().try_fold(SpawnHalt::Spent, |all, agent| {
                    let one = halt(&runtime_dir, resolved_agent_log_identity(agent).as_deref())?;
                    Some(all.max(one))
                })
            }
        }
    }
}
