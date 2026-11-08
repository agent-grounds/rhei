// The stable writer lock every plan-rewriting command takes, the replacement it
// makes under that lock, and the one classification every `fs2` try-lock in
// this crate depends on.
//
// Its own part because these are platform facts rather than command behavior,
// and the commands that got them wrong — `rhei complete`, `rhei transition`,
// `rhei reset`, a dashboard gate choice, the run-lock liveness probe, the
// headless launch lock, snapshot-continue — had each gone their own way.

// §AR-source-file-size.3 §AR-agent-orchestrator-workflow.3.3.1
// §AR-agent-orchestrator-workflow.3.3.1.1

/// A plan pathname held under its sole stable writer lock.
///
/// The lock is a permanent sibling sidecar, so replacing the destination never
/// changes the identity held by writers or prevents a current-path read.
// §FS-rhei-transition-cmd.3
struct LockedPlanFile {
    /// Retained across ordinary writes, including direct engine entry. §FS-rhei-recover.4
    _root_guard: Vec<rhei_core::root_access::RootAccessGuard>,
    writer_lock: Mutex<Option<fs::File>>,
    path: PathBuf,
    /// Whether the last read set aside a leading byte order mark, which every
    /// write under this lock puts back. §REQ-cross-platform.9
    byte_order_mark: std::sync::atomic::AtomicBool,
}

/// Open a lock sidecar, creating it where it is absent and leaving its bytes alone: the
/// one way every lock opens one, and the way a reset asks whether it could.
/// §FS-rhei-reset.2.3
fn open_lock_sidecar(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(path)
}

impl LockedPlanFile {
    /// Take the stable sidecar without opening the destination it protects.
    ///
    /// This also establishes writer exclusion before an absent destination's
    /// first publication. A waiter reads the current pathname only after the
    /// prior writer's last replacement or rollback.
    // §AR-agent-orchestrator-workflow.3.3.1 §FS-rhei-new.4
    fn open(path: &Path) -> MietteResult<Self> {
        // §FS-rhei-errors.6
        let root_guard = rhei_core::root_access::for_file(path).map_err(|err| root_access_report(path, err))?;
        let lock_path = plan_lock_path(path)?;
        let writer_lock = open_lock_sidecar(&lock_path)
            .map_err(|err| file_io_report(&lock_path, "failed to open plan lock file", err))?;
        lock_plan_writer(&writer_lock, &lock_path)?;
        Ok(Self {
            _root_guard: root_guard,
            writer_lock: Mutex::new(Some(writer_lock)),
            path: path.to_path_buf(),
            byte_order_mark: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Read the authoritative current destination pathname under the sidecar.
    ///
    /// `action` names the read the way `file_io_report` wants it, so a caller's
    /// diagnostic reads the same as it did when this was `fs::read_to_string`.
    ///
    /// The text is the authored file's, with the one leading byte order mark an
    /// editor may have saved set aside, so every rewrite that finds a header,
    /// frontmatter or task heading in it sees what the parser saw. The lock
    /// remembers the mark and [`Self::with_byte_order_mark`] puts it back on
    /// write, so a rewrite does not re-encode the file. §REQ-cross-platform.9
    // §FS-rhei-transition-cmd.3
    fn read_to_string(&self, action: &str) -> MietteResult<String> {
        let raw = fs::read_to_string(&self.path)
            .map_err(|err| file_io_report(&self.path, action, err))?;
        let text = rhei_core::source::without_byte_order_mark(&raw);
        let marked = text.len() != raw.len();
        self.byte_order_mark.store(marked, std::sync::atomic::Ordering::Relaxed);
        Ok(if marked { text.to_owned() } else { raw })
    }

    /// `content` as it is written under this lock: with the byte order mark
    /// the last read set aside put back in front. §REQ-cross-platform.9
    fn with_byte_order_mark<'a>(&self, content: &'a str) -> std::borrow::Cow<'a, str> {
        if self.byte_order_mark.load(std::sync::atomic::Ordering::Relaxed) {
            std::borrow::Cow::Owned(format!("\u{feff}{content}"))
        } else {
            std::borrow::Cow::Borrowed(content)
        }
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
    // A plan staged to be loaded before it is written reads as staged. §FS-rhei-reset.2.3
    if let Some(text) = rhei_core::source::staged(path) {
        return Ok(text);
    }
    fs::read_to_string(path).map_err(|err| file_io_report(path, action, err))
}

/// `raw` split into the one leading byte order mark it may carry and the text after
/// it, for a writer that scans a plan file it read without its lock and must hand
/// back the file's own bytes. §REQ-cross-platform.9
fn split_byte_order_mark(raw: &str) -> (&str, &str) {
    let text = rhei_core::source::without_byte_order_mark(raw);
    (&raw[..raw.len() - text.len()], text)
}

/// Rename a temp file over the unlocked destination pathname, waiting out a
/// refusal by another open handle - a reader's included - for a bounded time.
///
/// Every attempt renames the same staged file, which a refused attempt hands
/// back, and the caller's sidecar stays held across all of them. A failure that
/// is not a refusal, or a refusal that outlasts `rhei_core::open_handle::PATIENCE`,
/// is returned as the last attempt met it, the destination untouched.
// §AR-agent-orchestrator-workflow.3.3.1 §AR-agent-orchestrator-workflow.3.3.1.1
fn persist_locked(tmp: tempfile::NamedTempFile, path: &Path) -> Result<(), tempfile::PersistError> {
    let wait = rhei_core::open_handle::Wait::start();
    let mut staged = tmp;
    loop {
        match replace_once(staged, path) {
            Err(refused) if wait.again(&refused.error) => staged = refused.file,
            outcome => return outcome,
        }
    }
}

/// One replacement attempt: the rename a platform may refuse while another
/// handle holds the destination, and so the unit every retry repeats.
// §AR-agent-orchestrator-workflow.3.3.1.1
fn replace_once(tmp: tempfile::NamedTempFile, path: &Path) -> Result<(), tempfile::PersistError> {
    #[cfg(test)]
    if let Some(error) = take_replace_refusal() {
        return Err(tempfile::PersistError { error, file: tmp });
    }
    tmp.persist(path).map(|_| ())
}

/// Refusals a test arranges for this thread's next replacement attempts, in
/// place of the open handle that makes Windows refuse a rename, so the waiting
/// out is pinned on every platform. §AR-agent-orchestrator-workflow.3.3.1.1
#[cfg(test)]
struct ReplaceRefusals {
    /// Attempts still to refuse; `None` refuses every attempt.
    remaining: Option<usize>,
    /// The error each refused attempt reports.
    refusal: fn() -> std::io::Error,
    /// Every attempt made while the refusals were installed, refused or not.
    attempts: usize,
}

#[cfg(test)]
thread_local! {
    static REPLACE_REFUSALS: std::cell::RefCell<Option<ReplaceRefusals>> =
        const { std::cell::RefCell::new(None) };
}

/// Refuse this thread's next `count` replacement attempts (`None`: all of them)
/// with `refusal`, counting every attempt until `replace_attempts`.
#[cfg(test)]
fn refuse_replacements(count: Option<usize>, refusal: fn() -> std::io::Error) {
    REPLACE_REFUSALS.with(|installed| {
        *installed.borrow_mut() = Some(ReplaceRefusals { remaining: count, refusal, attempts: 0 })
    });
}

/// Remove the arranged refusals and say how many attempts were made under them.
#[cfg(test)]
fn replace_attempts() -> usize {
    REPLACE_REFUSALS.with(|installed| installed.borrow_mut().take().map_or(0, |r| r.attempts))
}

#[cfg(test)]
fn take_replace_refusal() -> Option<std::io::Error> {
    REPLACE_REFUSALS.with(|installed| {
        let mut installed = installed.borrow_mut();
        let refusals = installed.as_mut()?;
        refusals.attempts += 1;
        match refusals.remaining.as_mut() {
            Some(0) => None,
            Some(remaining) => {
                *remaining -= 1;
                Some((refusals.refusal)())
            }
            None => Some((refusals.refusal)()),
        }
    })
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
