// The run's side of a worker's edit that breaks the plan: which task regions
// are in flight and what each held at spawn, the restore made where a worker's
// process is reaped, and the one loader every run-path reload goes through,
// which waits on a live writer or stops with the culprit named.
//
// Its own part because that decision is the same at every reload and is
// written once; a reload site changes only which loader it names.

// §AR-source-file-size.3 §FS-rhei-run.3.7

/// One task region a live worker may be editing. §FS-rhei-run.3.7.1
struct InFlightRegion {
    file: PathBuf,
    snapshot: RegionSnapshot,
    /// Live processes of the task: a fanned-out state's invocations share one region.
    live: usize,
}

/// A worker whose process is reaped but whose completion the run has not yet
/// settled, kept with its region and snapshot while a reload it found broken
/// elsewhere is resolved. A later break in that region is still its own: the
/// restore is made on its behalf and the completion reads it when it settles.
// §FS-rhei-run.3.7.2 §FS-rhei-run.3.7.5
struct HeldExit {
    task_id: String,
    state: String,
    budget: AttemptBudget,
    region: InFlightRegion,
    /// The attempt's log, beside which the reverted text is kept. §FS-rhei-run.3.7.7
    log: PathBuf,
    /// Attempts the visit had spent before this one.
    charged: u64,
    /// The restore made on this completion's behalf, not yet told to its release.
    revert: Option<WorkerRevert>,
}

/// What a restore did for one attempt, left for the site that routes its exit.
// §FS-rhei-run.3.7.4 §FS-rhei-run.3.7.7
#[derive(Clone)]
struct WorkerRevert {
    task_id: String,
    state: String,
    /// `<file>:<line>`, the file relative to the workspace root.
    location: String,
    message: String,
    /// The reverted-text file, spelled as the journal spells paths.
    reverted_display: String,
    /// Attempts this visit has spent, this one included when it was charged.
    charged: u64,
    attempt_charged: bool,
    budget: AttemptBudget,
}

impl WorkerRevert {
    /// The one warning a restore prints. §FS-rhei-run.3.7.7
    fn warning(&self) -> String {
        let task = &self.task_id;
        let spent = match (self.attempt_charged, self.budget) {
            (false, _) => "the attempt is not charged".to_string(),
            (true, AttemptBudget::Visit(budget)) => {
                format!("attempt {} of {budget} spent", self.charged)
            }
            (true, AttemptBudget::Poll { max_attempts }) => {
                format!("attempt {} of {max_attempts} (poll.max_attempts) spent", self.charged)
            }
        };
        format!(
            "warning: Task {task}'s edit to its own task body broke the plan at {}\n         \
             ({}).\n         Reverted Task {task} to its text before this attempt; {spent}.\n         \
             The reverted text is {}\n  help: {}",
            self.location,
            self.message,
            self.reverted_display,
            worker_edit_help()
        )
    }
}

/// The revert an attempt's spawn record keeps, so a retry composed in a later
/// run is told the same thing. §FS-rhei-run.3.7.4 §FS-rhei-memory.4.4
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct RevertedEdit {
    location: String,
    message: String,
    text: PathBuf,
}

/// Where a reload failed, as the loader reported it.
struct PlanBreak {
    file: PathBuf,
    line: usize,
    message: String,
}

/// The run's in-flight regions and the restores it refused, behind one lock so
/// no restore lands between a reload and its classification. Unarmed outside
/// `rhei run`, where every load is plain.
struct WorkerRegions {
    armed: bool,
    root: Option<PathBuf>,
    in_flight: BTreeMap<String, InFlightRegion>,
    /// Exited workers whose completions are still held, by spawn record.
    held: BTreeMap<PathBuf, HeldExit>,
    /// Held exits an attributed stop left unrouted, by spawn record.
    stopped: std::collections::BTreeSet<PathBuf>,
    refusals: BTreeMap<String, String>,
}

static WORKER_REGIONS: Mutex<WorkerRegions> = Mutex::new(WorkerRegions {
    armed: false,
    root: None,
    in_flight: BTreeMap::new(),
    held: BTreeMap::new(),
    stopped: std::collections::BTreeSet::new(),
    refusals: BTreeMap::new(),
});

/// Signalled whenever a region leaves flight, which is what a waiting reload waits for.
static REGION_LEFT_FLIGHT: std::sync::Condvar = std::sync::Condvar::new();

fn worker_regions() -> std::sync::MutexGuard<'static, WorkerRegions> {
    WORKER_REGIONS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn worker_edit_help() -> &'static str {
    "write progress as plain paragraphs or lists; a heading inside a task body declares a child task"
}

/// Start keeping worker regions for the run on `input`. §FS-rhei-run.3.7
fn arm_worker_regions(input: &Path) {
    let mut regions = worker_regions();
    regions.armed = true;
    regions.root = Some(execution_workspace_root(input));
    regions.in_flight.clear();
    regions.held.clear();
    regions.stopped.clear();
    regions.refusals.clear();
}

/// `path` as the journal spells it: relative to the workspace root when inside it.
fn display_in_workspace(root: Option<&Path>, path: &Path) -> String {
    let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let path = canonical(path);
    root.map(canonical)
        .and_then(|root| path.strip_prefix(root).ok().map(|rel| rel.display().to_string()))
        .unwrap_or_else(|| path.display().to_string())
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Where loading `input` fails, from the same loaders `load_plan` runs; `None`
/// when it loads or the failure carries no line. §FS-rhei-run.3.7.2
fn locate_plan_break(input: &Path) -> Option<PlanBreak> {
    let _guards = rhei_core::root_access::for_input(input).ok()?;
    let err = if let Some(dir) = workspace::panta_project_dir(input) {
        workspace::load_panta_project(&dir).err()?
    } else if let Some(dir) = workspace::workspace_dir(input) {
        match workspace::load_workspace(&dir) {
            Ok(ws) => workspace::wrap_rhei_as_implicit_panta(ws, &dir).err()?,
            Err(err) => err,
        }
    } else {
        rhei_core::parse(&fs::read_to_string(input).ok()?).err()?
    };
    Some(PlanBreak {
        file: err.file.unwrap_or_else(|| input.to_path_buf()),
        line: err.line?,
        message: err.message,
    })
}

/// Which of the three cases a reload's failure is. §FS-rhei-run.3.7.2
#[derive(Debug, PartialEq, Eq)]
enum BreakClass {
    /// Inside the region of the worker whose exit found it.
    Own,
    /// Inside the region of a task whose worker is still running.
    Running(String),
    /// Anywhere else, or an error that carries no location.
    Outside,
}

impl WorkerRegions {
    /// Classify a break against the worker that just exited, if any, and the
    /// regions still in flight. §FS-rhei-run.3.7.2
    fn classify(&self, exited: Option<&InFlightRegion>, at: Option<&PlanBreak>) -> BreakClass {
        let Some(at) = at else { return BreakClass::Outside };
        let text = fs::read_to_string(&at.file).ok();
        let holds = |region: &InFlightRegion| {
            same_file(&region.file, &at.file)
                && text.as_deref().is_some_and(|text| {
                    region_span(text, &region.snapshot).is_ok_and(|span| span.holds_line(at.line))
                })
        };
        if exited.is_some_and(holds) {
            return BreakClass::Own;
        }
        self.in_flight
            .iter()
            .find(|(_, region)| region.live > 0 && holds(region))
            .map_or(BreakClass::Outside, |(task_id, _)| BreakClass::Running(task_id.clone()))
    }

    /// The held exit whose region holds the break and has not been restored
    /// yet: a case-1 break, whoever's reload found it. §FS-rhei-run.3.7.2
    fn held_holding(&self, at: Option<&PlanBreak>) -> Option<PathBuf> {
        self.held
            .iter()
            .filter(|(_, held)| held.revert.is_none())
            .find(|(_, held)| self.classify(Some(&held.region), at) == BreakClass::Own)
            .map(|(record, _)| record.clone())
    }

    /// One live process of `task_id` is gone; the region itself once it was the last.
    fn leave_flight(&mut self, task_id: &str) -> Option<InFlightRegion> {
        let region = self.in_flight.get_mut(task_id)?;
        region.live = region.live.saturating_sub(1);
        if region.live > 0 {
            return None;
        }
        self.in_flight.remove(task_id)
    }

    /// Stop the run on a break no exited worker answers for. Every exit still
    /// held unrouted is recorded the way an interruption is: uncharged, and its
    /// release journalled interrupted however late it is dropped. §FS-rhei-run.3.7.6
    fn stop_on(&mut self, at: &PlanBreak) -> Report {
        let unsettled: Vec<PathBuf> = self
            .held
            .iter()
            .filter(|(_, held)| held.revert.is_none())
            .map(|(record, _)| record.clone())
            .collect();
        for record in unsettled {
            uncharge_stopped_attempt(&record);
            self.stopped.insert(record);
        }
        self.attributed_stop(at)
    }

    /// The break no exited worker answers for, as the run stops on it. §FS-rhei-run.3.7.6
    fn attributed_stop(&self, at: &PlanBreak) -> Report {
        let place = format!("{}:{}", display_in_workspace(self.root.as_deref(), &at.file), at.line);
        let owner = task_owning_line(&at.file, at.line);
        let refused = owner
            .as_ref()
            .and_then(|task| self.refusals.get(task).map(|why| (task, why)))
            .map(|(task, why)| format!(" The run refused to restore Task {task}: {why}."))
            .unwrap_or_default();
        let head = match &owner {
            Some(task) => format!("Task {task}'s text broke the plan at {place}"),
            None => format!("the plan broke at {place}"),
        };
        miette!(
            help = format!("{}. Fix or remove that text, then re-run.", worker_edit_help()),
            "{head} ({}).{refused}",
            at.message
        )
    }
}

/// Load the plan for `rhei run`. A failure inside the region of an exited
/// worker whose completion is still held restores that region on its behalf;
/// one inside a live worker's region waits for that worker to exit, whose own
/// exit restores it; any other failure stops the run, attributed. Every held
/// completion goes through the same loop, however many culprits there are.
// §FS-rhei-run.3.7.2 §FS-rhei-run.3.7.5 §FS-rhei-run.3.7.6
fn load_run_plan(input: &Path) -> MietteResult<LoadedPlan> {
    let unlocked = match load_plan(input) {
        Ok(loaded) => return Ok(loaded),
        Err(err) => err,
    };
    let mut regions = worker_regions();
    if !regions.armed {
        return Err(unlocked);
    }
    loop {
        // Under the registry lock, so no restore lands between this load and
        // the classification below.
        let err = match load_plan(input) {
            Ok(loaded) => return Ok(loaded),
            Err(err) => err,
        };
        let at = locate_plan_break(input);
        if let (Some(record), Some(at)) = (regions.held_holding(at.as_ref()), at.as_ref()) {
            let mut held = regions.held.remove(&record).expect("held exit found above");
            match restore_exited(&mut regions, &held, &record, at) {
                Some(revert) => held.revert = Some(revert),
                None => return Err(regions.stop_on(at)),
            }
            regions.held.insert(record, held);
            continue;
        }
        if regions.classify(None, at.as_ref()) == BreakClass::Outside {
            return Err(at.map_or(err, |at| regions.stop_on(&at)));
        }
        // Bounded by the culprit's own process: its exit, its timeout, or the
        // operator's interrupt ends it, and its exit leaves flight. §FS-rhei-run.3.7.5
        regions =
            REGION_LEFT_FLIGHT.wait(regions).unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

/// A startup load whose failure is attributed as a mid-run one is.
/// §FS-rhei-run.3.7.6
fn attribute_startup_break(input: &Path, err: Report) -> Report {
    let regions = worker_regions();
    match (regions.armed, locate_plan_break(input)) {
        (true, Some(at)) => regions.attributed_stop(&at),
        _ => err,
    }
}

/// A worker's hold on its task's region, from just before its process starts
/// until the process has been reaped. Inert outside `rhei run`.
// §FS-rhei-run.3.7.1
struct WorkerRegionLease {
    task_id: Option<String>,
    state: String,
    input: PathBuf,
    budget: AttemptBudget,
}

/// Snapshot `task_id`'s region and put it in flight, or join the snapshot its
/// fanned-out siblings already took. §FS-rhei-run.3.7.1
fn begin_worker_region(
    loaded: &LoadedPlan,
    input: &Path,
    task_id: &str,
    state: &str,
    budget: AttemptBudget,
) -> WorkerRegionLease {
    let mut lease = WorkerRegionLease {
        task_id: None,
        state: state.to_string(),
        input: input.to_path_buf(),
        budget,
    };
    let mut regions = worker_regions();
    if !regions.armed {
        return lease;
    }
    if let Some(region) = regions.in_flight.get_mut(task_id) {
        region.live += 1;
    } else {
        let route = loaded.task_route(task_id, input);
        let Ok(text) = fs::read_to_string(&route.task_file) else { return lease };
        let Some(snapshot) = snapshot_region(&text, &route.local_id) else { return lease };
        let region = InFlightRegion { file: route.task_file, snapshot, live: 1 };
        regions.in_flight.insert(task_id.to_string(), region);
    }
    lease.task_id = Some(task_id.to_string());
    lease
}

impl WorkerRegionLease {
    /// The worker's process is reaped and its release read. When it was the
    /// task's last, a reload that fails inside its region restores the region,
    /// and the release and the operator are told; whether it did is returned.
    /// A reload that fails elsewhere holds the exit with its region until its
    /// completion settles. §FS-rhei-run.3.7.2 §FS-rhei-run.3.7.5 §FS-rhei-run.3.7.7
    fn exited(mut self, attempt: &SpawnPlan, release: &mut PendingSlotRelease) -> bool {
        let Some(task_id) = self.task_id.take() else { return false };
        let mut regions = worker_regions();
        let revert = regions.leave_flight(&task_id).and_then(|region| {
            let held = HeldExit {
                task_id,
                state: self.state.clone(),
                budget: self.budget,
                region,
                log: attempt.log.clone(),
                charged: attempt.charged,
                revert: None,
            };
            let at = locate_plan_break(&self.input)?;
            let revert = same_file(&at.file, &held.region.file)
                .then(|| restore_exited(&mut regions, &held, &attempt.record, &at))
                .flatten();
            if revert.is_none() {
                regions.held.insert(attempt.record.clone(), held);
                release.held_for(&attempt.record);
            }
            revert
        });
        drop(regions);
        REGION_LEFT_FLIGHT.notify_all();
        revert.map(|revert| announce_worker_revert(&revert, release)).is_some()
    }
}

/// Put an exited worker's region back between its two boundaries, under the
/// stable writer lock, keeping the worker's text beside the attempt's log; or
/// record why not. §FS-rhei-run.3.7.3
fn restore_exited(
    regions: &mut WorkerRegions,
    held: &HeldExit,
    record: &Path,
    at: &PlanBreak,
) -> Option<WorkerRevert> {
    let region = &held.region;
    let lock = LockedPlanFile::open(&region.file).ok()?;
    let text = lock.read_to_string("failed to read task file").ok()?;
    match region_span(&text, &region.snapshot) {
        Ok(span) if span.holds_line(at.line) => {}
        Ok(_) => return None,
        Err(refusal) => {
            regions.refusals.insert(held.task_id.clone(), refusal.describe(&region.snapshot));
            return None;
        }
    }
    let (restored, replaced) = restore_region(&text, &region.snapshot).ok()?;
    let reverted = reverted_text_path(&held.log);
    // The worker's text is kept before its region is overwritten, or not overwritten at all.
    if fs::write(&reverted, replaced).is_err()
        || write_file_atomic_locked(&region.file, &restored, Some(&lock)).is_err()
    {
        return None;
    }
    drop(lock);
    let root = regions.root.clone();
    let edit = RevertedEdit {
        location: format!("{}:{}", display_in_workspace(root.as_deref(), &at.file), at.line),
        message: at.message.clone(),
        text: reverted.clone(),
    };
    let charged = record_spawn_revert(record, &edit);
    Some(WorkerRevert {
        task_id: held.task_id.clone(),
        state: held.state.clone(),
        location: edit.location,
        message: edit.message,
        reverted_display: display_in_workspace(root.as_deref(), &reverted),
        charged: charged.map_or(held.charged + 1, |(charged, _)| charged),
        attempt_charged: charged.is_none_or(|(_, this)| this),
        budget: held.budget,
    })
}

impl Drop for WorkerRegionLease {
    /// A worker that unwound before it was reaped still leaves flight, so no
    /// reload waits on it forever.
    fn drop(&mut self) {
        let Some(task_id) = self.task_id.take() else { return };
        worker_regions().leave_flight(&task_id);
        REGION_LEFT_FLIGHT.notify_all();
    }
}
