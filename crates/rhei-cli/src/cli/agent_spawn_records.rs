// What `rhei run` knows about a worker it already spawned: that one ran at all,
// which visit of the state it belonged to, which attempt of that visit it was,
// and how it ended.
//
// Its own part because none of that is derivable from `runtime/logs/`. A log is
// opened before its subprocess starts, so its existence proves only that a
// spawn was attempted; and its name carries `{visit_count}`, which is pinned at
// 1 for every ordinary state in a cycle, so it cannot tell one stay in a state
// from the next. Both questions used to be answered by pattern-matching file
// names, and both answers were wrong.

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
    /// Exit `0` is the one ending that has to be inferred rather than read: the
    /// scheduler re-spawns an invocation only when its completion condition is
    /// still unmet, so an attempt that exited cleanly and is being retried is an
    /// attempt whose artifacts never answered for it.
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
            (_, Some(0)) | (_, None) => {
                "exited 0 without meeting this state's completion condition".to_string()
            }
            ("provider_limited", _) => "was provider-limited".to_string(),
            (_, Some(code)) => format!("exited {code}"),
        }
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
    let Some(mut stored) = read_spawn_record(record) else { return };
    stored.charged = stored.charged.saturating_sub(1);
    stored.ending = WITHHELD_ENDING.to_string();
    if let Ok(body) = serde_json::to_string_pretty(&stored) {
        let _ = fs::write(record, body);
    }
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
}

impl SpawnPlan {
    /// The budget spent, if this visit's is already gone and the spawn must
    /// not happen at all. A poll state's own bound never spends here — its
    /// exhaustion is `poll.max_attempts`, checked elsewhere. §FS-rhei-agents.3.2.3
    fn budget_spent(&self, budget: AttemptBudget) -> Option<u64> {
        match budget {
            AttemptBudget::Visit(budget) if self.charged >= budget => Some(budget),
            AttemptBudget::Visit(_) | AttemptBudget::Poll { .. } => None,
        }
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
            AttemptBudget::Visit(budget) => budget.to_string(),
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
        if self.charged.saturating_add(1) < budget {
            RetryOutlook::AttemptsLeft
        } else {
            RetryOutlook::BudgetSpent { budget }
        }
    }

    /// Record this spawn, now that it has ended.
    ///
    /// Called from the two places a subprocess is waited on, after the footer
    /// its log gets, and from nowhere a spawn can fail to start — the record's
    /// whole value is that its presence proves a worker ran.
    // §FS-rhei-agents.8.4
    fn record_spawn(&self, ended: SpawnEnding<'_>) {
        if let Some(parent) = self.record.parent() {
            if fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let attempt_charged = !matches!(ended.ending, "interrupted" | "provider_limited");
        let record = SpawnRecord {
            task: ended.task_id.to_string(),
            state: ended.state_name.to_string(),
            moves: self.moves,
            attempt: self.attempt,
            // An interrupted invocation is not an attempt the ticket spent: the
            // run ended it and the next one re-executes it. It keeps its
            // attempt log all the same. §FS-rhei-run.3.2 §FS-rhei-agents.3.2.3
            charged: self.charged + u64::from(attempt_charged),
            attempt_charged,
            kind: ended.kind.to_string(),
            worker: ended.worker.to_string(),
            log: self.log.clone(),
            started: ended.started.to_string(),
            ended: ended.ended.to_string(),
            duration: ended.duration.to_string(),
            code: ended.code,
            ending: ended.ending.to_string(),
        };
        if let Ok(body) = serde_json::to_string_pretty(&record) {
            let _ = fs::write(&self.record, body);
        }
    }
}

/// The facts a finished spawn records about itself. §FS-rhei-agents.8.4
struct SpawnEnding<'a> {
    task_id: &'a str,
    state_name: &'a str,
    kind: &'a str,
    worker: &'a str,
    started: &'a str,
    ended: &'a str,
    duration: &'a str,
    code: Option<i32>,
    ending: &'a str,
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

/// The halt an exhausted budget prints, wherever it is printed from.
///
/// Two moments reach it: the pass that declines to spawn because the budget is
/// gone, and the attempt that spent the last of it, which knows one run earlier
/// that no later pass will run the state again. One function so the operator
/// reads one sentence rather than two that disagree about what happens next.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
fn budget_spent_halt_line(task_id: &str, state_name: &str, budget: u64, owed: &str) -> String {
    format!(
        "  halting Task {task_id} in state '{state_name}': {budget} attempts spent on this \
         visit and the completion condition is still unmet: {owed}. The ticket stays in \
         '{state_name}'."
    )
}

/// What the run will do about a ticket whose attempt just stalled.
///
/// The stall message predicts engine behaviour, so it has to be conditioned on
/// the thing that decides that behaviour. Conditioning it on the completion
/// condition alone made the run promise a retry it had already ruled out — the
/// same class of untruth as the result stub that said no agent had run.
// §FS-rhei-agents.3.2.1
#[derive(Clone, Copy)]
enum RetryOutlook {
    /// A later pass will spawn this invocation again.
    AttemptsLeft,
    /// The attempt that just finished was the last one this visit gets.
    BudgetSpent { budget: u64 },
}

impl RetryOutlook {
    /// The halt line for this outlook, given what the completion condition is
    /// still owed. §FS-rhei-agents.3.2.1
    fn halt_line(self, task_id: &str, state_name: &str, missing: &[String]) -> String {
        match self {
            RetryOutlook::AttemptsLeft => format!(
                "  halting Task {task_id} in state '{state_name}': the completion condition is \
                 not met, so no transition fires; a later pass runs the state again."
            ),
            RetryOutlook::BudgetSpent { budget } => budget_spent_halt_line(
                task_id,
                state_name,
                budget,
                &completion_debt_label(missing),
            ),
        }
    }
}

/// Which attempt of which visit the next spawn of this invocation is.
///
/// One rule, one place: the scheduler asks it to name the log and to check the
/// budget, and prompt composition asks it to tell the invocation it is a retry.
/// Answering it twice, differently, is how the log names and the run's narration
/// came apart in the first place.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4 §FS-rhei-memory.4.4
fn plan_spawn_attempt(
    runtime_dir: &Path,
    task_root: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
) -> SpawnPlan {
    let record_path = spawn_record_path(runtime_dir, task_id, state_name, suffix);
    let moves = ticket_move_count(task_root, runtime_dir, task_id);
    let previous = read_spawn_record(&record_path).filter(|record| record.moves == moves);
    let attempt = previous.as_ref().map(|record| record.attempt + 1).unwrap_or(1);
    let charged = previous.as_ref().map(|record| record.charged).unwrap_or(0);
    SpawnPlan {
        log: agent_log_attempt_path(runtime_dir, task_id, state_name, suffix, attempt),
        record: record_path,
        moves,
        attempt,
        accounting: None,
        charged,
        previous,
    }
}

/// Plan an agent process and create its accounting identity before it can be
/// spawned. Programs use [`plan_spawn_attempt`] directly because they do not
/// produce agent-accounting records. §FS-rhei-cost-accounting.3.7
#[allow(clippy::too_many_arguments)]
fn plan_agent_spawn_attempt(
    runtime_dir: &Path,
    task_root: &Path,
    task_id: &str,
    state_name: &str,
    suffix: Option<&str>,
    resolved: &ResolvedAgent,
    visit: u64,
    run_id: &str,
) -> SpawnPlan {
    let mut plan = plan_spawn_attempt(runtime_dir, task_root, task_id, state_name, suffix);
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
    plan
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

/// Which rule bounds one visit's spawns, and by how much. `Poll` is carried
/// only to be named in the respawn note, never as a second bound alongside
/// the poll's own exhaustion rule.
// §FS-rhei-agents.3.2.1 §FS-rhei-agents.3.2.3
#[derive(Clone, Copy)]
enum AttemptBudget {
    /// The state's own `attempts:`, `defaults.attempts`, or the built-in.
    /// `SpawnPlan::budget_spent` and `SpawnPlan::retry_outlook` bound spawns
    /// at this count.
    Visit(u64),
    /// A poll state's `poll.max_attempts`. Never bounds a spawn — the poll's
    /// own exhaustion path (`ready_auto_advance.rs`) does that.
    Poll { max_attempts: u64 },
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
    if let Some(max_attempts) = state_def.and_then(|def| def.poll.as_ref()) {
        return AttemptBudget::Poll { max_attempts: u64::from(max_attempts.max_attempts) };
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
    }
}
