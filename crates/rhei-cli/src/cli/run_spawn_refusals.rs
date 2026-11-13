// Refused allocations and admissions belong to this run, not to a worker's log.
// Serial dispatch, initial pool fills and refills all retain the same diagnosis.
// §FS-rhei-run.3.4 §FS-rhei-agents.8.1

/// One invocation refused before a worker started. §FS-rhei-run.3.4
struct SpawnRefusal {
    task: String,
    state: String,
    identity: Option<String>,
    /// Admission belongs to this arrival, including uncounted ledger entries;
    /// allocation keeps its existing state-based lifetime. §FS-rhei-run.3.4
    arrival: Option<LogNumber>,
    reason: String,
}

/// Run-local refusals, retained across passes so a poll's refused next look
/// explains the eventual halt too. Admission and allocation evidence have
/// independent lifetimes for each invocation. §FS-rhei-run.3.4
#[derive(Default)]
struct SpawnRefusals {
    entries: Vec<SpawnRefusal>,
    admissions: Vec<SpawnRefusal>,
}

impl SpawnRefusals {
    /// Report the actual admission once, identically for serial dispatch and
    /// every pool fill, and retain its cause for the final halt. Success clears
    /// only this invocation's admission evidence, never an allocation refusal
    /// or another fanout arm's evidence. `None` takes the caller's stall path;
    /// `Some` carries the admitted accounting arm, if any. §FS-rhei-run.3.4
    fn admission(
        &mut self,
        task: &str,
        state: &str,
        identity: Option<&str>,
        arrival: LogNumber,
        admitted: BudgetAdmission,
        sink: &Arc<dyn rhei_tui::EventSink>,
    ) -> Option<Option<String>> {
        self.admissions.retain(|entry| {
            entry.task != task || entry.state != state || entry.identity.as_deref() != identity
        });
        match admitted {
            BudgetAdmission::Admitted { bounds, note, arm } => {
                sink.emit(rhei_tui::RunEvent::BudgetSnapshot { bounds });
                if let Some(note) = note {
                    emit_run_message(sink, rhei_tui::MessageLevel::Info, note);
                }
                Some(arm)
            }
            BudgetAdmission::NotAccounted => Some(None),
            BudgetAdmission::Refused { halt, event, bounds } => {
                // Keep the full facts/events even though the final help repeats
                // only their lead and points above. §FS-rhei-run.3.4 §FS-rhei-budgets.9
                if !bounds.is_empty() {
                    sink.emit(rhei_tui::RunEvent::BudgetSnapshot { bounds });
                }
                emit_run_message(sink, rhei_tui::MessageLevel::Error, halt.clone());
                self.admissions.push(SpawnRefusal {
                    task: task.to_owned(),
                    state: state.to_owned(),
                    identity: identity.map(str::to_owned),
                    arrival: Some(arrival),
                    reason: halt,
                });
                if let Some(event) = event {
                    sink.emit(*event);
                }
                None
            }
        }
    }

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
            arrival: None,
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
                arrival: None,
                reason: reason.clone(),
            });
        }
        planned
    }

    /// Repeat the diagnostic lead for tickets still in their refused state;
    /// the original diagnostic above carries the recovery instructions.
    /// Tickets that subsequently moved cannot explain this halt. §FS-rhei-run.3.4
    fn halt_help(
        &self,
        rhei: &rhei_core::ast::Rhei,
        spawns: &SpawnSiteRoots<'_>,
        settings: &RheiSettings,
    ) -> MietteResult<Option<String>> {
        let mut refusals = Vec::new();
        for entry in self.entries.iter().chain(&self.admissions) {
            let Some(task) = find_task_by_id(&rhei.tasks, &parse_task_id(&entry.task)) else { continue };
            let machine = spawns.machines.for_task(&task.id);
            if normalized_state_name(task.state.as_str(), machine) != entry.state {
                continue;
            }
            if let Some(arrival) = entry.arrival {
                // Use the spawn's own arrival identity: an uncounted round trip
                // changes its ledger entry even between scans. §FS-rhei-run.3.4
                let task_root = spawns.tasks.artifact_root(&entry.task);
                let runtime = spawns.machines.agent_runtime_dir(&entry.task, task_root, spawns.runtime_dir);
                let visit = render_visit_count(
                    rhei.metadata.as_ref(), &task.id, &entry.state, task.state.as_str(), machine,
                );
                // The ticket is still in the refused state, checked above. §FS-rhei-agents.8.1
                let now =
                    resolve_log_number_report(machine, &entry.state, visit, task_root, &runtime, &entry.task, Stay::Current)?;
                if now != arrival {
                    continue;
                }
                // Reconcile against all requested arms, using the scheduler's
                // resolver; keep still-required fanout siblings. §FS-rhei-run.3.4
                let invocations = resolve_agent_invocations_for_task(
                    machine, &entry.state, settings, spawns.opts, Some(task),
                )?;
                if !invocations.iter().any(|agent| resolved_agent_log_identity(agent) == entry.identity) {
                    continue;
                }
            }
            refusals.push(format!(
                "Task {} in state '{}': {}",
                entry.task, entry.state, entry.reason.lines().next().unwrap_or_default()
            ));
        }
        Ok((!refusals.is_empty()).then(|| format!(
            "spawning was refused; see the refusal diagnostic above and resolve its cause before re-running.\n{}",
            refusals.join("\n")
        )))
    }
}
