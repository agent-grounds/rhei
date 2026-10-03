// The agent invocations a reading knows were spawned and that no accounting
// record answers for.
//
// A record is written by the invocation it measures, so a reading made of
// records alone cannot see one that left none: a profile with no family, a
// record write that failed, an extractor that never ran. `rhei run` writes a
// spawn record for every worker it spawned, so the reading reconciles its
// records against those before it says what its coverage is. The check runs
// one way only: it can find that something is missing, never that nothing is.

// §FS-rhei-cost-accounting.6.2.1 §FS-rhei-agents.8.4

/// The spawn-record fields the reconciliation reads, and nothing else, so a
/// field the runtime adds later never makes an older record unreadable here.
/// §FS-rhei-agents.8.4
#[derive(Clone, Debug, serde::Deserialize)]
struct SpawnRecordFacts {
    task: String,
    state: String,
    moves: u64,
    attempt: u64,
    kind: String,
    worker: String,
    started: String,
    ended: String,
}

/// One agent spawn no record in the reading matches. It carries no token, cost
/// or model: nothing is estimated for it. §FS-rhei-cost-accounting.6.2.1
#[derive(Clone, Debug)]
struct UnrecordedSpawn {
    task: String,
    state: String,
    worker: String,
    started: String,
    ended: String,
}

impl UnrecordedSpawn {
    /// Where the window axis places it. §FS-rhei-cost-accounting.6.2.1
    fn started_secs(&self) -> Option<i64> {
        epoch_secs(rhei_tui::parse_rfc3339(&self.started)?)
    }

    /// `ended - started`, or nothing when either will not parse.
    /// §FS-rhei-summary.2.2
    fn elapsed_ms(&self) -> Option<u64> {
        let started = rhei_tui::parse_rfc3339(&self.started)?;
        let ended = rhei_tui::parse_rfc3339(&self.ended)?;
        ended.duration_since(started).ok().map(|elapsed| elapsed.as_millis() as u64)
    }

    /// Whether `--by` places it in the group keyed `key`. Groups are formed
    /// from records; `model` and `run` name nothing a spawn carries.
    /// §FS-rhei-cost-accounting.6.2.1
    fn in_group(&self, by: CostGroup, key: &str) -> bool {
        match by {
            CostGroup::Node => self.task == key,
            CostGroup::State => self.state == key,
            CostGroup::Agent => self.worker == key,
            CostGroup::Day => {
                self.started_secs().and_then(|_| self.started.get(..10)) == Some(key)
            }
            CostGroup::Model | CostGroup::Run => false,
        }
    }

    /// The plan-tree axis places it by `task`. §FS-rhei-cost-accounting.6.2.1
    fn in_subtree(&self, task_id: &str) -> bool {
        self.task == task_id || is_descendant_id(&self.task, task_id)
    }
}

/// The agent spawn records beside one accounting root, as `(path, facts)`, and
/// the ones that would not read.
///
/// A program spawn is never returned, because a program writes no invocation
/// record. An absent `runtime/spawns/` holds nothing to reconcile.
/// §FS-rhei-cost-accounting.6.2.1
fn read_agent_spawns(accounting_root: &Path) -> (Vec<SpawnRecordFacts>, Vec<String>) {
    let mut spawns = Vec::new();
    let mut errors = Vec::new();
    let Some(runtime_dir) = accounting_root.parent() else { return (spawns, errors) };
    let dir = spawn_records_dir(runtime_dir);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return (spawns, errors),
        Err(err) => {
            errors.push(format!("{}: {err}", dir.display()));
            return (spawns, errors);
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("json") {
            continue;
        }
        // Reported the way a malformed accounting artifact is, and skipped.
        // §FS-rhei-cost-accounting.11
        match fs::read_to_string(&path).map_err(|err| err.to_string()).and_then(|text| {
            serde_json::from_str::<SpawnRecordFacts>(&text).map_err(|err| err.to_string())
        }) {
            Ok(spawn) if spawn.kind == "agent" => spawns.push(spawn),
            Ok(_) => {}
            Err(err) => errors.push(format!("{}: {err}", path.display())),
        }
    }
    (spawns, errors)
}

/// The spawns no record answers for, in `started` order.
///
/// A record answers for a spawn when it names the same task and state and its
/// attempt identity names the spawn's `moves` and `attempt`; a legacy record,
/// whose identity carries neither, answers for one whose `[started, ended]`
/// holds its `started_at`. Matched as fields, never by file names or counts.
/// §FS-rhei-cost-accounting.6.2.1 §FS-rhei-cost-accounting.3.7
fn unrecorded_agent_spawns(
    spawns: Vec<SpawnRecordFacts>,
    records: &[InspectedRecord],
) -> Vec<UnrecordedSpawn> {
    let mut by_step: HashMap<(&str, &str), Vec<&AccountingInvocationRecord>> = HashMap::new();
    for held in records {
        by_step
            .entry((held.record.task_id.as_str(), held.record.state.as_str()))
            .or_default()
            .push(&held.record);
    }
    let mut unrecorded: Vec<UnrecordedSpawn> = spawns
        .into_iter()
        .filter(|spawn| {
            !by_step
                .get(&(spawn.task.as_str(), spawn.state.as_str()))
                .is_some_and(|step| step.iter().any(|record| record_answers_for(record, spawn)))
        })
        .map(|spawn| UnrecordedSpawn {
            task: spawn.task,
            state: spawn.state,
            worker: spawn.worker,
            started: spawn.started,
            ended: spawn.ended,
        })
        .collect();
    // One whose `started` will not read goes last, where no merge waits on it.
    unrecorded.sort_by_key(|spawn| {
        let started = spawn.started_secs();
        (started.is_none(), started, spawn.task.clone(), spawn.state.clone())
    });
    unrecorded
}

/// Whether one record of the spawn's own task and state is the record of it.
/// §FS-rhei-cost-accounting.6.2.1
fn record_answers_for(record: &AccountingInvocationRecord, spawn: &SpawnRecordFacts) -> bool {
    if let Some(identity) = invocation_attempt_identity(&record.invocation_id) {
        return identity == (spawn.moves, spawn.attempt);
    }
    // Spawn times are whole seconds, so the record's instant is compared at
    // the same grain.
    let secs = |text: &str| epoch_secs(rhei_tui::parse_rfc3339(text)?);
    match (record_started_at_secs(record), secs(&spawn.started), secs(&spawn.ended)) {
        (Some(at), Some(started), Some(ended)) => started <= at && at <= ended,
        _ => false,
    }
}

/// An aggregate over a selection that holds `unrecorded` spawns: it counts
/// them, and it can no longer say `complete`. `partial`, `unpriced` and `none`
/// stand. §FS-rhei-cost-accounting.6.2.1
fn count_unrecorded(
    mut summary: rhei_tui::AccountingRunSummary,
    unrecorded: usize,
) -> rhei_tui::AccountingRunSummary {
    summary.unrecorded_agent_invocation_count = unrecorded as u64;
    demote_if(summary, unrecorded > 0)
}

/// The coverage word a human reads, with the gap after it when there is one:
/// `Partial (2 of 4 agent invocations have no accounting record)`. With nothing
/// missing it is the word alone, as it always was.
/// §FS-rhei-cost-accounting.6.2.1 §FS-rhei-cost-accounting.8 §FS-rhei-summary.2.3
fn coverage_label(summary: &rhei_tui::AccountingRunSummary) -> String {
    let missing = summary.unrecorded_agent_invocation_count;
    if missing == 0 {
        return format!("{:?}", summary.coverage);
    }
    let known = summary.invocation_count + missing;
    // The spec's own sentence, whatever the counts. §FS-rhei-cost-accounting.6.2.1
    format!(
        "{:?} ({missing} of {known} agent invocations have no accounting record)",
        summary.coverage
    )
}
