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
    static HELD: RefCell<BTreeMap<PathBuf, Weak<Hold>>> = RefCell::new(BTreeMap::new());
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
    let marker = root.join(MARKER);
    let bytes = match fs::read(&marker) {
        Ok(bytes) => bytes,
        Err(err)
            if err.kind() == io::ErrorKind::NotFound && fs::symlink_metadata(&marker).is_err() =>
        {
            return Ok(())
        }
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

/// Acquire a complete root set in canonical order before returning data. §FS-rhei-panta.6.6
pub fn shared_roots(roots: impl IntoIterator<Item = PathBuf>) -> io::Result<Vec<RootAccessGuard>> {
    let mut roots = roots
        .into_iter()
        .map(|p| crate::platform::canonical_path(&p))
        .collect::<io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    roots.iter().map(|root| RootAccessGuard::shared(root)).collect()
}

/// Discover only root identities before acquiring guards, never task bytes. §FS-rhei-panta.6.6
pub fn for_input(path: &Path) -> io::Result<Vec<RootAccessGuard>> {
    let root = if path.is_dir() { path } else { crate::workspace::plan_parent_dir(path) };
    let mut roots = vec![root.to_path_buf()];
    check_pending(root)?;
    if root.join(crate::workspace::PANTA_INDEX_FILE).is_file() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir()
                && (path.join(crate::workspace::RHEI_INDEX_FILE).is_file()
                    || path.file_name().is_some_and(|name| name == "basin"))
            {
                roots.push(path);
            }
        }
    }
    shared_roots(roots)
}

/// Direct file consumers locate their owning workspace before taking access. §FS-rhei-recover.4
pub fn for_file(path: &Path) -> io::Result<RootAccessGuard> {
    let parent = crate::workspace::plan_parent_dir(path);
    let root = parent
        .ancestors()
        .find(|dir| {
            dir.join(MARKER).exists()
                || dir.join(crate::workspace::RHEI_INDEX_FILE).is_file()
                || dir.join(crate::workspace::PANTA_INDEX_FILE).is_file()
                || dir.join(".rhei/run.lock").is_file()
                || fs::read_dir(dir).is_ok_and(|entries| {
                    entries.flatten().any(|entry| {
                        entry.file_name().to_str().is_some_and(|name| name.ends_with(".rhei.md"))
                    })
                })
        })
        .unwrap_or(parent);
    RootAccessGuard::shared(root)
}
