//! Operation-lifetime exclusion for interrupted operator recovery. §FS-rhei-recover.4

use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

pub const MARKER: &str = ".rhei/forced-recovery.json";

/// Clones retain the same OS lock through derived reads and writes. §FS-rhei-panta.6.6
#[derive(Clone, Debug)]
pub struct RootAccessGuard(Arc<Hold>);

#[derive(Debug)]
struct Hold {
    file: File,
    exclusive: bool,
}

thread_local! {
    // Re-entrant loads under this thread's exclusive transaction do not relock.
    static HELD: RefCell<BTreeMap<PathBuf, Weak<Hold>>> = const { RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
thread_local! {
    static CONTENDED: RefCell<Option<std::sync::mpsc::Sender<()>>> = const { RefCell::new(None) };
}

impl Drop for Hold {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
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
    let directory = base.join("rhei/root-guards");
    fs::create_dir_all(&directory)?;
    let key = format!("{:x}", Sha256::digest(root.as_os_str().as_encoded_bytes()));
    Ok(directory.join(format!("{key}.lock")))
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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path(&root)?)?;
        #[cfg(test)]
        {
            let attempt =
                if exclusive { file.try_lock_exclusive() } else { FileExt::try_lock_shared(&file) };
            if attempt.is_err() {
                CONTENDED.with(|sender| {
                    if let Some(sender) = sender.borrow().as_ref() {
                        let _ = sender.send(());
                    }
                });
            } else {
                FileExt::unlock(&file)?;
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
            FileExt::lock_shared(&file)?;
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
