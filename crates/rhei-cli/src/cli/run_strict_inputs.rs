// `rhei run --strict-inputs`: the seal a run takes at start, the check points
// that compare it with the disk, and the settings copy the run reads once.
//
// A process-wide guard, as the run-diagnostic sink is: the check points sit
// under every spawn path and the transition engine, and threading a parameter
// through all of them is the same impracticality `run_diag.rs` describes. A
// run that is not strict never arms it, and then every check is a no-op.

// §FS-rhei-run.3.8 §FS-rhei-seal.8

static STRICT_INPUTS: Mutex<Option<StrictInputs>> = Mutex::new(None);

/// The settings a strict run read at start, handed to every later read.
/// §FS-rhei-seal.8.3
static STRICT_SETTINGS: std::sync::RwLock<Option<KeptSettings>> = std::sync::RwLock::new(None);

struct KeptSettings {
    settings: RheiSettings,
    /// Unit tests share one process, so a test's copy reaches its own thread only.
    #[cfg(test)]
    owner: std::thread::ThreadId,
}

impl KeptSettings {
    fn new(settings: RheiSettings) -> Self {
        #[cfg(test)]
        return Self { settings, owner: std::thread::current().id() };
        #[cfg(not(test))]
        Self { settings }
    }
}

struct StrictInputs {
    dry_run: bool,
    run_id: String,
    workspace_root: PathBuf,
    price_book: Option<PathBuf>,
    /// The remedy names the plan the operator ran, and the rhei when it is a member.
    plan_arg: String,
    project: bool,
    rheis: BTreeMap<String, StrictRhei>,
    /// Tickets already refused, so each gets one refusal however often it is ready.
    refused: BTreeSet<String>,
}

struct StrictRhei {
    scope: SealScope,
    generation: u32,
    sealed: SealSnapshot,
    /// Set at the first refusal: the rhei starts nothing more this run. §FS-rhei-seal.8.4
    drift: Option<Vec<SealChange>>,
}

/// Disarms the guard when the run ends, however it ends.
struct StrictInputsGuard;

impl Drop for StrictInputsGuard {
    fn drop(&mut self) {
        *STRICT_INPUTS.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
        *STRICT_SETTINGS.write().unwrap_or_else(|poison| poison.into_inner()) = None;
    }
}

/// The kept settings while a strict run is armed. §FS-rhei-seal.8.3
fn strict_kept_settings() -> Option<RheiSettings> {
    let kept = STRICT_SETTINGS.read().ok()?;
    let kept = kept.as_ref()?;
    #[cfg(test)]
    if kept.owner != std::thread::current().id() {
        return None;
    }
    Some(kept.settings.clone())
}

/// Hold `settings` as the copy every later read gets, until the guard drops.
/// §FS-rhei-seal.8.3
fn keep_strict_settings(settings: &RheiSettings) -> StrictInputsGuard {
    *STRICT_SETTINGS.write().unwrap_or_else(|poison| poison.into_inner()) =
        Some(KeptSettings::new(settings.clone()));
    StrictInputsGuard
}

/// Seal every in-scope rhei that has no seal, before the first spawn, and arm
/// the check points. `None` when the run is not strict. §FS-rhei-seal.8.1
fn arm_strict_inputs(
    input: &Path,
    loaded: &LoadedPlan,
    resolved: &ResolvedMachineSet,
    rhei_scope: &RheiScope,
    settings: &RheiSettings,
    opts: &RunOptions,
    run_id: &str,
) -> MietteResult<Option<StrictInputsGuard>> {
    if !opts.strict_inputs() {
        return Ok(None);
    }
    let workspace_root = run_execution_root(input);
    let price_book = opts.prices_path().map(Path::to_path_buf);
    let mut strict = StrictInputs {
        dry_run: opts.dry_run(),
        run_id: run_id.to_string(),
        workspace_root: workspace_root.clone(),
        price_book: price_book.clone(),
        plan_arg: plan_arg_for_help(input),
        project: loaded.is_panta_project(),
        rheis: BTreeMap::new(),
        refused: BTreeSet::new(),
    };
    for scope in seal_scopes(loaded, resolved, rhei_scope, &workspace_root, price_book.as_deref()) {
        strict.adopt(scope)?;
    }
    let guard = keep_strict_settings(settings);
    *STRICT_INPUTS.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(strict);
    Ok(Some(guard))
}

impl StrictInputs {
    /// Hold a rhei's latest generation, sealing generation 1 when it has none;
    /// a dry run says it would and writes nothing. §FS-rhei-seal.8.1 §FS-rhei-seal.8.2
    fn adopt(&mut self, scope: SealScope) -> MietteResult<()> {
        let (generation, sealed) = match latest_seal_generation(&scope)? {
            Some(latest) => latest,
            None if self.dry_run => {
                let snapshot = take_seal_snapshot(&scope)?;
                let count = snapshot.file_count();
                strict_announce(format!("Would seal rhei `{}`: generation 1, {count} files", scope.rhei));
                (1, snapshot)
            }
            None => {
                let taken = take_seal(&scope, SealTaker::Run(&self.run_id))?;
                strict_announce(sealed_line(&scope.rhei, &taken));
                (taken.generation, taken.snapshot)
            }
        };
        self.rheis.insert(scope.rhei.clone(), StrictRhei { scope, generation, sealed, drift: None });
        Ok(())
    }

    fn remedy(&self, rhei: &str) -> String {
        match self.project {
            true => format!("rhei seal {} --rhei {rhei}", self.plan_arg),
            false => format!("rhei seal {}", self.plan_arg),
        }
    }
}

/// Before the run's sink exists, a line goes where human output goes.
/// §FS-rhei-run-json.1
fn strict_announce(line: String) {
    if stdout_carries_json_records() {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}

/// What a check point found for a rhei that changed.
struct StrictRefusal {
    first: bool,
    rhei: String,
    generation: u32,
    changes: Vec<SealChange>,
    remedy: String,
}

/// Re-hash the ticket's rhei and compare it with the latest generation.
/// `None` when the run is not strict, the rhei is out of scope, or nothing
/// changed. §FS-rhei-seal.8.2
fn strict_check(task_id: &str) -> Option<StrictRefusal> {
    let mut guard = STRICT_INPUTS.lock().unwrap_or_else(|poison| poison.into_inner());
    let strict = guard.as_mut()?;
    let rhei = task_id.split('.').next().unwrap_or(task_id);
    let entry = strict.rheis.get_mut(rhei)?;
    if entry.drift.is_none() {
        let changes = match take_seal_snapshot(&entry.scope) {
            Ok(current) => seal_changes(&entry.sealed, &current),
            // A sealed file the run cannot read is not the file it sealed.
            Err(err) => vec![SealChange { path: format!("{err}"), kind: "modified" }],
        };
        if changes.is_empty() {
            return None;
        }
        entry.drift = Some(changes);
    }
    let (generation, changes) = (entry.generation, entry.drift.clone().unwrap_or_default());
    Some(StrictRefusal {
        first: strict.refused.insert(task_id.to_string()),
        rhei: rhei.to_string(),
        generation,
        changes,
        remedy: strict.remedy(rhei),
    })
}

/// Whether a strict run refuses to start `task_id` in `state`: its rhei's
/// sealed inputs changed. Called before bound admission, so a refusal reserves
/// and charges nothing. §FS-rhei-seal.8.2 §FS-rhei-seal.8.4
fn strict_refuses_start(task_id: &str, state: &str) -> bool {
    let Some(refusal) = strict_check(task_id) else { return false };
    if refusal.first {
        emit_seal_halt(task_id, state, &refusal, "was not started");
    }
    true
}

/// Whether this run already refused `task_id`; it is not scheduled again.
/// §FS-rhei-seal.8.4
fn strict_ticket_refused(task_id: &str) -> bool {
    let guard = STRICT_INPUTS.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.as_ref().is_some_and(|strict| strict.refused.contains(task_id))
}

/// The check point of a transition that runs a callback: refused, it returns
/// a [`SealRefused`] before anything is written. §FS-rhei-seal.8.2 §FS-rhei-seal.8.5
fn strict_check_callback(runs_callback: bool, task_id: &str, state: &str) -> MietteResult<()> {
    if !runs_callback {
        return Ok(());
    }
    let Some(refusal) = strict_check(task_id) else { return Ok(()) };
    if refusal.first {
        emit_seal_halt(task_id, state, &refusal, "had its exit left unrouted");
    }
    Err(miette::Report::new(SealRefused { task: task_id.to_string() }))
}

/// A callback refused at its check point. The exit that selected it is left
/// unrouted, as an interrupted one is. §FS-rhei-seal.8.5
#[derive(Debug)]
struct SealRefused {
    task: String,
}

impl std::fmt::Display for SealRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} was not routed: a file sealed for its rhei has changed", self.task)
    }
}

impl std::error::Error for SealRefused {}
impl miette::Diagnostic for SealRefused {}

fn is_seal_refusal(error: &miette::Report) -> bool {
    error.downcast_ref::<SealRefused>().is_some()
}

/// The refusal, as prose and as a `seal_halt` record. §FS-rhei-seal.8.4
fn emit_seal_halt(task_id: &str, state: &str, refusal: &StrictRefusal, what: &str) {
    let mut text = format!(
        "error: {task_id} {what}: a file sealed for rhei `{}` has changed",
        refusal.rhei
    );
    for change in &refusal.changes {
        text.push_str(&format!(
            "\n  {:<9} {}   (sealed in generation {})",
            change.kind, change.path, refusal.generation
        ));
    }
    text.push_str(&format!(
        "\n  The ticket stays in '{state}'. Nothing was charged.\n  help: restore the file, or \
         accept the change with `{}`; then run again",
        refusal.remedy
    ));
    let event = rhei_tui::RunEvent::SealHalt {
        task: task_id.to_string(),
        rhei: refusal.rhei.clone(),
        state: state.to_string(),
        generation: refusal.generation,
        changes: refusal.changes.iter().map(|c| (c.path.clone(), c.kind.to_string())).collect(),
        remedy: refusal.remedy.clone(),
    };
    let sink = RUN_DIAG_SINK.read().ok().and_then(|sink| sink.clone());
    match sink {
        Some(sink) => {
            sink.emit(rhei_tui::RunEvent::Message { level: rhei_tui::MessageLevel::Error, text });
            sink.emit(event);
        }
        None => eprintln!("{text}"),
    }
}

/// How many tickets a strict run refused; a refusal is work for a person, so
/// the run exits 1 even when everything else went well. §FS-rhei-seal.8.4
fn strict_refusal_outcome(result: MietteResult<()>) -> MietteResult<()> {
    let refused = STRICT_INPUTS.lock().ok().and_then(|strict| strict.as_ref().map(|s| s.refused.len()));
    match (result, refused) {
        (Ok(()), Some(count)) if count > 0 => Err(miette!(
            help = "restore the changed files, or accept them with `rhei seal`; then run again",
            "{count} ticket(s) refused because their sealed inputs changed"
        )),
        (result, _) => result,
    }
}

/// At a live admission: seal each admitted member on arrival, and answer the
/// existing members whose sealed files changed, which keep the machine they
/// hold. §FS-rhei-seal.8.6
fn strict_admission(
    admitted: &[String],
    loaded: &LoadedPlan,
    resolved: &ResolvedMachineSet,
) -> MietteResult<BTreeSet<String>> {
    let mut guard = STRICT_INPUTS.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(strict) = guard.as_mut() else { return Ok(BTreeSet::new()) };
    let mut drifted = BTreeSet::new();
    for (rhei, entry) in strict.rheis.iter_mut() {
        if entry.drift.is_none() {
            let current = take_seal_snapshot(&entry.scope)?;
            let changes = seal_changes(&entry.sealed, &current);
            if !changes.is_empty() {
                entry.drift = Some(changes);
            }
        }
        if entry.drift.is_some() {
            drifted.insert(rhei.clone());
        }
    }
    let scope: RheiScope = Some(admitted.iter().cloned().collect());
    let (root, book) = (strict.workspace_root.clone(), strict.price_book.clone());
    for scope in seal_scopes(loaded, resolved, &scope, &root, book.as_deref()) {
        strict.adopt(scope)?;
    }
    Ok(drifted)
}

/// Keep, in the machines a live admission adopts, what a drifted member held.
/// §FS-rhei-seal.8.6
fn keep_held_machines(held: &ExecutionMachines, adopted: &mut ExecutionMachines, rheis: &BTreeSet<String>) {
    for rhei in rheis {
        let machine = held.set.per_rhei.get(rhei).unwrap_or(&held.set.default).clone();
        adopted.set.per_rhei.insert(rhei.clone(), machine);
        let callbacks = held.per_rhei_callbacks.get(rhei).unwrap_or(&held.default_callbacks).clone();
        adopted.per_rhei_callbacks.insert(rhei.clone(), callbacks);
    }
}
