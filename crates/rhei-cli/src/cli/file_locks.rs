// The stable writer lock every plan-rewriting command takes, and the one
// classification every `fs2` try-lock in this crate depends on.
//
// Its own part because both are platform facts rather than command behavior,
// and the commands that got them wrong — `rhei complete`, `rhei transition`,
// `rhei reset`, a dashboard gate choice, the run-lock liveness probe, the
// headless launch lock, snapshot-continue — had each gone their own way.

// §AR-source-file-size.3 §AR-agent-orchestrator-workflow.3.3.1

/// A plan pathname held under one stable exclusive writer lock.
///
/// `writer_lock` is a persistent sibling sidecar that atomic plan replacement
/// never touches. `file` retains the destination handle required by mandatory
/// locking platforms; a rename may release that handle, but never the sidecar.
struct LockedPlanFile {
    writer_lock: Mutex<Option<fs::File>>,
    file: PlanLockHandle,
    path: PathBuf,
}

impl LockedPlanFile {
    /// Take the stable sidecar before opening the destination it protects.
    ///
    /// A waiter therefore opens the current destination only after the prior
    /// writer's last replacement and cannot retain a stale plan inode.
    fn open(path: &Path) -> MietteResult<Self> {
        let lock_path = plan_lock_path(path)?;
        let writer_lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|err| file_io_report(&lock_path, "failed to open plan lock file", err))?;
        lock_plan_writer(&writer_lock, &lock_path)?;

        let file = fs::File::open(path)
            .map_err(|err| file_io_report(path, "failed to open plan file", err))?;
        file.lock_exclusive()
            .map_err(|err| file_io_report(path, "failed to acquire file lock", err))?;
        let file = Arc::new(Mutex::new(Some(file)));
        // The loader reads a plan, a workspace index, or a project manifest
        // through `rhei_core::source`, which knows nothing about locks until
        // this process tells it where to ask. §FS-rhei-new.4
        rhei_core::source::set_reader(plan_source_reader);
        held_plan_locks().lock().expect("held plan locks").push((path.to_path_buf(), file.clone()));
        Ok(Self {
            writer_lock: Mutex::new(Some(writer_lock)),
            file,
            path: path.to_path_buf(),
        })
    }

    /// Read the locked file, by path first.
    ///
    /// That ordering is the point: every plan rewrite in rhei replaces the file
    /// by renaming a temp file over it, so a writer that took the lock before
    /// us has left a *different* file at `path` and this handle still names the
    /// one it replaced. Reading by path is what makes the lock protect content
    /// rather than merely serialize.
    ///
    /// The fallback is for Windows, where a second handle onto a file *this*
    /// process has locked is refused outright — the writer locks the plan and
    /// then cannot read it.
    ///
    /// No cooperating writer can replace `path` while this object holds its
    /// sidecar. The handle fallback is therefore authoritative precisely when
    /// this process's own mandatory destination lock refused the path read.
    ///
    /// `action` names the read the way `file_io_report` wants it, so a caller's
    /// diagnostic reads the same as it did when this was `fs::read_to_string`.
    fn read_to_string(&self, action: &str) -> MietteResult<String> {
        let err = match fs::read_to_string(&self.path) {
            Ok(raw) => return Ok(raw),
            Err(err) => err,
        };
        if !lock_is_contended(&err) {
            return Err(file_io_report(&self.path, action, err));
        }
        read_through_handle(&self.file)
            .unwrap_or(Err(err))
            .map_err(|err| file_io_report(&self.path, action, err))
    }

    /// Release only the replaceable destination handle, retaining the sidecar.
    fn release_destination(&self) {
        if let Some(file) = self.file.lock().expect("plan lock handle").take() {
            let _ = fs2::FileExt::unlock(&file);
        }
        held_plan_locks()
            .lock()
            .expect("held plan locks")
            .retain(|(_, handle)| !Arc::ptr_eq(handle, &self.file));
    }

    /// Release the destination and then the stable writer sidecar. Idempotent.
    fn release(&self) {
        self.release_destination();
        if let Some(file) = self.writer_lock.lock().expect("plan writer lock").take() {
            let _ = fs2::FileExt::unlock(&file);
        }
    }
}

impl Drop for LockedPlanFile {
    fn drop(&mut self) {
        self.release();
    }
}

/// Every plan file this process currently holds locked.
///
/// On Windows a byte-range lock belongs to the handle that took it, so a second
/// open of the same file — from this very process — is refused. A command that
/// locks a plan and then hands the *path* to something that reads it therefore
/// reads nothing: `rhei new` locks the plan and then asks the loader to
/// validate it, and the loader knows about paths, not about locks. This is
/// where such a reader finds the handle that already holds the file.
// §FS-rhei-new.4
fn held_plan_locks() -> &'static Mutex<Vec<(PathBuf, PlanLockHandle)>> {
    static HELD_PLAN_LOCKS: OnceLock<Mutex<Vec<(PathBuf, PlanLockHandle)>>> = OnceLock::new();
    HELD_PLAN_LOCKS.get_or_init(|| Mutex::new(Vec::new()))
}

/// The open, locked file, shared between the lock object and the registry.
type PlanLockHandle = Arc<Mutex<Option<fs::File>>>;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanLockEvent {
    Contended,
    Acquired,
}

#[cfg(test)]
thread_local! {
    static PLAN_LOCK_OBSERVER: std::cell::RefCell<Option<mpsc::Sender<PlanLockEvent>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_plan_lock_observer(observer: mpsc::Sender<PlanLockEvent>) {
    PLAN_LOCK_OBSERVER.with(|installed| *installed.borrow_mut() = Some(observer));
}

#[cfg(test)]
fn notify_plan_lock_observer(event: PlanLockEvent) {
    PLAN_LOCK_OBSERVER.with(|installed| {
        let observer = installed.borrow().clone();
        if let Some(observer) = observer {
            let _ = observer.send(event);
        }
        if event == PlanLockEvent::Acquired {
            installed.borrow_mut().take();
        }
    });
}

/// The sibling lock identity for one replaceable plan pathname.
///
/// Canonicalizing only the parent makes relative and absolute spellings agree
/// without resolving a final symlink that an atomic rewrite would replace.
// §AR-agent-orchestrator-workflow.3.3.1
fn plan_lock_path(path: &Path) -> MietteResult<PathBuf> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let parent = rhei_core::platform::canonical_path(parent)
        .map_err(|err| file_io_report(parent, "failed to resolve plan lock directory", err))?;
    let name = path.file_name().ok_or_else(|| {
        miette!("failed to derive plan lock file for {}", path.display())
    })?;
    let mut lock_name = name.to_os_string();
    lock_name.push(".lock");
    Ok(parent.join(lock_name))
}

fn lock_plan_writer(file: &fs::File, path: &Path) -> MietteResult<()> {
    #[cfg(test)]
    if PLAN_LOCK_OBSERVER.with(|installed| installed.borrow().is_some()) {
        match file.try_lock_exclusive() {
            Ok(()) => {
                notify_plan_lock_observer(PlanLockEvent::Acquired);
                return Ok(());
            }
            Err(err) if lock_is_contended(&err) => {
                notify_plan_lock_observer(PlanLockEvent::Contended);
            }
            Err(err) => return Err(file_io_report(path, "failed to acquire plan lock", err)),
        }
    }

    file.lock_exclusive()
        .map_err(|err| file_io_report(path, "failed to acquire plan lock", err))?;
    #[cfg(test)]
    notify_plan_lock_observer(PlanLockEvent::Acquired);
    Ok(())
}

/// Read the whole file behind a lock handle, from the start.
///
/// `None` when the lock has already been released, which leaves the caller with
/// the original refusal to report. That is a rule and not an accident: a
/// released handle names whatever file it named before, and after
/// [`persist_locked`]'s rename that is an orphan nobody can reach by path. A
/// read served from it would be content no writer will ever see again.
fn read_through_handle(handle: &Mutex<Option<fs::File>>) -> Option<std::io::Result<String>> {
    let guard = handle.lock().expect("plan lock handle");
    // `None` once released — the caller keeps its original error. §FS-rhei-new.4
    let file = guard.as_ref()?;
    let mut reader = file;
    Some((|| {
        reader.seek(std::io::SeekFrom::Start(0))?;
        let mut raw = String::new();
        reader.read_to_string(&mut raw)?;
        Ok(raw)
    })())
}

/// Read `path` through whichever lock this process holds on it, if any.
fn read_through_held_lock(path: &Path) -> Option<std::io::Result<String>> {
    let held = {
        let locks = held_plan_locks().lock().expect("held plan locks");
        locks
            .iter()
            .find(|(locked_path, _)| same_path(locked_path, path))
            .map(|(_, handle)| handle.clone())
    }?;
    read_through_handle(&held)
}

/// Read `path`, by path first and through this process's own lock when the path
/// read is the one that lock refuses.
///
/// The ordering is [`LockedPlanFile::read_to_string`]'s, for its reasons: a
/// writer that took the lock before us has left a different file at `path`, and
/// only reading by path sees it. The fallback covers the case that reading by
/// path cannot: the file is one *we* locked, and Windows will not open it twice.
/// A destination handle released for replacement is deregistered and never
/// read through; its stable writer sidecar still excludes other writers.
///
/// The `io::Error` is passed through rather than wrapped, because the loader
/// this is installed into branches on its kind.
// §FS-rhei-new.4
fn plan_source_reader(path: &Path) -> std::io::Result<String> {
    let err = match fs::read_to_string(path) {
        Ok(raw) => return Ok(raw),
        Err(err) => err,
    };
    if !lock_is_contended(&err) {
        return Err(err);
    }
    read_through_held_lock(path).unwrap_or(Err(err))
}

/// [`plan_source_reader`] with the diagnostic a CLI caller wants; `action` names
/// the read the way `file_io_report` does.
// §FS-rhei-new.4
fn read_plan_source(path: &Path, action: &str) -> MietteResult<String> {
    plan_source_reader(path).map_err(|err| file_io_report(path, action, err))
}

/// Rename a temp file over `path`, which `locked` may be holding.
///
/// A mandatory destination lock may refuse the first replace. Release only
/// that replaceable handle and retry once; [`LockedPlanFile::writer_lock`]
/// continues excluding every cooperating writer across the retry.
// §AR-agent-orchestrator-workflow.3.3.1
fn persist_locked(
    tmp: tempfile::NamedTempFile,
    path: &Path,
    locked: Option<&LockedPlanFile>,
) -> Result<(), tempfile::PersistError> {
    let refused = match tmp.persist(path) {
        Ok(_) => return Ok(()),
        Err(refused) => refused,
    };
    let Some(locked) = locked else {
        return Err(refused);
    };
    locked.release_destination();
    refused.file.persist(path).map(|_| ())
}

/// Whether a failed try-lock — or a read a lock refused — means *somebody
/// already holds it*, as opposed to failing for a reason that says nothing
/// about the holder.
///
/// The platforms disagree on the errno: Unix refuses a contended `flock` with
/// `EWOULDBLOCK`, Windows refuses a contended `LockFileEx` with
/// `ERROR_LOCK_VIOLATION` (os error 33), which is not `WouldBlock`. Classifying
/// only `WouldBlock` as held reported every live Windows run as *unknown*.
/// `fs2::lock_contended_error()` is the platform's own answer, so both spell
/// the same verdict here.
// §FS-rhei-run-headless.3
fn lock_is_contended(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    match (err.raw_os_error(), fs2::lock_contended_error().raw_os_error()) {
        (Some(observed), Some(contended)) => observed == contended,
        _ => false,
    }
}
