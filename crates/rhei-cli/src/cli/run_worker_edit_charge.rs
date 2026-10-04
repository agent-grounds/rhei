// What an exited worker's attempt is charged with once the run has decided
// about its reload: the revert kept on its spawn record, what its release and
// the operator are told, the reload that settles a held completion, and the
// charge of a reverted visit.
//
// Its own part because this is the attempt's account, read and written where
// an exit is routed; the registry beside it decides which attempt that is.

// §AR-source-file-size.3 §FS-rhei-run.3.7.4

impl WorkerRegions {
    /// How the exit held at `record` settles when the run stopped before
    /// routing it: `None` while the run goes on, or once its own completion
    /// settled it; otherwise the restore made on its behalf, which makes it a
    /// reverted attempt that ended on its own, or none, which makes it an
    /// interrupted one. §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.6
    fn held_at_stop(&mut self, record: &Path) -> Option<Option<WorkerRevert>> {
        if !self.stopped {
            return None;
        }
        self.held.remove(record).map(|held| held.revert)
    }
}

impl PendingSlotRelease {
    /// Settle an exit held when the run stopped, as `settled` reads its record:
    /// unrouted however late, failed and announced when its region was
    /// restored, interrupted when not. §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.6
    fn settle_held(&mut self, settled: impl FnOnce(&Path) -> Option<Option<WorkerRevert>>) {
        match self.held.take().as_deref().and_then(settled) {
            Some(Some(revert)) => announce_worker_revert(&revert, self),
            Some(None) => self.interrupted(),
            None => {}
        }
    }
}

/// Keep the revert on the attempt's spawn record, and say what the visit has
/// spent and whether this attempt was charged. §FS-rhei-run.3.7.4
fn record_spawn_revert(record: &Path, edit: &RevertedEdit) -> Option<(u64, bool)> {
    let mut stored = read_spawn_record(record)?;
    stored.reverted = Some(edit.clone());
    let body = serde_json::to_string_pretty(&stored).ok()?;
    fs::write(record, body).ok()?;
    Some((stored.charged, stored.attempt_charged))
}

/// Whether the run reverted the edit of the attempt recorded at `record`, read
/// where an exit is routed on another thread than its restore. §FS-rhei-run.3.7.4
fn attempt_was_reverted(record: &Path) -> bool {
    read_spawn_record(record).is_some_and(|stored| stored.reverted.is_some())
}

/// The worker whose exit found a break the run stops on is recorded the way an
/// interruption is: uncharged, its exit unrouted. §FS-rhei-run.3.7.6
fn uncharge_stopped_attempt(record: &Path) {
    let Some(mut stored) = read_spawn_record(record) else { return };
    if stored.attempt_charged {
        stored.charged = stored.charged.saturating_sub(1);
        stored.attempt_charged = false;
    }
    stored.ending = "interrupted".to_string();
    if let Ok(body) = serde_json::to_string_pretty(&stored) {
        let _ = fs::write(record, body);
    }
}

/// What a budget spent on a reverted last attempt is still owed, as its halt
/// says it wherever the halt is printed. §FS-rhei-run.3.7.4
const REVERTED_EDIT_OWED: &str = "the plan, as re-read after the exit, does not load with its edit";

impl SpawnPlan {
    /// What the halt of a spent budget names as still owed: the reverted edit
    /// when the last attempt's was reverted, else the completion condition's
    /// debt. §FS-rhei-run.3.7.4 §FS-rhei-agents.3.2.1
    fn spent_budget_owed(&self, owed: &[String]) -> String {
        if self.previous.as_ref().is_some_and(|prev| prev.reverted.is_some()) {
            return REVERTED_EDIT_OWED.to_string();
        }
        completion_debt_label(owed)
    }
}

/// Put a restore's key on the attempt's release and tell the operator, with the
/// halt when it spent the last attempt. §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.7
fn announce_worker_revert(revert: &WorkerRevert, release: &mut PendingSlotRelease) {
    let sink = &release.sink.clone();
    release.reverted(&revert.location);
    emit_run_message(sink, rhei_tui::MessageLevel::Warn, revert.warning());
    let budget = match revert.budget {
        AttemptBudget::Visit(budget) => budget,
        AttemptBudget::Poll { max_attempts } => max_attempts,
    };
    if revert.attempt_charged && revert.charged >= budget {
        let halt =
            budget_spent_halt_line(&revert.task_id, &revert.state, budget, REVERTED_EDIT_OWED);
        emit_run_message(sink, rhei_tui::MessageLevel::Warn, halt);
    }
}

/// The reload after a worker's exit. A restore made on this completion's behalf
/// while it was held is told to its release here; when the reload stops the
/// run, the worker is recorded interrupted and uncharged rather than
/// half-recorded. §FS-rhei-run.3.7.5 §FS-rhei-run.3.7.6
fn reload_after_worker_exit(
    input: &Path,
    release: &mut PendingSlotRelease,
    spawn_record: &Path,
) -> MietteResult<LoadedPlan> {
    let reloaded = load_run_plan(input);
    let held = worker_regions().held.remove(spawn_record);
    if let Some(revert) = held.and_then(|held| held.revert) {
        announce_worker_revert(&revert, release);
    }
    reloaded.inspect_err(|_| {
        release.interrupted();
        uncharge_stopped_attempt(spawn_record);
    })
}

/// The reverted visit's charge: a failed attempt and no transition; on a poll
/// state the attempt counts against `poll.max_attempts` as a self-loop's does,
/// with no exhaustion edge fired here. A poll whose bound this attempt spent is
/// stalled rather than scheduled: no attempt follows it, so none is waited for,
/// as none is for a stalled ticket on any other state. §FS-rhei-run.3.7.4
fn charge_reverted_attempt(
    release: &mut PendingSlotRelease,
    reloaded: &LoadedPlan,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    state: &str,
    spawn_record: &Path,
) -> MietteResult<()> {
    release.failed("its edit broke the plan and was reverted");
    let Some(task) = find_task_by_id(&reloaded.rhei.tasks, &parse_task_id(task_id)) else {
        return Ok(());
    };
    let bound = machine.states.get(state).and_then(|def| def.poll.as_ref());
    let spent = bound.zip(read_spawn_record(spawn_record)).is_some_and(|(poll, stored)| {
        stored.reverted.is_some() && stored.charged >= u64::from(poll.max_attempts)
    });
    record_poll_self_loop(reloaded, input, machine, task, state, state, spent.then_some(0))
        .map(|_| ())
}
