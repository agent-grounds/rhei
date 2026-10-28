// The run's identity on disk: `runtime/run.json` in the workspace and a
// pointer under the user's state directory so a bare id resolves from
// anywhere. Every non-dry run publishes both, detached or not — the identity
// belongs to the run, not to `--headless`.

// §FS-rhei-run-headless.2

use std::sync::OnceLock;

/// Where a run is in its life. `finished` and `failed` are written by the
/// process itself on the way out; a `SIGKILL`ed run is left saying `running`,
/// which is why liveness is probed rather than decided by this field alone.
// §FS-rhei-run-headless.2
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RunStatus {
    Running,
    Finished,
    Failed,
}

impl RunStatus {
    fn label(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Finished => "finished",
            RunStatus::Failed => "failed",
        }
    }

    /// Whether the run recorded its own end.
    fn is_terminal(self) -> bool {
        matches!(self, RunStatus::Finished | RunStatus::Failed)
    }
}

/// What a liveness probe could establish. The third answer is the point: an
/// error reading the run lock or the workspace descriptor says nothing about
/// the run, and a sweep that read it as death would unregister — irreversibly
/// — a run that is still working.
// §FS-rhei-run-headless.3
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Liveness {
    Live,
    /// The run is over, but its workspace still names it: the pointer stays so
    /// `rhei attach <id>` can still report what happened.
    Ended,
    /// The run is over *and* the workspace no longer names it — superseded by a
    /// later run, or gone from disk entirely. Nothing can make the pointer
    /// meaningful again, so this is the only verdict that prunes.
    Gone,
    /// Something in the way could not be read. Says nothing either way.
    Unknown(String),
}

impl Liveness {
    /// Whether the run is over as far as anything destructive is concerned.
    pub(crate) fn has_ended(&self) -> bool {
        matches!(self, Liveness::Ended | Liveness::Gone)
    }
}

/// How long a follower keeps polling a run whose liveness it cannot decide.
///
/// Long enough to outlast the momentary outage the tri-state exists for — a
/// `chmod` being put back, a full disk being cleared — and short enough that a
/// CI step waiting on a run does not hang on an answer that is never coming.

// §FS-rhei-run-headless.3
const UNDECIDED_GRACE: Duration = Duration::from_secs(5);

/// A run of consecutive undecided liveness probes, and the budget one gets.
///
/// Every follower — `attach --wait`, `attach --json`, `stop --wait` — needs the
/// same answer to the same question: an undecided probe is not an ended run, so
/// keep waiting, but do not wait forever on a filesystem that has stopped
/// answering. Counting it in one place is what keeps the three from disagreeing.

// §FS-rhei-run-headless.3
#[derive(Default)]
pub(crate) struct UndecidedWatch {
    since: Option<Instant>,
    reason: String,
}

impl UndecidedWatch {
    /// Note a decided probe, which retires any grace in progress.
    pub(crate) fn decided(&mut self) {
        self.since = None;
        self.reason.clear();
    }

    /// Note an undecided probe. `true` once the grace has run out, and the
    /// watch is rearmed so a caller that keeps going reports again rather than
    /// on every poll from then on.
    pub(crate) fn exhausted(&mut self, reason: &str) -> bool {
        reason.clone_into(&mut self.reason);
        match self.since {
            Some(started) if started.elapsed() >= UNDECIDED_GRACE => {
                self.since = None;
                true
            }
            Some(_) => false,
            None => {
                self.since = Some(Instant::now());
                false
            }
        }
    }

    /// Why the last undecided probe could not answer.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

/// The published description of one run. §FS-rhei-run-headless.2
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RunDescriptor {
    pub(crate) id: String,
    pub(crate) pid: u32,
    pub(crate) status: RunStatus,
    pub(crate) workspace: PathBuf,
    pub(crate) plan: PathBuf,
    /// The state machine this run resolved, when `--state-machine` selected
    /// one. An attached surface must judge the plan under the run's own
    /// machine, not under whatever the default resolves to today.
    // §FS-rhei-run-headless.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) state_machine: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) control_url: Option<String>,
    pub(crate) started_at: String,
    pub(crate) headless: bool,
    pub(crate) parallel: usize,
    /// The detached run's redirected console. Absent for a foreground run,
    /// which has no console of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) log: Option<PathBuf>,
    pub(crate) events: PathBuf,
    /// Always serialized, `null` while unknown: the §FS-rhei-run-headless.2
    /// object is a fixed shape, and a consumer that reads `exit_code` must not
    /// have to tell "still running" from "this build forgot the field".
    // §FS-rhei-run-headless.2
    #[serde(default)]
    pub(crate) exit_code: Option<i32>,
    /// What ran and on what: always serialized, `null` where an older
    /// descriptor carried none. §FS-rhei-run-headless.2
    #[serde(default)]
    pub(crate) rhei_build: Option<rhei_tui::RheiBuild>,
    #[serde(default)]
    pub(crate) checkout_head_at_start: Option<rhei_tui::CheckoutHead>,
}

impl RunDescriptor {
    /// Whether *this* run is still the live one on its workspace.
    /// The workspace descriptor first agrees on id, pid, and non-terminal
    /// status. The run lock is then the primary process-independent probe. On
    /// Linux, a stable process-start identity plus an exclusively locked file
    /// descriptor carrying this run's ownership record identifies the recorded
    /// supervisor whether the locked inode is current or displaced.
    ///
    /// Every step can also fail to answer, and a failure to answer is its own
    /// verdict: see [`Liveness`].
    // §FS-rhei-run-headless.3
    pub(crate) fn liveness(&self) -> Liveness {
        let path = run_descriptor_path(&self.workspace);
        let current = match read_descriptor_result(&path) {
            // Only `ENOENT` proves absence. Any other error is the filesystem
            // declining to answer, which is not the same thing.
            DescriptorRead::Missing => return Liveness::Gone,
            DescriptorRead::Unreadable(why) => {
                return Liveness::Unknown(format!("{} could not be read: {why}", path.display()));
            }
            DescriptorRead::Loaded(current) => current,
        };
        if current.id != self.id || current.pid != self.pid {
            return Liveness::Gone;
        }
        if self.status.is_terminal() || current.status.is_terminal() {
            return Liveness::Ended;
        }
        classify_lock_and_process(probe_run_lock(&self.workspace), self)
    }

    /// A one-line human summary for `rhei runs` and `rhei stop`.
    pub(crate) fn summary_line(&self) -> String {
        let plan = self.plan.display();
        let mode = if self.headless { "headless" } else { "foreground" };
        format!(
            "{id}  {status:<9} {mode:<10} pid {pid:<7} parallel {parallel}  {plan}",
            id = self.id,
            status = self.status.label(),
            pid = self.pid,
            parallel = self.parallel,
        )
    }
}

/// Ask a workspace's run lock whether anybody holds it, **without creating
/// anything**. `open_run_lock_file` does `create_dir_all` plus `O_CREAT`, which
/// is right for a run that is about to take the lock and wrong for a listing:
/// `rhei runs` and shell completion would write into every foreign workspace
/// they inspect.
///
/// A missing lock file is `unknown`, not free. `flock` survives unlinking, so
/// at this point in the sequence — the workspace descriptor still names this
/// run — an absent file does not prove the holder is gone.
// §FS-rhei-run-headless.3 §FS-rhei-run.2.6
fn probe_run_lock(workspace_root: &Path) -> RunLockProbe {
    let path = workspace_root.join(".rhei").join("run.lock");
    let file = match fs::OpenOptions::new().read(true).open(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return RunLockProbe::Missing(format!("{} does not exist", path.display()));
        }
        Err(err) => {
            return RunLockProbe::Unknown(format!(
                "{} could not be opened: {err}",
                path.display()
            ));
        }
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = fs2::FileExt::unlock(&file);
            RunLockProbe::Free
        }
        // Contention is the whole point of the probe, and the platforms spell
        // it with different errnos. §FS-rhei-run-headless.3
        Err(err) if lock_is_contended(&err) => RunLockProbe::Held,
        Err(err) => {
            RunLockProbe::Unknown(format!("{} could not be probed: {err}", path.display()))
        }
    }
}

/// What the current lock pathname says, before stable ownership evidence closes
/// the gap between a pathname and a lock held on an older inode.
enum RunLockProbe {
    Held,
    Free,
    Missing(String),
    Unknown(String),
}

/// Reconcile the primary lock probe with stable ownership of the current or
/// displaced inode. Descriptor identity and terminal status have already taken
/// precedence in `liveness`.
// §FS-rhei-run-headless.3
#[cfg(target_os = "linux")]
fn classify_lock_and_process(lock: RunLockProbe, descriptor: &RunDescriptor) -> Liveness {
    match lock {
        RunLockProbe::Unknown(reason) => Liveness::Unknown(reason),
        RunLockProbe::Held | RunLockProbe::Free => match probe_recorded_lock_owner(descriptor) {
            ProcessProbe::OwnsLock => Liveness::Live,
            ProcessProbe::Gone | ProcessProbe::DoesNotOwnLock => Liveness::Ended,
            ProcessProbe::Unknown(reason) => Liveness::Unknown(reason),
        },
        RunLockProbe::Missing(lock_reason) => match probe_recorded_lock_owner(descriptor) {
            ProcessProbe::OwnsLock => Liveness::Live,
            ProcessProbe::DoesNotOwnLock => Liveness::Ended,
            ProcessProbe::Gone => Liveness::Unknown(lock_reason),
            ProcessProbe::Unknown(process_reason) => {
                Liveness::Unknown(format!("{lock_reason}; {process_reason}"))
            }
        },
    }
}

/// Platforms without Linux's stable process and descriptor inspection retain
/// the lock-only behavior. In particular, an acquirable lock remains a decided
/// end and a missing pathname remains unknown.
#[cfg(not(target_os = "linux"))]
fn classify_lock_and_process(lock: RunLockProbe, _descriptor: &RunDescriptor) -> Liveness {
    match lock {
        RunLockProbe::Held => Liveness::Live,
        RunLockProbe::Free => Liveness::Ended,
        RunLockProbe::Missing(reason) | RunLockProbe::Unknown(reason) => {
            Liveness::Unknown(reason)
        }
    }
}

/// `<workspace>/runtime/run.json`. §FS-rhei-run-headless.2
pub(crate) fn run_descriptor_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("runtime").join("run.json")
}

/// `<workspace>/runtime/run.log` — a detached run's redirected console.
pub(crate) fn run_console_log_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join("runtime").join("run.log")
}

/// Write a descriptor to `path` atomically, so a reader never sees half a file.
fn write_descriptor(path: &Path, descriptor: &RunDescriptor) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(descriptor)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, format!("{body}\n"))?;
    fs::rename(&temp, path)
}

/// Why a descriptor read produced nothing. The distinction is load-bearing:
/// `Missing` is the only outcome that lets a registry entry be pruned.
// §FS-rhei-run-headless.3
pub(crate) enum DescriptorRead {
    Loaded(Box<RunDescriptor>),
    /// `ENOENT`, specifically.
    Missing,
    Unreadable(String),
}

pub(crate) fn read_descriptor_result(path: &Path) -> DescriptorRead {
    match fs::read_to_string(path) {
        Ok(body) => match serde_json::from_str::<RunDescriptor>(&body) {
            Ok(descriptor) => DescriptorRead::Loaded(Box::new(descriptor)),
            Err(err) => DescriptorRead::Unreadable(err.to_string()),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DescriptorRead::Missing,
        Err(err) => DescriptorRead::Unreadable(err.to_string()),
    }
}

pub(crate) fn read_descriptor(path: &Path) -> Option<RunDescriptor> {
    match read_descriptor_result(path) {
        DescriptorRead::Loaded(descriptor) => Some(*descriptor),
        DescriptorRead::Missing | DescriptorRead::Unreadable(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Publication and finalization
// ---------------------------------------------------------------------------

/// What this process published, so the exit path can stamp the run's real
/// status and code once `dispatch` has returned and the exit code is known —
/// and can prove, before it writes, that the files still describe *this* run.
// §FS-rhei-run-headless.2
struct PublishedRun {
    path: PathBuf,
    id: String,
    pid: u32,
    workspace: PathBuf,
    /// Copied onto every invocation record the run writes, because the next
    /// run overwrites run.json. §FS-rhei-cost-accounting.3.5
    provenance: rhei_tui::RunProvenance,
}

static PUBLISHED_DESCRIPTOR: OnceLock<Mutex<Option<PublishedRun>>> = OnceLock::new();

fn published_slot() -> &'static Mutex<Option<PublishedRun>> {
    PUBLISHED_DESCRIPTOR.get_or_init(|| Mutex::new(None))
}

/// Absolute, and canonical where the path exists.
///
/// A descriptor is read by a process standing somewhere else entirely, so a
/// recorded `./plan.rhei.md` names nothing there — `rhei attach <id>` from
/// another directory could not load the plan at all.
// §FS-rhei-run-headless.2
fn absolutize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
    };
    rhei_core::platform::canonical_path(&absolute).unwrap_or(absolute)
}

/// Publish a run's descriptor and its registry pointer.
///
/// Best-effort throughout: failing to publish costs attachment, never the run.
/// A registry failure is *said out loud*, though — silence there is what left
/// an operator with a 30-second launch hang and no idea why.
// §FS-rhei-run-headless.2
pub(crate) fn publish_run_descriptor(descriptor: &RunDescriptor) {
    let descriptor = RunDescriptor {
        workspace: absolutize(&descriptor.workspace),
        plan: absolutize(&descriptor.plan),
        state_machine: descriptor.state_machine.as_deref().map(absolutize),
        log: descriptor.log.as_deref().map(absolutize),
        events: absolutize(&descriptor.events),
        ..descriptor.clone()
    };
    let path = run_descriptor_path(&descriptor.workspace);
    if let Err(err) = write_descriptor(&path, &descriptor) {
        eprintln!("warning: could not publish the run descriptor at {}: {err}", path.display());
        return;
    }
    *published_slot().lock().unwrap_or_else(|poison| poison.into_inner()) = Some(PublishedRun {
        path,
        id: descriptor.id.clone(),
        pid: descriptor.pid,
        workspace: descriptor.workspace.clone(),
        provenance: rhei_tui::RunProvenance {
            rhei_build: descriptor.rhei_build.clone().unwrap_or_default(),
            checkout_head_at_start: descriptor.checkout_head_at_start.clone(),
        },
    });
    let Some(registry) = run_registry_path(&descriptor.id) else {
        eprintln!(
            "warning: neither XDG_STATE_HOME nor HOME is set, so run {} has no registry \
             entry; reach it by path instead of by id",
            descriptor.id
        );
        return;
    };
    if let Err(err) = write_descriptor(&registry, &descriptor) {
        eprintln!(
            "warning: could not write the run registry entry at {}: {err}\n\
             `rhei attach {}` will need the workspace path instead of the id.",
            registry.display(),
            descriptor.id
        );
    }
}

/// The id of the run this process published, if it has published one.
///
/// The published slot, not a re-read of `runtime/run.json`: by the time an
/// artifact is written mid-run a later run may already have overwritten that
/// file, and stamping its id onto this run's record would attribute the spend
/// to the wrong run. A process that published nothing — a dry run — has no id
/// to give, and what it writes is unattributed.
// §FS-rhei-run-headless.2 §FS-rhei-cost-accounting.3.5
pub(crate) fn current_run_id() -> Option<String> {
    published_slot()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .map(|published| published.id.clone())
}

/// The published run's provenance, for the records it writes.
/// §FS-rhei-cost-accounting.3.5
pub(crate) fn current_run_provenance() -> Option<rhei_tui::RunProvenance> {
    published_slot()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .map(|published| published.provenance.clone())
}

/// Stamp the run's terminal status and exit code into both copies.
///
/// The registry entry is **rewritten, not removed**. A pointer that vanishes
/// the instant a run ends breaks the one shape the launcher deliberately does
/// not provide on its own — launch, then `rhei attach --wait <id>` — because
/// the id stops resolving exactly when the answer becomes available.
// §FS-rhei-run-headless.2 §FS-rhei-run-headless.5.3
pub(crate) fn finalize_run_descriptor(exit_code: i32) {
    // A poisoned lock must not skip finalization: the slot holds a path and an
    // identity, nothing a panic could have torn. §FS-rhei-run-headless.2
    let taken = published_slot().lock().unwrap_or_else(|poison| poison.into_inner()).take();
    let Some(published) = taken else {
        return;
    };
    let Some(mut descriptor) = read_descriptor(&published.path) else {
        return;
    };
    // A later run may already own this workspace. Stamping its descriptor with
    // our exit code would report a live run as finished.
    if descriptor.id != published.id || descriptor.pid != published.pid {
        return;
    }
    descriptor.status = if exit_code == 0 { RunStatus::Finished } else { RunStatus::Failed };
    descriptor.exit_code = Some(exit_code);
    descriptor.control_url = None;
    let _ = write_descriptor(&published.path, &descriptor);
    finalize_registry_entry(&published, &descriptor);
}

/// Rewrite this run's registry entry with its terminal status — but only while
/// the entry still describes this process. Run ids are six hex characters, so a
/// collision is rare and not impossible, and a blind write by id could drop a
/// still-live run's pointer.
// §FS-rhei-run-headless.2
fn finalize_registry_entry(published: &PublishedRun, descriptor: &RunDescriptor) {
    let Some(path) = run_registry_path(&published.id) else {
        return;
    };
    let Some(existing) = read_descriptor(&path) else {
        return;
    };
    if existing.pid != published.pid || existing.workspace != published.workspace {
        return;
    }
    let _ = write_descriptor(&path, descriptor);
}
