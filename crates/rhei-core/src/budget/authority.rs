//! An external committed-history witness, so that a lost tail is
//! distinguishable from a fresh project.
//!
//! The witness is not a second spendable balance. It exists so that deleting
//! the journal cannot recreate capacity; deleting the journal **and** the
//! witness is the stated residual of this design.
//! §FS-rhei-budgets.5.3 §AR-neural-admission.4

use super::journal::sync_directory;
use super::types::BudgetError;
use super::Result;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) struct Authority {
    _lock: File,
    path: PathBuf,
    bytes: Vec<u8>,
    /// True when this machine had no witness for a journal that verifies, and
    /// one was written from the journal's own bytes. §FS-rhei-budgets.5.4
    adopted: bool,
}

impl Authority {
    /// `$XDG_STATE_HOME/rhei/budget-authority/<uuid>/`, falling back to
    /// `$HOME/.local/state` (or `%USERPROFILE%` on Windows).
    /// §FS-rhei-budgets.5.3
    pub(crate) fn lock(root: &Path, uuid: &str) -> Result<Self> {
        Self::lock_at(root, &authority_base()?, uuid)
    }

    pub(crate) fn lock_at(root: &Path, base: &Path, uuid: &str) -> Result<Self> {
        super::types::uuid(uuid)?;
        if !base.is_absolute() {
            return Err(BudgetError::corrupt("budget authority directory must be absolute"));
        }
        // The plain spelling, as `account::canonical` already resolves the same
        // root with: a verbatim path is a second spelling of one location, and
        // two spellings of the project root inside one module make the guard
        // below stop matching. §REQ-cross-platform.5
        let root = crate::platform::canonical_path(root)
            .map_err(|error| BudgetError::unreachable(root, &error))?;
        // Resolve existing ancestors before creating anything, so a symlink
        // pointing back at the account is seen for what it is.
        // §FS-rhei-budgets.5.3
        let resolved = resolve_existing(base)?;
        // What a witness must not be is the *journal*: a chain verifying
        // against itself verifies nothing. Where an operator's state directory
        // sits is otherwise theirs. §FS-rhei-budgets.5.3 §REQ-bounded-neural-work.2
        if resolved.starts_with(root.join(super::account::ACCOUNT_DIR)) {
            return Err(BudgetError::corrupt(
                "the budget authority cannot live inside the account it witnesses",
            ));
        }
        // Created whatever the caller came for: an adopted journal must be
        // able to write a witness this machine never had, and an empty
        // directory is not capacity. §FS-rhei-budgets.5.4
        let dir = resolved.join("rhei/budget-authority").join(uuid);
        durable_directories(&dir)?;
        let lock_path = dir.join("history.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| BudgetError::unreachable(&lock_path, &error))?;
        lock.lock_exclusive()?;
        let path = dir.join("history.jsonl");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(BudgetError::unreachable(&path, &e)),
        };
        Ok(Self { _lock: lock, path, bytes, adopted: false })
    }

    /// The journal and the witness must be the same bytes.
    ///
    /// A witness **ahead** of the journal is the damaged state: the journal is
    /// absent, truncated, or was rolled back, and new work is refused naming
    /// both paths. A journal with **no** witness on this machine is adopted
    /// instead — see [`Self::adopt`]. §FS-rhei-budgets.5.4
    pub(crate) fn verify(&self, bytes: &[u8], journal: &Path) -> Result<()> {
        if self.bytes == bytes {
            return Ok(());
        }
        Err(BudgetError::corrupt(format!(
            "the committed history at {} does not match the project journal at {}; \
             restore the journal by copying the witness back over it",
            self.path.display(),
            journal.display()
        )))
    }

    /// Take a verifying journal that this machine has never witnessed: a
    /// project cloned from git, or the same project on a new machine.
    ///
    /// This mints nothing, because the consumed counts travelled with the
    /// journal. It is a deliberate loosening of the money path this was carved
    /// from, where refusal was the safer default; for counts, refusing every
    /// fresh clone would refuse work that runs today.
    /// §FS-rhei-budgets.5.4 §REQ-bounded-neural-work.2
    pub(crate) fn adopt(&mut self, bytes: &[u8]) -> Result<()> {
        if !self.bytes.is_empty() {
            return Err(BudgetError::corrupt("cannot adopt over an existing witness"));
        }
        self.write_initial(bytes)?;
        self.adopted = true;
        Ok(())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub(crate) fn adopted(&self) -> bool {
        self.adopted
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The witness is written first and the journal second, so a crash between
    /// them leaves the witness ahead — the damaged state, which refuses rather
    /// than silently losing a receipt. §AR-neural-admission.4
    pub(crate) fn append(&mut self, line: &[u8]) -> Result<()> {
        if self.bytes.is_empty() {
            return self.write_initial(line);
        }
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line)?;
        file.sync_all()?;
        sync_directory(self.path.parent().expect("authority has a parent"))?;
        self.bytes.extend_from_slice(line);
        Ok(())
    }

    fn write_initial(&mut self, bytes: &[u8]) -> Result<()> {
        let pending = self.path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&pending)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&pending, &self.path)?;
        sync_directory(self.path.parent().expect("authority has a parent"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

/// Where this process keeps its witnesses, resolved from the environment once.
///
/// Once, because the witness location is a property of the run rather than of
/// the moment a spawn is admitted: a process that re-read the environment could
/// write two different witnesses for one account, and one whose environment is
/// rewritten underneath it — a test harness moving `HOME` in another thread —
/// would decide where an unrelated account's witness lives. §FS-rhei-budgets.5.3
/// Only a resolution that succeeded is remembered: an environment with neither
/// variable set is a condition of the moment, and caching it would refuse every
/// later admission of the process on the strength of one.
pub(super) fn authority_base() -> Result<PathBuf> {
    static BASE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    if let Some(base) = BASE.get() {
        return Ok(base.clone());
    }
    let resolved = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".local/state"))
        })
        .ok_or_else(|| {
            BudgetError::new(
                "unreachable_budget_path",
                "no external budget authority directory: set XDG_STATE_HOME or HOME",
            )
        })?;
    Ok(BASE.get_or_init(|| resolved).clone())
}

/// Resolve the longest existing prefix of `base` and re-attach the rest.
///
/// A prefix that goes away between being tested for and being resolved is the
/// same case as one that was never there, so this climbs again rather than
/// reporting a path that is merely absent — a state directory Rhei is about to
/// create is not a damaged account. §FS-rhei-budgets.5.3 §FS-rhei-budgets.5.4
///
/// The resolved prefix is the plain spelling, because this path is printed in
/// the refusal of §FS-rhei-budgets.5.4 and joined against: a verbatim base
/// rewrites the separators of every component pushed onto it, so the witness
/// was reported in a spelling no other line of a run uses.
/// §REQ-cross-platform.5
fn resolve_existing(base: &Path) -> Result<PathBuf> {
    let mut ancestor = base;
    loop {
        match crate::platform::canonical_path(ancestor) {
            Ok(resolved) => {
                let rest = base.strip_prefix(ancestor).map_err(BudgetError::corrupt)?;
                return Ok(resolved.join(rest));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor =
                    ancestor.parent().ok_or_else(|| BudgetError::unreachable(base, &error))?;
            }
            Err(error) => return Err(BudgetError::unreachable(ancestor, &error)),
        }
    }
}

/// Persist newly created ancestors as well as the leaf: a directory entry that
/// is not synced is a directory that can vanish under a crash.
/// §FS-rhei-budgets.5.1
pub(crate) fn durable_directories(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let parent = path.parent().ok_or_else(|| BudgetError::corrupt("invalid directory"))?;
    durable_directories(parent)?;
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(e) => return Err(BudgetError::unreachable(path, &e)),
    }
    sync_directory(path)?;
    sync_directory(parent)
}
