// What an exited worker's attempt is charged with once the run has decided
// about its reload: the revert kept on its spawn record, what its release and
// the operator are told, the reload that settles a held completion, and the
// charge of a reverted visit.
//
// Its own part because this is the attempt's account, read and written where
// an exit is routed; the registry beside it decides which attempt that is.

// §AR-source-file-size.3 §FS-rhei-run.3.7.4

/// Whether an attributed stop left the held exit recorded at `record` unrouted.
/// §FS-rhei-run.3.7.6
fn stopped_on_break(record: &Path) -> bool {
    worker_regions().stopped.contains(record)
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
        let halt = budget_spent_halt_line(
            &revert.task_id,
            &revert.state,
            budget,
            "the plan, as re-read after the exit, does not load with its edit",
        );
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
/// with no exhaustion edge fired here. §FS-rhei-run.3.7.4
fn charge_reverted_attempt(
    release: &mut PendingSlotRelease,
    reloaded: &LoadedPlan,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    task_id: &str,
    state: &str,
) -> MietteResult<()> {
    release.failed("its edit broke the plan and was reverted");
    let Some(task) = find_task_by_id(&reloaded.rhei.tasks, &parse_task_id(task_id)) else {
        return Ok(());
    };
    record_poll_self_loop_if_needed(reloaded, input, machine, task, state, state).map(|_| ())
}
