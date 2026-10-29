// What `rhei run` knows about a worker it already spawned: that one ran at all,
// which visit of the state it belonged to, which attempt of that visit it was,
// and how it ended.
//
// Its own part because none of that is derivable from `runtime/logs/`. A log is
// opened before its subprocess starts, so its existence proves only that a
// spawn was attempted; and its name spells an allocation — which entry it is
// comes from the movement history, not from the name. Both questions used to be
// answered by pattern-matching file names, and both answers were wrong.

// §AR-source-file-size.3 §FS-rhei-agents.8.4 §FS-rhei-run.3

/// A visit gets at least the one invocation that makes it a visit, and by
/// default one informed retry after it. §FS-rhei-agents.3.2.3
const DEFAULT_ATTEMPT_BUDGET: u64 = 2;

/// One worker spawn that actually ran, as it is left on disk.
///
/// `moves` is the visit key: the number of transitions the ticket had already
/// made when this spawn started. It changes the moment the ticket moves — a hop,
/// a self-loop, a hand `rhei transition` — and does not change while the ticket
/// stalls in place, which is exactly the distinction `{visit_count}` cannot
/// draw. `task` and `state` are stored so a reader can match them as *fields*:
/// matching record file names by prefix is how state `review` came to claim
/// `review-fix`'s worker, log, and duration.
// §FS-rhei-agents.8.4
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SpawnRecord {
    task: String,
    state: String,
    moves: u64,
    attempt: u64,
    /// How many attempts of this visit have been charged against its budget.
    /// An invocation the run itself interrupted is not one of them: the
    /// shutdown ended it, and the next run re-executes it. Defaulted so a
    /// record written before the budget existed still reads.
    // §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3
    #[serde(default)]
    charged: u64,
    /// Audited operator allowances for this visit. §FS-rhei-agents.8.4
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    grants: Vec<AttemptGrant>,
    /// Whether this invocation itself spent the visit budget. Kept beside the
    /// cumulative counter so old records still drive retries correctly.
    // §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4
    #[serde(default = "default_attempt_charged")]
    attempt_charged: bool,
    /// `agent` or `program`, so the account of a state says which kind ran
    /// rather than assuming the one the state would resolve to today.
    kind: String,
    /// The resolved agent id, or the program's command line.
    worker: String,
    log: PathBuf,
    started: String,
    ended: String,
    duration: String,
    code: Option<i32>,
    /// Why the spawn stopped: `exited`, `timed out`, or `interrupted`. A retry
    /// reports the ending it is retrying, and these are different rules.
    // §FS-rhei-agents.3.2.1
    ending: String,
    /// The edit the run reverted after this attempt, when its edit broke the plan.
    // §FS-rhei-run.3.7.4 §FS-rhei-memory.4.4
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reverted: Option<RevertedEdit>,
}

fn default_attempt_charged() -> bool {
    true
}

/// The `ending` of an attempt whose edge §FS-rhei-supervision.3.6 took away.
/// Its own ending because the invocation neither failed nor left its state
/// owing an artifact, and both of the other exit-0 sentences say it did.
// §FS-rhei-agents.3.2.3 §FS-rhei-supervision.3.6
const WITHHELD_ENDING: &str = "withheld";

impl SpawnRecord {
    /// Whether this invocation successfully finished work in `moves`, the
    /// ticket's current visit.
    ///
    /// `withheld` is the engine's durable spelling of an `exited`/`0` record
    /// whose already-satisfied release edge was withheld; it remains successful
    /// work when the ticket has not moved. §FS-rhei-supervision.3.6
    // §FS-rhei-agents.3.2 §FS-rhei-agents.8.4
    fn proves_successful_work(&self, task_id: &str, state_name: &str, moves: u64) -> bool {
        self.task == task_id
            && self.state == state_name
            && self.moves == moves
            && (self.ending == "exited" || self.ending == WITHHELD_ENDING)
            && self.code == Some(0)
    }

    /// How the previous attempt ended, as the retry note and the retried
    /// prompt both say it.
    ///
    /// Read from the record, never inferred: the `ending` decides before the
    /// `code`, so a provider-limited record reads as one whatever its code, and
    /// a record with no `code` was ended rather than exited and is never
    /// `exited 0`. Whether the completion condition was met is not the
    /// record's to say — an attempt that wrote everything is re-spawned too —
    /// so the sentence says nothing about it (§FS-rhei-agents.3.2.1).
    // §FS-rhei-agents.3.2 §FS-rhei-agents.3.2.1
    fn ending_sentence(&self) -> String {
        match (self.ending.as_str(), self.code) {
            ("timed out", _) => format!("timed out after {}", self.duration),
            ("interrupted", _) => "was interrupted by a run shutdown".to_string(),
            (WITHHELD_ENDING, _) => {
                "exited 0 and met its completion condition, and the engine withheld its release \
                 edge because the visit released nothing"
                    .to_string()
            }
            ("provider_limited", _) => "was provider-limited".to_string(),
            (_, None) => "ended without an exit code (a signal ended it)".to_string(),
            (_, Some(code)) => format!("exited {code}"),
        }
    }

    /// Whether the attempt exited `0` on its own: the one ending after which
    /// a retry may say the completion condition went unmet, when the owed
    /// clause beside it names what is unmet (§FS-rhei-memory.4.4).
    // §FS-rhei-memory.4.4
    fn exited_zero(&self) -> bool {
        self.ending == "exited" && self.code == Some(0)
    }
}

/// Give back the attempt an engine-withheld visit was charged, and say in the
/// record that the edge, not the worker, is why the state has not moved.
///
/// The budget bounds a worker that could not answer for itself
/// (§FS-rhei-agents.3.2.3). This one answered — exit `0`, completion condition
/// met — and §FS-rhei-supervision.3.6 took its self-loop away. Charging it
/// would run the budget out on the engine's own decision, and a supervisor
/// bars its subtree while it is held, so that budget running out would leave
/// the workspace recoverable only by `rhei reset`.
///
/// Written where the record already is: the visit is judged after the exit,
/// which is after the spawn recorded itself. A record that will not be read or
/// written is left alone — an attempt given back is a courtesy, never a thing
/// the run depends on.
// §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4 §FS-rhei-supervision.3.6
fn uncharge_withheld_visit(record: &Path) {
    // §FS-rhei-agents.8.4: giving back an attempt preserves a concurrent grant.
    let _ = update_spawn_record(record, |stored| {
        let Some(mut stored) = stored else { return Ok(None) };
        stored.charged = stored.charged.saturating_sub(1);
        stored.ending = WITHHELD_ENDING.to_string();
        Ok(Some(stored))
    });
}

/// `runtime/spawns/` — beside `runtime/logs/`, and swept with it by `rhei
/// reset`, because it answers for the same invocations.
// §FS-rhei-agents.8.4
fn spawn_records_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("spawns")
}

/// The record of one invocation, named as its log is minus the attempt suffix:
/// one file per invocation, rewritten by each attempt, so reading the current
/// attempt count costs one `open` rather than a walk over every name a retry
/// might have taken.
// §FS-rhei-agents.8.4
fn spawn_record_path(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
) -> PathBuf {
    let suffix = suffix
        .filter(|value| !value.is_empty())
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    spawn_records_dir(runtime_dir).join(format!("task-{task_id}-{state_name}{suffix}.json"))
}

fn read_spawn_record(path: &Path) -> Option<SpawnRecord> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// How many times this ticket has moved, from the central ledger every verb
/// appends to.
///
/// Both candidate ledgers are counted and summed. A single-file plan has one:
/// the ticket's owning root and the run's runtime directory are the same place.
/// A Panta project can route a ticket's moves to its owning rhei's root while
/// the run's logs go to the project's, and this question must be answered the
/// same either way. Summing is safe because the answer is only ever compared
/// with itself: both counts grow when the ticket moves and neither moves while
/// it stalls, which is the whole property a visit key needs.
///
/// A missing or unreadable ledger reads as zero, which makes a fresh ticket's
/// first visit look exactly like what it is.
// §FS-rhei-agents.8.4 §FS-rhei-viz.4 §FS-rhei-panta.6.2
fn ticket_move_count(task_root: &Path, runtime_dir: &Path, task_id: &str) -> u64 {
    let owning = task_root.join("runtime").join("state-transitions.log");
    let running = runtime_dir.join("state-transitions.log");
    let prefix = format!("{task_id} ");
    let lines_in = |path: &Path| -> u64 {
        fs::read_to_string(path)
            .map(|raw| raw.lines().filter(|line| line.starts_with(&prefix)).count() as u64)
            .unwrap_or(0)
    };
    let mut moves = lines_in(&owning);
    if running != owning {
        moves += lines_in(&running);
    }
    moves
}

/// What the next spawn of one invocation is: where it writes, which attempt of
/// which visit it is, and what the attempt before it left behind.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
struct SpawnPlan {
    /// The transcript this spawn opens.
    log: PathBuf,
    /// Where its record goes once it has actually run.
    record: PathBuf,
    /// The visit this spawn belongs to. §FS-rhei-agents.8.4
    moves: u64,
    /// 1 for the first spawn of this visit.
    attempt: u64,
    /// One accounting identity for this agent process, created with the same
    /// persisted visit and attempt facts that name its log and budget charge.
    // §FS-rhei-cost-accounting.3.7
    accounting: Option<AccountingAttemptIdentity>,
    /// What this visit has already spent of its budget. §FS-rhei-agents.3.2.3
    charged: u64,
    /// The previous attempt *of this same visit*, when there was one. A record
    /// left by an earlier visit is not one: re-entering a state is a fresh
    /// start, not a second attempt at the last one.
    previous: Option<SpawnRecord>,
    /// The state's part of the configuration an agent spawn consumes; `null`
    /// for a program. §FS-rhei-evidence.6
    evidence_state: serde_json::Value,
}

impl SpawnPlan {
    /// The budget spent, if this visit's is already gone and the spawn must
    /// not happen at all. A poll state's ordinary exhaustion is its own edge,
    /// checked elsewhere; only an attempt whose edit the run reverted, which
    /// fires no edge, can leave `poll.max_attempts` spent here.
    // §FS-rhei-agents.3.2.3 §FS-rhei-run.3.7.4
    fn budget_spent(&self, budget: AttemptBudget) -> Option<u64> {
        let last_reverted = self.previous.as_ref().is_some_and(|prev| prev.reverted.is_some());
        self.effective_budget(budget).spent(self.charged, last_reverted)
    }

    /// The line the run prints beside `Log:` when it is retrying rather than
    /// starting, naming the attempt, the budget it comes out of, what ended the
    /// attempt before it, and where that attempt's transcript is.
    // §FS-rhei-agents.3.2.1
    fn respawn_note(
        &self,
        task_id: &str,
        state_name: &str,
        budget: AttemptBudget,
    ) -> Option<String> {
        let previous = self.previous.as_ref()?;
        // A withheld visit is unspent and keeps its attempts, so this is not a
        // retry of it — it is the same visit again. §FS-rhei-supervision.3.6
        if previous.ending == WITHHELD_ENDING {
            return Some(format!(
                "  Visiting Task {task_id} in state '{state_name}' again: the previous visit {} \
                 (previous log: {}).",
                previous.ending_sentence(),
                previous.log.display()
            ));
        }
        let budget = match budget {
            AttemptBudget::Visit(budget) => self.effective_allowance(budget).to_string(),
            AttemptBudget::Poll { max_attempts } => format!("{max_attempts} (poll.max_attempts)"),
        };
        Some(format!(
            "  Re-spawning Task {task_id} in state '{state_name}': attempt {} of {budget}; \
             the previous attempt {} (previous log: {}).",
            self.charged + 1,
            previous.ending_sentence(),
            previous.log.display()
        ))
    }

    /// Whether the attempt about to run leaves another one behind it.
    ///
    /// Asked *before* the spawn and answered about the state of the visit after
    /// it, because the message that needs it is printed when that spawn has
    /// already finished. An interrupted spawn never reaches that message, so the
    /// charge this predicts is the charge that happens. A poll state's own
    /// exhaustion rule is its only bound, so it always has attempts left here.
    // §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
    fn retry_outlook(&self, budget: AttemptBudget) -> RetryOutlook {
        let AttemptBudget::Visit(budget) = budget else {
            return RetryOutlook::AttemptsLeft;
        };
        let configured = budget;
        let budget = self.effective_allowance(budget);
        if self.charged.saturating_add(1) < budget {
            RetryOutlook::AttemptsLeft
        } else if configured == budget {
            RetryOutlook::BudgetSpent { budget }
        } else {
            RetryOutlook::BudgetSpentWithRecovery {
                budget,
                recovery: HaltRecovery::Visit { configured, checkpoints: None },
            }
        }
    }
}

/// What the completion condition still owes, as the halt line names it.
///
/// The same list the exit-0 stall warning prints, so an operator reading
/// "attempts spent" and an operator reading "outputs are missing" are looking at
/// the same artifacts. An empty list is said plainly rather than skipped: it
/// means the condition that failed named no file, and hiding that would leave
/// the halt looking like it had forgotten to say what it was waiting for.
// §FS-rhei-agents.3.2 §FS-rhei-agents.3.2.3
fn completion_debt_label(missing: &[String]) -> String {
    if missing.is_empty() {
        "this state's completion condition names nothing on disk".to_string()
    } else {
        missing.join(", ")
    }
}


/// Give an agent process's plan its accounting identity before it can be
/// spawned. Programs skip this because they do not produce agent-accounting
/// records. §FS-rhei-cost-accounting.3.7
fn attach_agent_accounting(
    plan: &mut SpawnPlan,
    task_id: &str,
    state_name: &str,
    resolved: &ResolvedAgent,
    visit: u64,
    run_id: &str,
) {
    // §FS-rhei-cost-accounting.3.7: the caller's run identity survives descriptor publication failure.
    plan.accounting = Some(AccountingAttemptIdentity {
        invocation_id: accounting_attempt_invocation_id(
            task_id,
            state_name,
            resolved,
            visit,
            run_id,
            plan.moves,
            plan.attempt,
        ),
        run_id: Some(run_id.to_string()),
    });
}

/// The most recent worker that actually ran in this state on this ticket, of
/// whatever identity or visit — or `None` when none did.
///
/// Asked where the engine is about to speak for a state it did not spawn a
/// worker in, so it holds no resolved identity to key a record by and must look
/// for one. Matching is on the record's `task` and `state` fields: a name-prefix
/// match would let `review` answer with `review-fix`'s worker.
// §FS-rhei-agents.8.4 §FS-rhei-run.3
fn newest_spawn_record_for_state(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
) -> Option<SpawnRecord> {
    let mut newest: Option<(String, SpawnRecord)> = None;
    for entry in fs::read_dir(spawn_records_dir(runtime_dir)).ok()?.flatten() {
        // §FS-rhei-agents.8.4: staging files are not published records.
        if entry.path().extension() != Some(OsStr::new("json")) { continue; }
        // One unreadable or half-written record must not discard the matches
        // already found; it is one file, not an answer about the state.
        let Some(record) = read_spawn_record(&entry.path()) else { continue };
        if record.task != task_id || record.state != state_name {
            continue;
        }
        if newest.as_ref().is_none_or(|(seen, _)| record.ended >= *seen) {
            newest = Some((record.ended.clone(), record));
        }
    }
    newest.map(|(_, record)| record)
}

/// Every record a worker left in this state on this ticket, of whatever
/// identity or visit, beside the path it was read from, in path order.
///
/// Asked when an invocation has no successful record of its own and the
/// visit's finished work may sit under a name the plan no longer spells.
/// Matching is on the record's `task` and `state` fields, as in
/// [`newest_spawn_record_for_state`]: a name-prefix match would hand state
/// `review` the records of `review-fix`.
// §FS-rhei-agents.8.4 §FS-rhei-agent-visit-pairing.3
fn spawn_records_for_state(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
) -> Vec<(PathBuf, SpawnRecord)> {
    let Ok(entries) = fs::read_dir(spawn_records_dir(runtime_dir)) else { return Vec::new() };
    let mut records = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            // §FS-rhei-agents.8.4: ignore a complete but unpublished atomic-write staging file.
            if path.extension() != Some(OsStr::new("json")) { return None; }
            let record = read_spawn_record(&path)?;
            (record.task == task_id && record.state == state_name).then_some((path, record))
        })
        .collect::<Vec<_>>();
    records.sort_by(|(left, _), (right, _)| left.cmp(right));
    records
}

/// Which rule bounds one visit's spawns, and by how much. `Poll` is never a
/// second bound alongside the poll's own exhaustion rule: it bounds only the
/// attempts whose edits were reverted, which that rule never sees.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
#[derive(Clone, Copy)]
enum AttemptBudget {
    /// The state's own `attempts:`, `defaults.attempts`, or the built-in.
    /// `SpawnPlan::budget_spent` and `SpawnPlan::retry_outlook` bound spawns
    /// at this count.
    Visit(u64),
    /// A poll state's `poll.max_attempts`. The poll's own exhaustion path
    /// (`ready_auto_advance.rs`) bounds its ordinary attempts; a visit whose
    /// last attempt was reverted stalls here once it reaches the bound.
    // §FS-rhei-run.3.7.4
    Poll { max_attempts: u64 },
}

impl AttemptBudget {
    /// The bound a visit has spent, if it has, once `charged` of its attempts
    /// are charged and its last one was reverted or not: a state's own budget
    /// at its count, a poll's only when that last edit was reverted too.
    ///
    /// The one copy of the rule. The spawner refuses on it, a reverted
    /// attempt's charge and halt read it, and the deadline scan asks the
    /// spawner, so none of them can call a visit spent that another would
    /// still spawn. §FS-rhei-agents.3.2.3 §FS-rhei-run.3.7.4 §FS-rhei-run.5.1
    fn spent(self, charged: u64, last_reverted: bool) -> Option<u64> {
        match self {
            AttemptBudget::Visit(budget) if charged >= budget => Some(budget),
            AttemptBudget::Poll { max_attempts } if last_reverted && charged >= max_attempts => {
                Some(max_attempts)
            }
            AttemptBudget::Visit(_) | AttemptBudget::Poll { .. } => None,
        }
    }
}

/// How many spawns one visit to this state may have.
///
/// The chain a timeout resolves through, one level shorter because a budget has
/// no per-agent meaning: the state's own `attempts:`, then `defaults.attempts`,
/// then the built-in. Below `1` is raised to `1` — every visit gets the
/// invocation that makes it a visit.
///
/// A poll state is exempt. Re-spawning without moving *is* what a poll state
/// does, and it already carries its own bound in `poll.max_attempts`; a second
/// bound over the same spawns would stop the loop before its own cap and
/// silently change what the machine's author declared.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3 §FS-rhei-agents.7.1 §FS-rhei-states.2
fn resolve_attempt_budget(
    state_def: Option<&rhei_validator::StateDef>,
    settings: &RheiSettings,
) -> AttemptBudget {
    if let Some(budget) = state_def.and_then(poll_attempt_budget) {
        return budget;
    }
    AttemptBudget::Visit(
        state_def
            .and_then(|def| def.attempts)
            .or(settings.defaults.attempts)
            .map(u64::from)
            .unwrap_or(DEFAULT_ATTEMPT_BUDGET)
            .max(1),
    )
}

/// A `poll:` state's budget, its own `poll.max_attempts`: no setting reaches
/// it, so a reader asking only about a poll's bound need load none.
// §FS-rhei-agents.3.2.3 §FS-rhei-run.3.7.4
fn poll_attempt_budget(state_def: &rhei_validator::StateDef) -> Option<AttemptBudget> {
    let poll = state_def.poll.as_ref()?;
    Some(AttemptBudget::Poll { max_attempts: u64::from(poll.max_attempts) })
}

/// A plan for a unit test that only cares about the transcript a spawn writes:
/// the first attempt of a first visit, recording beside the log it is given.
#[cfg(test)]
fn spawn_plan_for_test(log: &Path) -> SpawnPlan {
    SpawnPlan {
        log: log.to_path_buf(),
        record: log.with_extension("spawn.json"),
        moves: 0,
        attempt: 1,
        accounting: Some(AccountingAttemptIdentity {
            invocation_id: "test::pending::mock::visit-1::run-test::move-0::attempt-1".into(),
            run_id: Some("test".into()),
        }),
        charged: 0,
        previous: None,
        evidence_state: serde_json::Value::Null,
    }
}
