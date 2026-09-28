//! Operation-lifetime exclusion for interrupted operator recovery. §FS-rhei-recover.4

use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};

pub const MARKER: &str = ".rhei/forced-recovery.json";

/// The one lever a caller has over where root guards live, named wherever a
/// guard cannot be taken. §FS-rhei-errors.1.5
const LEVER: &str = "rhei keeps root guards in the account's state directory; \
                     set XDG_STATE_HOME to a writable directory.";

/// Where a degraded root guard says what it cost. `rhei-core` never writes to
/// the terminal a live run owns, so the CLI substitutes the run's own sink.
/// §FS-rhei-recover.4.1
pub type WarningSink = Box<dyn Fn(&str) + Send + Sync>;

static WARNING_SINK: RwLock<Option<WarningSink>> = RwLock::new(None);

// Canonical roots already warned about. The `HELD` cache cannot carry this: it
// is thread-local and holds `Weak`s, so one root would warn more than once.
// §FS-rhei-recover.4.1
static WARNED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// Install the sink a degradation warning is written through, or restore the
/// default of one line on stderr. §FS-rhei-recover.4.1
pub fn set_warning_sink(sink: Option<WarningSink>) {
    *WARNING_SINK.write().unwrap_or_else(|poison| poison.into_inner()) = sink;
}

/// One warning per canonical root per process; a lock that is taken is silent.
/// §FS-rhei-recover.4.1
fn warn_once(root: &Path, text: String) {
    {
        let mut warned = WARNED.lock().unwrap_or_else(|poison| poison.into_inner());
        if !warned.insert(root.to_path_buf()) {
            return;
        }
    }
    let sink = WARNING_SINK.read().unwrap_or_else(|poison| poison.into_inner());
    match sink.as_ref() {
        Some(sink) => sink(&text),
        None => eprintln!("{text}"),
    }
}

/// A guard that could not be taken, holding the subjects it measured: the lock
/// it would have created, the execution root it was for, and the error itself.
/// §FS-rhei-errors.1.5
struct GuardFailure {
    kind: io::ErrorKind,
    lock: String,
    root: String,
    cause: String,
}

impl GuardFailure {
    fn new(lock: Option<&Path>, root: &Path, error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            lock: lock.map_or_else(
                || "in the account state directory".to_owned(),
                |lock| lock.display().to_string(),
            ),
            root: root.display().to_string(),
            cause: error.to_string(),
        }
    }

    /// Exclusive acquisition never degrades, and its refusal names its subject.
    /// The breaks give the lock, the root and the lever a line each to open, so
    /// a rendered diagnostic reads as three subjects rather than one paragraph;
    /// they do not bound how the renderer wraps a line that is still too long.
    /// §FS-rhei-recover.4.1 §FS-rhei-errors.1.5
    fn refusal(&self) -> io::Error {
        io::Error::new(
            self.kind,
            format!(
                "cannot create root guard lock {}\nfor execution root {}: {}\n{LEVER}",
                self.lock, self.root, self.cause
            ),
        )
    }

    /// A shared acquisition proceeds, and says once what the account cost it.
    /// One line, because a warning is written to stderr rather than rendered.
    /// §FS-rhei-recover.4.1
    fn warning(&self) -> String {
        format!(
            "warning: cannot create root guard lock {} for execution root {}: {}; proceeding \
             without exclusion between rhei processes on that root. {LEVER}",
            self.lock, self.root, self.cause
        )
    }
}

/// Clones retain the same OS lock through derived reads and writes. §FS-rhei-panta.6.6
#[derive(Clone, Debug)]
pub struct RootAccessGuard(Arc<Hold>);

#[derive(Debug)]
struct Hold {
    // `None` is a hold the account could not lock: guarded or unguarded is a
    // property of the hold, never of one nested load. §FS-rhei-recover.4.1
    file: Option<File>,
    exclusive: bool,
}

thread_local! {
    // Re-entrant loads under this thread's exclusive transaction do not relock.
    static HELD: RefCell<BTreeMap<PathBuf, Weak<Hold>>> = const { RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
thread_local! {
    static CONTENDED: RefCell<Option<std::sync::mpsc::Sender<()>>> = const { RefCell::new(None) };
    // `lock_path` reads the environment on every call and the test binary is one
    // process, so a case that needs its own account state directory overrides the
    // base here rather than racing every other test through `XDG_STATE_HOME`.
    static LOCK_BASE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    // Opens to fail with `Interrupted` before the real attempt runs.
    static INTERRUPTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl Drop for Hold {
    fn drop(&mut self) {
        // An unguarded hold has no OS lock to release. §FS-rhei-recover.4.1
        if let Some(file) = &self.file {
            let _ = FileExt::unlock(file);
        }
    }
}

/// A marker always outranks authored-input lenience. §FS-rhei-recover.4
pub fn check_pending(root: &Path) -> io::Result<()> {
    let mut inaccessible = None;
    for owner in pending_roots(root) {
        if let Err(error) = check_marker(&owner) {
            if error.kind() == io::ErrorKind::Other {
                return Err(error);
            }
            inaccessible.get_or_insert(error);
        }
    }
    inaccessible.map_or(Ok(()), Err)
}

fn check_marker(root: &Path) -> io::Result<()> {
    let marker = root.join(MARKER);
    // An unsearchable parent cannot establish marker presence or absence. §FS-rhei-run-headless.3
    match fs::symlink_metadata(&marker) {
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(io::Error::new(
                error.kind(),
                format!("cannot check recovery marker {}: {error}", marker.display()),
            ))
        }
    }
    // Once existence is established, even unreadable contents mean refusal. §FS-rhei-recover.4
    let bytes = match fs::read(&marker) {
        Ok(bytes) => bytes,
        Err(err) => {
            return Err(io::Error::other(format!(
            "forced recovery pending: ? ? -> ?; marker {} is unreadable: {err}\nrhei recover {}",
            marker.display(), crate::platform::shell_quote(&root.display().to_string())
        )))
        }
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let hop = &value["hop"];
    let field = |key| hop[key].as_str().unwrap_or("?");
    Err(io::Error::other(format!(
        "forced recovery pending: {} {} -> {}; marker {}\nrhei recover {}",
        field("task_id"),
        field("from"),
        field("to"),
        marker.display(),
        crate::platform::shell_quote(&root.display().to_string())
    )))
}

/// Lock storage belongs to the account, never to a read-only project. §FS-rhei-recover.4
fn lock_path(root: &Path) -> io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(base) = LOCK_BASE.with(|base| base.borrow().clone()) {
        return Ok(guard_lock(&base, root));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            if cfg!(windows) {
                std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
            } else {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            }
        })
        .ok_or_else(|| io::Error::other("cannot locate rhei platform state directory"))?;
    Ok(guard_lock(&base, root))
}

/// Naming the lock is separate from creating it, so a guard that fails can say
/// which lock it failed on. §FS-rhei-errors.1.5
fn guard_lock(base: &Path, root: &Path) -> PathBuf {
    let key = format!("{:x}", Sha256::digest(root.as_os_str().as_encoded_bytes()));
    base.join("rhei/root-guards").join(format!("{key}.lock"))
}

/// Locate, create and open a root's lock. All three failing are one verdict —
/// the account cannot hold this lock — rather than a list of error codes, which
/// is the distinction this reads the same way on every platform.
/// §FS-rhei-recover.4.1 §REQ-cross-platform
fn open_lock(root: &Path) -> Result<File, GuardFailure> {
    let path = lock_path(root).map_err(|error| GuardFailure::new(None, root, &error))?;
    let attempt = || {
        #[cfg(test)]
        if INTERRUPTS.with(std::cell::Cell::get) > 0 {
            INTERRUPTS.with(|left| left.set(left.get() - 1));
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        fs::create_dir_all(path.parent().expect("a lock path has a guard directory"))?;
        OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)
    };
    // A signal is not a verdict about the filesystem. §FS-rhei-recover.5
    match attempt() {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => attempt(),
        outcome => outcome,
    }
    .map_err(|error| GuardFailure::new(Some(&path), root, &error))
}

impl RootAccessGuard {
    /// Double-check around shared acquisition; callers retain the guard. §FS-rhei-recover.4
    pub fn shared(root: &Path) -> io::Result<Self> {
        Self::acquire(root, false, false).map(|guard| guard.expect("blocking acquisition"))
    }

    /// Only the attended coordinator may inspect a marker under this hold. §FS-rhei-recover.3
    pub fn exclusive(root: &Path) -> io::Result<Self> {
        Self::acquire(root, true, false).map(|guard| guard.expect("blocking acquisition"))
    }

    /// A nonblocking probe allows deterministic contention observation. §FS-rhei-recover.5
    pub fn try_exclusive(root: &Path) -> io::Result<Option<Self>> {
        Self::acquire(root, true, true)
    }

    fn acquire(root: &Path, exclusive: bool, try_only: bool) -> io::Result<Option<Self>> {
        let root = crate::platform::canonical_path(root)?;
        let held = HELD.with(|held| held.borrow().get(&root).and_then(Weak::upgrade));
        if let Some(held) = held {
            if exclusive && !held.exclusive {
                if try_only {
                    return Ok(None);
                }
                return Err(io::Error::other(
                    "release shared root access before exclusive recovery",
                ));
            }
            if !exclusive {
                check_pending(&root)?;
            }
            return Ok(Some(Self(held)));
        }
        if !exclusive {
            check_pending(&root)?;
        }
        let file = match open_lock(&root) {
            Ok(file) => Some(file),
            // Publishing or resolving a marker without exclusion is worse than
            // refusing, and a probe reports contention only. §FS-rhei-recover.4.1
            Err(failure) if exclusive || try_only => return Err(failure.refusal()),
            Err(failure) => {
                warn_once(&root, failure.warning());
                None
            }
        };
        if let Some(file) = &file {
            #[cfg(test)]
            {
                let attempt = if exclusive {
                    file.try_lock_exclusive()
                } else {
                    FileExt::try_lock_shared(file)
                };
                if attempt.is_err() {
                    CONTENDED.with(|sender| {
                        if let Some(sender) = sender.borrow().as_ref() {
                            let _ = sender.send(());
                        }
                    });
                } else {
                    FileExt::unlock(file)?;
                }
            }
            if try_only {
                match file.try_lock_exclusive() {
                    Ok(()) => (),
                    Err(err)
                        if err.kind() == io::ErrorKind::WouldBlock
                            || err.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                    {
                        return Ok(None)
                    }
                    Err(err) => return Err(err),
                }
            } else if exclusive {
                file.lock_exclusive()?;
            } else {
                FileExt::lock_shared(file)?;
            }
        }
        let guard = Self(Arc::new(Hold { file, exclusive }));
        if !exclusive {
            check_pending(&root)?;
        }
        HELD.with(|held| held.borrow_mut().insert(root, Arc::downgrade(&guard.0)));
        Ok(Some(guard))
    }
}

#[cfg(test)]
#[path = "root_access_tests.rs"]
mod tests;

#[path = "root_access_discovery.rs"]
mod discovery;
use discovery::pending_roots;
pub use discovery::{for_file, for_input, input_roots, shared_roots};
