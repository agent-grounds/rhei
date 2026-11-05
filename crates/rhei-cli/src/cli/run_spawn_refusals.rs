// Refused allocations belong to this run, not to the worker's record or log.
// Serial dispatch, initial pool fills and refills all retain the same diagnosis.
// §FS-rhei-run.3 §FS-rhei-agents.8.1

/// One invocation whose allocation was refused before a worker started.
/// §FS-rhei-run.3
struct SpawnRefusal {
    task: String,
    state: String,
    identity: Option<String>,
    reason: String,
}

/// Run-local refusals, retained across passes so a poll's refused next look
/// explains the eventual halt too. §FS-rhei-run.3
#[derive(Default)]
struct SpawnRefusals {
    entries: Vec<SpawnRefusal>,
}

impl SpawnRefusals {
    /// Exclusive creation may refuse after a successful preflight. Retain only
    /// that allocation refusal, while the caller preserves ordinary spawn-error
    /// reporting and continue-on-error behavior. §FS-rhei-run.3
    fn completion_error(
        &mut self,
        task: &str,
        state: &str,
        identity: Option<&str>,
        error: &miette::Report,
    ) {
        let Some(refusal) = error.downcast_ref::<ExclusiveLogRefusal>() else { return };
        self.entries.retain(|entry| {
            entry.task != task || entry.state != state || entry.identity.as_deref() != identity
        });
        self.entries.push(SpawnRefusal {
            task: task.to_owned(),
            state: state.to_owned(),
            identity: identity.map(str::to_owned),
            reason: refusal.reason.clone(),
        });
    }

    /// Say and retain an actual scheduling refusal, never a hypothetical poll
    /// eligibility check. A later successful allocation clears that invocation's
    /// earlier refusal without clearing a fan-out sibling's. §FS-rhei-run.3
    fn observe<T>(
        &mut self,
        task: &str,
        state: &str,
        identity: Option<&str>,
        planned: Result<T, String>,
        sink: &Arc<dyn rhei_tui::EventSink>,
    ) -> Result<T, String> {
        self.entries.retain(|entry| {
            entry.task != task || entry.state != state || entry.identity.as_deref() != identity
        });
        if let Err(reason) = &planned {
            emit_run_message(sink, rhei_tui::MessageLevel::Error, format!("  {reason}"));
            self.entries.push(SpawnRefusal {
                task: task.to_owned(),
                state: state.to_owned(),
                identity: identity.map(str::to_owned),
                reason: reason.clone(),
            });
        }
        planned
    }

    /// Repeat the diagnostic lead for tickets still in their refused state;
    /// the original diagnostic above carries the recovery instructions.
    /// Tickets that subsequently moved cannot explain this halt. §FS-rhei-run.3
    fn halt_help(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machines: &rhei_validator::MachineSet,
    ) -> Option<String> {
        let refusals: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let task = find_task_by_id(&rhei.tasks, &parse_task_id(&entry.task))?;
                let machine = machines.for_task(&task.id);
                (normalized_state_name(task.state.as_str(), machine) == entry.state).then(|| {
                    format!(
                        "Task {} in state '{}': {}",
                        entry.task,
                        entry.state,
                        entry.reason.lines().next().unwrap_or_default()
                    )
                })
            })
            .collect();
        (!refusals.is_empty()).then(|| format!(
            "spawning was refused; see the refusal diagnostic above and resolve its cause before re-running.\n{}",
            refusals.join("\n")
        ))
    }
}
