// How a spawn goes on past an attempt killed together with its run: the plan
// that folds the killed attempt in, the record a spawn site writes for it, and
// the bound a poll state's kills are held to.
//
// Apart from the classifier because that half only reads, and this is the half
// that writes; a reading of the plan runs the same fold and writes nothing.

// §AR-source-file-size.3 §FS-rhei-agents.8.1.1 §FS-rhei-agents.3.2.3

/// The kills of one visit that spend nothing; every later one is charged, and
/// in a poll state each grant allows as many more as it adds attempts.
/// §FS-rhei-agents.3.2.3
const FREE_KILLS_PER_VISIT: u64 = 1;

/// One invocation's next spawn, as its spawn site and a reading of the plan
/// both name it. §FS-rhei-agents.8.1
struct NamedSpawn<'a> {
    machine: &'a rhei_validator::StateMachine,
    task_root: &'a Path,
    runtime_dir: &'a Path,
    task_id: &'a str,
    state_name: &'a str,
    visit_count: u64,
    identity: Option<&'a str>,
    reset: &'a ResetOffer,
}

/// Why a spawn may not take the name it is planned under. §FS-rhei-agents.8.1
enum PlanRefusal {
    /// The ledger that numbers it cannot be read.
    Ledger(String),
    /// Its log is taken, and this is what the killed runs' records say of it.
    Taken { log: PathBuf, verdict: TakenLog },
    /// The killed attempt's spawn record could not be written: the log, and why.
    Unrecorded { log: PathBuf, error: String },
}

impl PlanRefusal {
    /// The refusal as a run prints it. §FS-rhei-agents.8.1 §FS-rhei-agents.8.1.1
    fn message(&self, task_id: &str, reset: Option<&ResetOffer>, transitions: &[String]) -> String {
        match self {
            PlanRefusal::Ledger(message) => message.clone(),
            PlanRefusal::Taken { log, verdict } => {
                verdict.refusal(log, task_id, reset, transitions)
            }
            PlanRefusal::Unrecorded { log, error } => format!(
                "refusing to spawn: {} is an attempt killed together with its run, and its \
                 spawn record could not be written: {error}\n  help: make runtime/spawns/ \
                 writable, then run again; the visit resumes",
                log.display()
            ),
        }
    }
}

impl NamedSpawn<'_> {
    /// Plan the next spawn, folding in each killed attempt whose log is taken:
    /// a spawn site, which names its `worker`, writes that attempt's record
    /// first; a reading, which passes `None`, predicts the record and writes
    /// nothing. Any other taken log is refused with its case.
    // §FS-rhei-agents.8.1.1 §FS-rhei-evidence.11.1
    fn plan(&self, worker: Option<&str>) -> Result<(SpawnPlan, LogNumber), PlanRefusal> {
        let (task_id, state_name) = (self.task_id, self.state_name);
        let number = resolve_log_number(
            self.machine,
            state_name,
            self.visit_count,
            self.task_root,
            self.runtime_dir,
            task_id,
        )
        .map_err(PlanRefusal::Ledger)?;
        let mut plan = SpawnPlan {
            reset: Some(self.reset.clone()),
            ..plan_spawn_attempt(
                self.runtime_dir,
                self.task_root,
                task_id,
                state_name,
                self.identity,
                number,
            )
        };
        let program = self.machine.states.get(state_name).is_some_and(|def| def.program.is_some());
        while plan.log.exists() {
            let verdict = classify_taken_log(&plan, self.task_root, task_id, state_name, program);
            let TakenLog::Killed { started } = &verdict else {
                return Err(PlanRefusal::Taken { log: plan.log, verdict });
            };
            let killed = match worker {
                Some(worker) => write_killed_record(&plan, started, task_id, state_name, worker)
                    .map_err(|error| PlanRefusal::Unrecorded { log: plan.log.clone(), error })?,
                None => killed_record(&plan, None, started, task_id, state_name, ""),
            };
            let next = plan_following(
                self.runtime_dir,
                task_id,
                state_name,
                self.identity,
                number,
                plan.moves,
                Some(killed),
            );
            // Each fold moves the attempt on, so the loop ends; a record behind
            // the plan accounts for nothing.
            if next.attempt <= plan.attempt {
                return Err(PlanRefusal::Taken { log: plan.log, verdict: TakenLog::Unaccounted });
            }
            plan = SpawnPlan { reset: plan.reset.take(), ..next };
        }
        Ok((plan, number))
    }
}

/// The spawn record of the killed attempt `plan` names, built on `stored`, the
/// record now at its path, or else on the plan's previous attempt: the visit's
/// kills grow by one, the first is free and every later one charged, and grants
/// and the unavailable count carry forward. `ended` is now, when the kill was
/// learned of, and there is neither an exit code nor a duration to say.
// §FS-rhei-agents.8.4 §FS-rhei-agents.3.2.3
fn killed_record(
    plan: &SpawnPlan,
    stored: Option<&SpawnRecord>,
    started: &str,
    task_id: &str,
    state_name: &str,
    worker: &str,
) -> SpawnRecord {
    let base = stored.or(plan.previous.as_ref());
    let killed = base.map_or(0, |base| base.killed) + 1;
    let attempt_charged = killed > FREE_KILLS_PER_VISIT;
    SpawnRecord {
        task: task_id.to_string(),
        state: state_name.to_string(),
        moves: plan.moves,
        attempt: plan.attempt,
        charged: base.map_or(0, |base| base.charged) + u64::from(attempt_charged),
        grants: base.map(|base| base.grants.clone()).unwrap_or_default(),
        attempt_charged,
        kind: "agent".to_string(),
        worker: worker.to_string(),
        log: plan.log.clone(),
        started: started.to_string(),
        ended: format_iso8601_utc(std::time::SystemTime::now()),
        duration: "unknown".to_string(),
        code: None,
        ending: KILLED_ENDING.to_string(),
        unavailable: base.map_or(0, |base| base.unavailable),
        killed,
        signal: None,
        reverted: None,
    }
}

/// Write the killed attempt's spawn record under the lock every record write
/// takes, the one the interrupted path's record goes through, before anything
/// is composed. A record already at that attempt or past it — another run's —
/// is kept as it is. The record now standing, or why none could be written.
// §FS-rhei-agents.8.1.1 §FS-rhei-agents.8.4
fn write_killed_record(
    plan: &SpawnPlan,
    started: &str,
    task_id: &str,
    state_name: &str,
    worker: &str,
) -> Result<SpawnRecord, String> {
    if let Some(parent) = plan.record.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
    }
    let mut kept = None;
    let written = update_spawn_record_with_policy(&plan.record, true, |stored| {
        let stored = stored.filter(|stored| stored.moves == plan.moves);
        if let Some(stored) = stored.as_ref().filter(|stored| stored.attempt >= plan.attempt) {
            kept = Some(stored.clone());
            return Ok(None);
        }
        Ok(Some(killed_record(plan, stored.as_ref(), started, task_id, state_name, worker)))
    })
    .map_err(|err| format!("{err}"))?;
    written.or(kept).ok_or_else(|| format!("{} was not written", plan.record.display()))
}

impl SpawnPlan {
    /// The kills of this visit, when a poll state has had more than it allows:
    /// one free, and one more for each attempt its grants added. Any other
    /// state's kills are bounded by its `attempts:`, through their charges.
    // §FS-rhei-agents.3.2.3 §FS-rhei-retry.2
    fn kills_spent(&self, budget: AttemptBudget) -> Option<u64> {
        let AttemptBudget::Poll { .. } = budget else { return None };
        let previous = self.previous.as_ref()?;
        previous.kills_spent().then_some(previous.killed)
    }
}

impl SpawnRecord {
    /// Whether this visit has been killed more often than a poll state allows.
    /// §FS-rhei-agents.3.2.3
    fn kills_spent(&self) -> bool {
        self.killed > self.allowance(FREE_KILLS_PER_VISIT)
    }
}

/// The halt of a poll visit its kills have spent, naming the grant that allows
/// `configured` more. §FS-rhei-agents.3.2.3 §FS-rhei-retry.2
fn kill_halt_line(task_id: &str, state_name: &str, kills: u64, configured: u64) -> String {
    format!(
        "halting Task {task_id} in state '{state_name}': killed together with its run {kills} \
         times on this visit. The ticket stays in '{state_name}'. To give this visit \
         {configured} more attempts, run: rhei retry {task_id}."
    )
}

/// What a run's spawn site knows beyond the name of the spawn it plans: the
/// ticket and its plan, for the `rhei transition` lines a program's refusal
/// lists, and the settings, for the attempts a poll's kill halt offers.
// §FS-rhei-agents.8.1.1
struct SpawnSite<'a> {
    machine: &'a rhei_validator::StateMachine,
    task: &'a rhei_core::ast::Task,
    loaded: &'a LoadedPlan,
    settings: &'a RheiSettings,
    reset: &'a ResetOffer,
}

impl SpawnSite<'_> {
    /// Plan the next spawn of one invocation as [`plan_named_spawn`] reads it,
    /// and act on what it finds before anything is composed, staged or paid
    /// for: write each killed attempt's record, refuse a taken log with its
    /// case, and halt a poll visit its kills have spent. `worker` is the agent
    /// a killed attempt's record names; a program site passes `""`, since no
    /// program's log is one.
    // §FS-rhei-agents.8.1 §FS-rhei-agents.8.1.1 §FS-rhei-agents.3.2.3
    fn plan(
        &self,
        task_root: &Path,
        runtime_dir: &Path,
        state_name: &str,
        visit_count: u64,
        identity: Option<&str>,
        worker: &str,
    ) -> Result<(SpawnPlan, LogNumber), String> {
        let task_id = &self.task.id.to_string();
        let (machine, reset) = (self.machine, self.reset);
        let named = NamedSpawn {
            machine,
            task_root,
            runtime_dir,
            task_id,
            state_name,
            visit_count,
            identity,
            reset,
        };
        let (plan, number) = named.plan(Some(worker)).map_err(|refusal| {
            let transitions = match &refusal {
                PlanRefusal::Taken { verdict: TakenLog::Program, .. } => {
                    let metadata = self.loaded.rhei.metadata.as_ref();
                    transition_command_lines(self.task, state_name, machine, metadata, &reset.target)
                }
                _ => Vec::new(),
            };
            refusal.message(task_id, Some(reset), &transitions)
        })?;
        let state_def = machine.states.get(state_name);
        if let Some(kills) = plan.kills_spent(resolve_attempt_budget(state_def, self.settings)) {
            let configured = configured_attempts(state_def, self.settings);
            return Err(kill_halt_line(task_id, state_name, kills, configured));
        }
        Ok((plan, number))
    }
}
