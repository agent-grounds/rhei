// The stable writer lock every plan-rewriting command takes, and the one
// classification every `fs2` try-lock in this crate depends on.
//
// Its own part because both are platform facts rather than command behavior,
// and the commands that got them wrong — `rhei complete`, `rhei transition`,
// `rhei reset`, a dashboard gate choice, the run-lock liveness probe, the
// headless launch lock, snapshot-continue — had each gone their own way.

// §AR-source-file-size.3 §AR-agent-orchestrator-workflow.3.3.1

/// A plan pathname held under its sole stable writer lock.
///
/// The lock is a permanent sibling sidecar, so replacing the destination never
/// changes the identity held by writers or prevents a current-path read.
// §FS-rhei-transition-cmd.3
struct LockedPlanFile {
    writer_lock: Mutex<Option<fs::File>>,
    path: PathBuf,
}

impl LockedPlanFile {
    /// Take the stable sidecar without opening the destination it protects.
    ///
    /// This also establishes writer exclusion before an absent destination's
    /// first publication. A waiter reads the current pathname only after the
    /// prior writer's last replacement or rollback.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-new.4
    fn open(path: &Path) -> MietteResult<Self> {
        let lock_path = plan_lock_path(path)?;
        let writer_lock = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|err| file_io_report(&lock_path, "failed to open plan lock file", err))?;
        lock_plan_writer(&writer_lock, &lock_path)?;
        Ok(Self { writer_lock: Mutex::new(Some(writer_lock)), path: path.to_path_buf() })
    }

    /// Read the authoritative current destination pathname under the sidecar.
    ///
    /// `action` names the read the way `file_io_report` wants it, so a caller's
    /// diagnostic reads the same as it did when this was `fs::read_to_string`.
    // §FS-rhei-transition-cmd.3
    fn read_to_string(&self, action: &str) -> MietteResult<String> {
        fs::read_to_string(&self.path)
            .map_err(|err| file_io_report(&self.path, action, err))
    }

    /// Release the stable writer sidecar. Idempotent.
    fn release(&self) {
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
        miette!(help = "the plan path must name a file", "failed to derive plan lock file for {}", path.display())
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

/// Read a plan source by its current pathname. The public core reader hook is
/// retained for API compatibility, but CLI writers no longer install a private
/// held-handle fallback because the destination itself is never locked.
// §FS-rhei-transition-cmd.3
fn read_plan_source(path: &Path, action: &str) -> MietteResult<String> {
    fs::read_to_string(path).map_err(|err| file_io_report(path, action, err))
}

/// Rename a temp file over the unlocked destination pathname.
// §AR-agent-orchestrator-workflow.3.3.1
fn persist_locked(tmp: tempfile::NamedTempFile, path: &Path) -> Result<(), tempfile::PersistError> {
    tmp.persist(path).map(|_| ())
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
