// Which name and which attempt the next spawn of one invocation takes, and
// whether it may take it at all: the planning half of the spawn records, which
// reads what they say about earlier attempts and the ledger's entry number,
// and refuses a name no record accounts for before anything is paid for.
//
// Apart from the records because a record is what a spawn leaves behind and a
// plan is what one is about to do; both sides of the refusal live here.

// §AR-source-file-size.3 §FS-rhei-agents.8.1 §FS-rhei-agents.8.4

/// Which attempt of which visit the next spawn of this invocation is.
///
/// One rule, one place: the scheduler asks it to name the log and to check the
/// budget, and prompt composition asks it to tell the invocation it is a retry.
/// Answering it twice, differently, is how the log names and the run's narration
/// came apart in the first place.
///
/// `identity` and `number` are the two parts of the name the grammar puts
/// between the state and the attempt; the attempt is this function's to decide.
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4 §FS-rhei-memory.4.4
fn plan_spawn_attempt(
    runtime_dir: &Path,
    task_root: &Path,
    task_id: &str,
    state_name: &str,
    identity: Option<&str>,
    number: LogNumber,
) -> SpawnPlan {
    let suffix = log_suffix(identity, number.value());
    let record_path = spawn_record_path(runtime_dir, task_id, state_name, suffix.as_deref());
    let moves = ticket_move_count(task_root, runtime_dir, task_id);
    let previous = current_visit_record(runtime_dir, task_id, state_name, identity, number, moves)
        .filter(|record| record.moves == moves);
    let attempt = previous.as_ref().map(|record| record.attempt + 1).unwrap_or(1);
    let charged = previous.as_ref().map(|record| record.charged).unwrap_or(0);
    SpawnPlan {
        log: agent_log_attempt_path(runtime_dir, task_id, state_name, suffix.as_deref(), attempt),
        record: record_path,
        moves,
        attempt,
        accounting: None,
        charged,
        previous,
        evidence_state: serde_json::Value::Null,
    }
}

/// The spawn record this invocation's own name spells — or, for an uncounted
/// re-entry with none, the unsuffixed record a runtime written before entry
/// numbers existed left at the current `moves`, so its retry continues as
/// `-{n}-attempt2` rather than starting over.
///
/// Records are trusted exactly as before: the same name-and-`moves` test,
/// never checked against the log's header (V-D01).
// §FS-rhei-agents.8.1 §FS-rhei-agents.8.4
fn current_visit_record(
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    identity: Option<&str>,
    number: LogNumber,
    moves: u64,
) -> Option<SpawnRecord> {
    let suffix = log_suffix(identity, number.value());
    let own_path = spawn_record_path(runtime_dir, task_id, state_name, suffix.as_deref());
    let own = read_spawn_record(&own_path);
    if own.is_some() {
        return own;
    }
    let LogNumber::Entry(entry) = number else { return None };
    if entry <= 1 {
        return None;
    }
    read_spawn_record(&spawn_record_path(runtime_dir, task_id, state_name, identity)).filter(
        |record| record.task == task_id && record.state == state_name && record.moves == moves,
    )
}

impl SpawnPlan {
    /// The refusal of a spawn whose log name is already taken.
    ///
    /// The attempt is derived from the records, so a name they account for is
    /// always behind the one planned: a file *at* the planned name is one no
    /// record accounts for — a planted file, a stale report-less leftover, or
    /// the log of a spawn killed before it could record itself. It is kept, and
    /// neither the attempt nor the entry moves forward because it exists.
    // §FS-rhei-agents.8.1
    fn unaccounted_log_refusal(&self, task_id: &str) -> Option<String> {
        self.log.exists().then(|| unaccounted_log_message(&self.log, task_id))
    }
}

/// The sentence a taken, unaccounted log name is refused with, and on the line
/// after it the remedy, naming the rhei that owns the ticket — the first
/// segment of its qualified id. §FS-rhei-agents.8.1
fn unaccounted_log_message(log: &Path, task_id: &str) -> String {
    let rhei = task_id.split('.').next().unwrap_or(task_id);
    format!(
        "refusing to spawn: {} exists and no spawn record accounts for it\n  help: move \
         that file away, or run `rhei reset --rhei {rhei}` to clear the ticket's runtime",
        log.display()
    )
}

/// Plan the next spawn of one invocation under the name §FS-rhei-agents.8.1 gives it, or
/// say why it may not happen: the ledger that numbers it cannot be read, or the
/// name is taken by a file no record accounts for. Both are refusals the
/// caller prints before anything is composed, staged, or paid for.
// §FS-rhei-agents.8.1 §FS-rhei-programs.5.1
fn plan_named_spawn(
    machine: &rhei_validator::StateMachine,
    task_root: &Path,
    runtime_dir: &Path,
    task_id: &str,
    state_name: &str,
    visit_count: u64,
    identity: Option<&str>,
) -> Result<(SpawnPlan, LogNumber), String> {
    let number =
        resolve_log_number(machine, state_name, visit_count, task_root, runtime_dir, task_id)?;
    let plan = plan_spawn_attempt(runtime_dir, task_root, task_id, state_name, identity, number);
    match plan.unaccounted_log_refusal(task_id) {
        Some(refusal) => Err(refusal),
        None => Ok((plan, number)),
    }
}

/// The roots a run's spawn sites plan a ticket's next spawn under: where its
/// ledger is read and its spawn record kept, and what its agents resolve
/// under. A reading of the plan holds them to ask what that spawn would be
/// without making it. §FS-rhei-agents.8.4
///
/// A reading no run is behind asks as a run started now would: under the
/// plan's own runtime, with the default options, and with no rhei admitted
/// after startup.
struct SpawnSiteRoots<'a> {
    /// Each ticket's own root, which its ledger and a program's record sit under.
    tasks: ReadySetRoots<'a>,
    /// The run's runtime directory, where an initial member's agents record.
    runtime_dir: &'a Path,
    /// Which rheis' agents record under their own root instead. §FS-rhei-panta.6.2
    machines: &'a ExecutionMachines,
    /// The run's settings. `None` reads the merged settings each time an agent
    /// poll asks, as the ready set's input check reads them per ticket; a
    /// program poll resolves no agents and reads none.
    settings: Option<&'a RheiSettings>,
    /// The options each agent resolves under, which name the record it keeps.
    opts: &'a RunOptions,
}

impl<'a> SpawnSiteRoots<'a> {
    /// The roots a run's spawn sites plan under, with each ticket's own root as
    /// its loaded plan gives it.
    fn new(
        workspace_root: &'a Path,
        runtime_dir: &'a Path,
        loaded: &'a LoadedPlan,
        machines: &'a ExecutionMachines,
        settings: Option<&'a RheiSettings>,
        opts: &'a RunOptions,
    ) -> Self {
        let tasks = ReadySetRoots { workspace_root, task_roots: &loaded.task_roots };
        Self { tasks, runtime_dir, machines, settings, opts }
    }

    /// The roots a reading no run is behind plans under, given the plan's own
    /// runtime: the default options, and the merged settings, read each time an
    /// agent poll asks.
    fn reading(
        workspace_root: &'a Path,
        runtime_dir: &'a Path,
        loaded: &'a LoadedPlan,
        machines: &'a ExecutionMachines,
    ) -> Self {
        Self::new(workspace_root, runtime_dir, loaded, machines, None, no_run_options())
    }

    /// The one answer to whether a poll ticket waits for its next attempt, and
    /// until when: the `pollNextAttemptAt` its state records, while that is at
    /// or after `since` and the attempt it schedules would spawn. A poll whose
    /// bound is spent is not blocked by it, whatever the last charge or a lowered
    /// `max_attempts` left there, so the ticket stays ready and its spawn site
    /// halts it. A reader asking whether the deadline holds now passes `now + 1`.
    ///
    /// The spawn record is read only for a poll this deadline would hold.
    // §FS-rhei-run.5.1 §FS-rhei-run.3.7.4
    fn pending_poll_deadline(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
        since: u64,
    ) -> Option<u64> {
        machine.states.get(state_name)?.poll.as_ref()?;
        let deadline = poll_next_attempt_at(rhei.metadata.as_ref(), &task.id, state_name)
            .filter(|deadline| *deadline >= since)?;
        (!self.poll_bound_spent(rhei, machine, task, state_name)).then_some(deadline)
    }

    /// Whether this ticket's `poll:` state has spent its bound: every spawn it
    /// would make next is refused for `poll.max_attempts`, so it is stalled, not
    /// waiting. A state that is no poll, or an agent poll whose agents or
    /// settings do not resolve, answers `false` and leaves its deadline standing.
    ///
    /// Settings are read only to resolve an agent poll's agents. A poll's budget
    /// is its own `max_attempts`, which no setting changes, so a program poll
    /// answers from its spawn record alone.
    // §FS-rhei-run.3.7.4
    fn poll_bound_spent(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
    ) -> bool {
        let Some(state_def) = machine.states.get(state_name) else { return false };
        let Some(budget) = poll_attempt_budget(state_def) else { return false };
        let agents = if state_def.program.is_some() {
            None
        } else {
            let mut read = None;
            let settings = match self.settings {
                Some(settings) => settings,
                None => match load_merged_settings(self.tasks.workspace_root) {
                    Ok(settings) => &*read.insert(settings),
                    Err(_) => return false,
                },
            };
            let resolved =
                resolve_agent_invocations_for_task(machine, state_name, settings, self.opts, Some(task));
            let Ok(resolved) = resolved else { return false };
            Some(resolved)
        };
        self.next_spawns_spent(rhei, machine, task, state_name, agents.as_deref(), budget)
    }

    /// Whether every spawn this ticket's state would make next is refused for
    /// `budget` spent, asked as its spawn site asks it: the same plan, from the
    /// same roots and under the same name, held to the same gate. So a reader
    /// that asks this never waits for a spawn the spawner will refuse.
    ///
    /// `agents` is `None` for a program state. A plan its site would refuse for
    /// another reason, such as an unreadable ledger, answers `false`: that
    /// refusal is not a spent budget.
    // §FS-rhei-agents.3.2.3 §FS-rhei-agents.8.4 §FS-rhei-run.3.7.4
    fn next_spawns_spent(
        &self,
        rhei: &rhei_core::ast::Rhei,
        machine: &rhei_validator::StateMachine,
        task: &rhei_core::ast::Task,
        state_name: &str,
        agents: Option<&[ResolvedAgent]>,
        budget: AttemptBudget,
    ) -> bool {
        let task_id = task.id.to_string();
        let task_root = self.tasks.artifact_root(&task_id);
        let visit_count = render_visit_count(
            rhei.metadata.as_ref(),
            &task.id,
            state_name,
            task.state.as_str(),
            machine,
        );
        let spent = |runtime_dir: &Path, identity: Option<&str>| {
            let planned = plan_named_spawn(
                machine,
                task_root,
                runtime_dir,
                &task_id,
                state_name,
                visit_count,
                identity,
            );
            planned.is_ok_and(|(plan, _)| plan.budget_spent(budget).is_some())
        };
        match agents {
            // A program records under its ticket's own root. §FS-rhei-programs.5.1
            None => spent(&task_root.join("runtime"), None),
            Some(agents) => {
                let runtime_dir =
                    self.machines.agent_runtime_dir(&task_id, task_root, self.runtime_dir);
                !agents.is_empty()
                    && agents.iter().all(|agent| {
                        spent(&runtime_dir, resolved_agent_log_identity(agent).as_deref())
                    })
            }
        }
    }

    /// A single-file plan's roots, where every ticket's root is the plan's own
    /// and no rhei was admitted after startup: the shape the unit tests build,
    /// under the default settings rather than whatever the machine has.
    #[cfg(test)]
    fn plan_only(workspace_root: &'a Path, runtime_dir: &'a Path) -> Self {
        static NONE_ADMITTED: std::sync::OnceLock<ExecutionMachines> = std::sync::OnceLock::new();
        let machines = NONE_ADMITTED.get_or_init(|| ExecutionMachines {
            set: rhei_validator::MachineSet::single(
                rhei_validator::StateMachine::builtin_default(),
            ),
            default_callbacks: CallbackPaths {
                plan_path: PathBuf::new(),
                state_machine_path: None,
                working_dir: PathBuf::new(),
            },
            per_rhei_callbacks: BTreeMap::new(),
            member_local_runtimes: BTreeSet::new(),
            state_machine_override: None,
        });
        static DEFAULT_SETTINGS: std::sync::OnceLock<RheiSettings> = std::sync::OnceLock::new();
        let settings = Some(DEFAULT_SETTINGS.get_or_init(RheiSettings::default));
        let tasks = ReadySetRoots::plan_only(workspace_root);
        Self { tasks, runtime_dir, machines, settings, opts: no_run_options() }
    }
}

/// The options a reading no run is behind resolves agents under.
fn no_run_options() -> &'static RunOptions {
    static DEFAULTS: std::sync::OnceLock<RunOptions> = std::sync::OnceLock::new();
    DEFAULTS.get_or_init(default_run_options)
}
