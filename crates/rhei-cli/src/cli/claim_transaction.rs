// The persistence boundary unique to an auto-advancing `rhei next` claim.
//
// Its own part because ordinary transitions intentionally keep their existing
// behavior. This adapter holds the shared ledger lock while the claim's plan
// writes can still be reversed.

// §AR-source-file-size.3 §FS-rhei-next.3.1

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaimFaultPoint {
    LedgerPreflight,
    StateMetadataWrite,
    StateTaskWrite,
    AssigneeWrite,
    LedgerAppend,
    RestoreMetadata,
    RestoreTask,
    RestoreLedger,
}

#[cfg(test)]
thread_local! {
    static CLAIM_FAULTS: std::cell::RefCell<Vec<(ClaimFaultPoint, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static CLAIM_BEFORE_LOCK_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    static CLAIM_AFTER_STATE_WRITE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Install one-shot, thread-local persistence failures for focused unit tests.
/// No command flag or authored setting can reach this seam.
#[cfg(test)]
fn set_claim_faults(faults: Vec<(ClaimFaultPoint, &str)>) {
    CLAIM_FAULTS.with(|installed| {
        *installed.borrow_mut() = faults
            .into_iter()
            .map(|(point, message)| (point, message.to_string()))
            .collect();
    });
}

#[cfg(test)]
fn take_claim_fault(point: ClaimFaultPoint) -> Option<String> {
    CLAIM_FAULTS.with(|installed| {
        let mut installed = installed.borrow_mut();
        let index = installed.iter().position(|(candidate, _)| *candidate == point)?;
        Some(installed.remove(index).1)
    })
}

/// Install a one-shot mutation immediately after `next` selects a task and
/// before its claim path acquires the plan lock. Unit tests use this to make a
/// stale selection deterministic; no command flag or authored setting can
/// reach the seam. §FS-rhei-next.3.1
#[cfg(test)]
fn set_claim_before_lock_hook(hook: impl FnOnce() + 'static) {
    CLAIM_BEFORE_LOCK_HOOK.with(|installed| {
        *installed.borrow_mut() = Some(Box::new(hook));
    });
}

#[cfg(test)]
fn run_claim_before_lock_hook() {
    let hook = CLAIM_BEFORE_LOCK_HOOK.with(|installed| installed.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

/// Pause after the provisional state replacement while the complete writer
/// lock stack remains held. Focused tests use this to start an ordinary writer
/// in the former inode-lock gap. §AR-agent-orchestrator-workflow.3.3.1
#[cfg(test)]
fn set_claim_after_state_write_hook(hook: impl FnOnce() + 'static) {
    CLAIM_AFTER_STATE_WRITE_HOOK.with(|installed| {
        *installed.borrow_mut() = Some(Box::new(hook));
    });
}

#[cfg(test)]
fn run_claim_after_state_write_hook() {
    let hook = CLAIM_AFTER_STATE_WRITE_HOOK.with(|installed| installed.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

/// Original bytes and serialized bookkeeping retained until a claim commits.
struct ClaimTransaction<'a> {
    files: TransitionFiles<'a>,
    metadata_handle: &'a LockedPlanFile,
    task_handle: Option<&'a LockedPlanFile>,
    metadata_raw: &'a str,
    task_raw: &'a str,
    ledger: LockedTransitionLedger,
}

struct ClaimRollback {
    error: Report,
    restored: bool,
}

impl<'a> ClaimTransaction<'a> {
    /// Preflight and lock the ledger before the first plan byte changes.
    fn begin(
        files: TransitionFiles<'a>,
        metadata_handle: &'a LockedPlanFile,
        task_handle: Option<&'a LockedPlanFile>,
        metadata_raw: &'a str,
        task_raw: &'a str,
    ) -> MietteResult<Self> {
        #[cfg(test)]
        if let Some(message) = take_claim_fault(ClaimFaultPoint::LedgerPreflight) {
            return Err(miette!(
                help = transition_log_help(),
                "ledger preflight injection: {message}"
            ));
        }
        let ledger = LockedTransitionLedger::open(files.artifact_root)?;
        Ok(Self { files, metadata_handle, task_handle, metadata_raw, task_raw, ledger })
    }

    fn write_state(
        &mut self,
        metadata_raw_updated: &str,
        task_raw_updated: Option<&str>,
    ) -> MietteResult<()> {
        #[cfg(test)]
        if let Some(message) = take_claim_fault(ClaimFaultPoint::StateMetadataWrite) {
            return Err(miette!(
                help = runtime_dir_help(),
                "metadata persistence injection: {message}"
            ));
        }
        write_file_atomic_locked(
            self.files.metadata_file,
            metadata_raw_updated,
            Some(self.metadata_handle),
        )?;
        if let Some(task_raw_updated) = task_raw_updated {
            #[cfg(test)]
            if let Some(message) = take_claim_fault(ClaimFaultPoint::StateTaskWrite) {
                return Err(miette!(
                    help = runtime_dir_help(),
                    "task persistence injection: {message}"
                ));
            }
            write_file_atomic_locked(self.files.task_file, task_raw_updated, self.task_handle)?;
        }
        Ok(())
    }

    /// Add ownership to the target-state view with one more atomic rewrite.
    fn write_assignee(
        &mut self,
        metadata_raw_updated: &str,
        task_raw_updated: Option<&str>,
        local_id: &str,
        assignee: &str,
    ) -> MietteResult<()> {
        #[cfg(test)]
        if let Some(message) = take_claim_fault(ClaimFaultPoint::AssigneeWrite) {
            return Err(miette!(
                help = "check that the task file is writable",
                "assignee persistence injection: {message}"
            ));
        }
        if let Some(task_raw_updated) = task_raw_updated {
            let claimed = insert_task_assignee(task_raw_updated, local_id, assignee)?;
            write_file_atomic_locked(self.files.task_file, &claimed, self.task_handle)
        } else {
            let claimed = insert_task_assignee(metadata_raw_updated, local_id, assignee)?;
            write_file_atomic_locked(
                self.files.metadata_file,
                &claimed,
                Some(self.metadata_handle),
            )
        }
    }

    /// The serialized ledger line is the last pre-commit operation.
    fn commit(&mut self, from: &str, to: &str) -> MietteResult<()> {
        self.ledger.append(self.files.artifact_id, from, to)
    }

    /// Restore every local persistence surface before the plan locks go away.
    fn rollback(&mut self, original: Report) -> ClaimRollback {
        let mut failures = Vec::new();
        #[cfg(test)]
        let metadata_restore_fault =
            take_claim_fault(ClaimFaultPoint::RestoreMetadata).map(|message| {
                miette!(help = runtime_dir_help(), "metadata restoration injection: {message}")
            });
        #[cfg(not(test))]
        let metadata_restore_fault: Option<Report> = None;
        let metadata_restore = metadata_restore_fault.map_or_else(
            || {
                write_file_atomic_locked(
                    self.files.metadata_file,
                    self.metadata_raw,
                    Some(self.metadata_handle),
                )
            },
            Err,
        );
        if let Err(err) = metadata_restore {
            failures.push(format!("metadata restore failed: {err}"));
        }
        if self.files.task_file != self.files.metadata_file {
            #[cfg(test)]
            let task_restore_fault = take_claim_fault(ClaimFaultPoint::RestoreTask)
                .map(|message| {
                    miette!(help = runtime_dir_help(), "task restoration injection: {message}")
                });
            #[cfg(not(test))]
            let task_restore_fault: Option<Report> = None;
            let task_restore = task_restore_fault.map_or_else(
                || {
                    write_file_atomic_locked(
                        self.files.task_file,
                        self.task_raw,
                        self.task_handle,
                    )
                },
                Err,
            );
            if let Err(err) = task_restore {
                failures.push(format!("task restore failed: {err}"));
            }
        }
        #[cfg(test)]
        let ledger_restore_fault = take_claim_fault(ClaimFaultPoint::RestoreLedger)
            .map(|message| {
                miette!(help = transition_log_help(), "ledger restoration injection: {message}")
            });
        #[cfg(not(test))]
        let ledger_restore_fault: Option<Report> = None;
        let ledger_restore = ledger_restore_fault.map_or_else(|| self.ledger.restore(), Err);
        if let Err(err) = ledger_restore {
            failures.push(format!("ledger restore failed: {err}"));
        }
        if failures.is_empty() {
            return ClaimRollback { error: original, restored: true };
        }

        ClaimRollback {
            error: miette!(
                help = "the claim could not be restored completely; inspect every named path before taking further action",
                "claim failed ({original}); restoration also failed ({}). Affected paths: task={}, metadata={}, ledger={}",
                failures.join("; "),
                self.files.task_file.display(),
                self.files.metadata_file.display(),
                self.ledger.path().display(),
            ),
            restored: false,
        }
    }
}

/// The declared state transition in `error_handling.on_enter_failure`, if any.
///
/// Error-handling actions are still an intentionally loose YAML surface. The
/// claim path needs only the established `transition_to: <state>` action and
/// reads that one value without broadening the public machine model.
// §FS-rhei-transitions.4.9 §FS-rhei-next.3.1
fn configured_claim_recovery_target(
    state_machine_path: Option<&Path>,
) -> MietteResult<Option<String>> {
    let Some(path) = state_machine_path else { return Ok(None) };
    let raw = fs::read_to_string(path)
        .map_err(|err| file_io_report(path, "failed to read state machine recovery policy", err))?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|err| {
        miette!(
            help = state_machine_help(),
            "failed to parse state machine recovery policy: {err}"
        )
    })?;
    let actions = yaml
        .get("error_handling")
        .and_then(|value| value.get("on_enter_failure"))
        .and_then(serde_yaml::Value::as_sequence);
    Ok(actions.and_then(|actions| {
        actions.iter().find_map(|action| {
            action
                .as_mapping()?
                .get(serde_yaml::Value::String("transition_to".to_string()))?
                .as_str()
                .map(str::to_string)
        })
    }))
}

/// Resolve the owner from the effective target while retaining `next`'s
/// warning-plus-`manual` fallback for an invalid configured agent.
// §FS-rhei-next.3.1
fn resolve_claim_assignee(
    claim: bool,
    machine: &rhei_validator::StateMachine,
    state: &str,
    settings: &RheiSettings,
    task: &rhei_core::ast::Task,
) -> Option<String> {
    claim.then(|| {
        resolve_agent_for_task(machine, state, settings, &default_run_options(), task)
            .ok()
            .flatten()
            .map(|resolved| resolved.agent.id().to_string())
            .unwrap_or_else(|| "manual".to_string())
    })
}

/// Persist the target-state view, restoring through the claim transaction on
/// any claim-only failure and leaving ordinary transition behavior unchanged.
// §FS-rhei-next.3.1
#[allow(clippy::too_many_arguments)]
fn persist_transition_state(
    transaction: &mut Option<ClaimTransaction<'_>>,
    metadata_file: &Path,
    task_file: &Path,
    metadata_handle: &LockedPlanFile,
    task_handle: Option<&LockedPlanFile>,
    metadata_updated: &str,
    task_updated: Option<&str>,
) -> MietteResult<()> {
    let write = if let Some(transaction) = transaction.as_mut() {
        transaction.write_state(metadata_updated, task_updated)
    } else {
        write_file_atomic_locked(metadata_file, metadata_updated, Some(metadata_handle)).and_then(
            |()| match task_updated {
                Some(task_updated) => {
                    write_file_atomic_locked(task_file, task_updated, task_handle)
                }
                None => Ok(()),
            },
        )
    };
    match (write, transaction.as_mut()) {
        (Err(error), Some(transaction)) => Err(transaction.rollback(error).error),
        (result, _) => result,
    }
}

/// Add ownership and the one serialized ledger line that commits a claim.
// §FS-rhei-next.3.1
#[allow(clippy::too_many_arguments)]
fn commit_claim(
    transaction: Option<&mut ClaimTransaction<'_>>,
    assignee: Option<&str>,
    metadata_updated: &str,
    task_updated: Option<&str>,
    local_id: &str,
    from: &str,
    to: &str,
) -> MietteResult<()> {
    let Some(transaction) = transaction else { return Ok(()) };
    let assignee = assignee.expect("claim assignee");
    if let Err(error) =
        transaction.write_assignee(metadata_updated, task_updated, local_id, assignee)
    {
        return Err(transaction.rollback(error).error);
    }
    if let Err(error) = transaction.commit(from, to) {
        return Err(transaction.rollback(error).error);
    }
    Ok(())
}

/// Finish a failed claim enter: restore first, then apply its declared recovery
/// transition from the source state after releasing the claim's locks.
// §FS-rhei-next.3.1 §FS-rhei-transitions.4.9
#[allow(clippy::too_many_arguments)]
fn finish_failed_claim_enter(
    transaction: &mut Option<ClaimTransaction<'_>>,
    original: Report,
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    no_callbacks: bool,
    task_handle: Option<&LockedPlanFile>,
    metadata_handle: &LockedPlanFile,
) -> Report {
    let rollback = transaction.as_mut().expect("claim transaction").rollback(original);
    if !rollback.restored {
        return rollback.error;
    }
    let recovery = match configured_claim_recovery_target(callback_paths.state_machine_path.as_deref()) {
        Ok(recovery) => recovery,
        Err(policy_error) => {
            return miette!(
                help = state_machine_help(),
                "{}; reading the configured on-enter recovery also failed: {policy_error}",
                rollback.error
            );
        }
    };
    let Some(recovery) = recovery else { return rollback.error };

    // Restoration completes while all claim locks are held. The declared
    // recovery is then a separate ordinary transition with its own state and
    // ledger bookkeeping, never residue of the unsuccessful claim.
    transaction.take();
    if let Some(task_handle) = task_handle {
        task_handle.release();
    }
    metadata_handle.release();
    match execute_transition(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        &recovery,
        None,
        no_callbacks,
    ) {
        Ok(recovered_to) => miette!(
            help = callback_command_help(),
            "{}; configured on-enter recovery transitioned Task {} from '{}' to '{}'",
            rollback.error,
            files.artifact_id,
            from,
            recovered_to
        ),
        Err(recovery_error) => miette!(
            help = callback_command_help(),
            "{}; configured on-enter recovery to '{}' also failed: {recovery_error}",
            rollback.error,
            recovery
        ),
    }
}
