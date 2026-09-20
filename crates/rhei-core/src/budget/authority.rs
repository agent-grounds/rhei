//! An external committed-history witness prevents project copies and lost tails
//! from manufacturing a new balance. §FS-rhei-budgets.3.1 §FS-rhei-budgets.3.2

use super::journal::sync_directory;
use super::{BudgetError, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) struct Authority {
    _lock: File,
    path: PathBuf,
    bytes: Vec<u8>,
}

impl Authority {
    pub(crate) fn lock(root: &Path, uuid: &str, create: bool) -> Result<Self> {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(|home| PathBuf::from(home).join(".local/state"))
            })
            .ok_or_else(|| BudgetError::corrupt("no external budget authority directory"))?;
        Self::lock_at(root, &base, uuid, create)
    }

    pub(super) fn lock_at(root: &Path, base: &Path, uuid: &str, create: bool) -> Result<Self> {
        super::types::uuid(uuid)?;
        if !base.is_absolute() {
            return Err(BudgetError::corrupt("budget authority directory must be absolute"));
        }
        let root = std::fs::canonicalize(root)?;
        // Resolve existing ancestors before creating anything, including a
        // symlink pointing back into a project. §FS-rhei-budgets.3.1
        let mut ancestor = base;
        while !ancestor.exists() {
            ancestor =
                ancestor.parent().ok_or_else(|| BudgetError::corrupt("invalid authority path"))?;
        }
        let resolved = std::fs::canonicalize(ancestor)?
            .join(base.strip_prefix(ancestor).map_err(BudgetError::corrupt)?);
        if resolved.starts_with(&root) {
            return Err(BudgetError::corrupt("budget authority must live outside the project"));
        }
        let dir = resolved.join("rhei/budget-authority").join(uuid);
        if create {
            durable_directories(&dir)?;
        }
        let lock = OpenOptions::new()
            .create(create)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("history.lock"))?;
        lock.lock_exclusive()?;
        let path = dir.join("history.jsonl");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        if !create && bytes.is_empty() {
            return Err(BudgetError::corrupt("missing committed history witness"));
        }
        Ok(Self { _lock: lock, path, bytes })
    }

    pub(crate) fn verify(&self, bytes: &[u8]) -> Result<()> {
        if self.bytes != bytes {
            return Err(BudgetError::corrupt("journal differs from authoritative committed history; recover the complete witnessed copy"));
        }
        Ok(())
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn append(&mut self, line: &[u8]) -> Result<()> {
        if self.bytes.is_empty() {
            let pending = self.path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
            let mut file = OpenOptions::new().write(true).create_new(true).open(&pending)?;
            file.write_all(line)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&pending, &self.path)?;
            sync_directory(self.path.parent().expect("authority has a parent"))?;
            self.bytes.extend_from_slice(line);
            return Ok(());
        }
        let mut options = OpenOptions::new();
        options.append(true).write(true);
        let mut file = options.open(&self.path)?;
        file.write_all(line)?;
        file.sync_all()?;
        sync_directory(self.path.parent().expect("authority has a parent"))?;
        self.bytes.extend_from_slice(line);
        Ok(())
    }
}

/// Persist newly created ancestors as well as the leaf. §FS-rhei-budgets.3.1
pub(crate) fn durable_directories(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let parent = path.parent().ok_or_else(|| BudgetError::corrupt("invalid directory"))?;
    durable_directories(parent)?;
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
        Err(e) => return Err(e.into()),
    }
    sync_directory(path)?;
    sync_directory(parent)
}
