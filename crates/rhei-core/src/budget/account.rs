//! Resolving a project to its one account, and establishing an absent one.
//!
//! The account is per **project** and lives outside `runtime/`: a count that
//! lived under an execution root would be reset with it, and a count that lived
//! per rhei would let a project add a rhei to buy capacity.
//! §FS-rhei-budgets.5.1 §AR-neural-admission.7

use super::journal::{Audit, Journal};
use super::types::BudgetError;
use super::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where an account's receipts live, relative to the project root.
pub const ACCOUNT_DIR: &str = ".agent-grounds/rhei/budgets";

/// A project's account: the root it belongs to and the uuid that names it.
#[derive(Clone, Debug)]
pub struct Account {
    root: PathBuf,
    uuid: String,
}

impl Account {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn directory(&self) -> PathBuf {
        self.root.join(ACCOUNT_DIR).join(&self.uuid)
    }

    /// The account this project root already has, from its own directory or —
    /// when the directory was deleted — from the witness index, which records
    /// every canonical root it has seen.
    ///
    /// That second lookup is what makes deleting `budgets/` a **damaged**
    /// account rather than a fresh one: the uuid is recovered, its witness is
    /// found, and the journal is missing. §FS-rhei-budgets.5.3
    pub fn locate(project_root: &Path) -> Result<Option<Self>> {
        let root = canonical(project_root)?;
        if let Some(uuid) = local_uuid(&root)? {
            return Ok(Some(Self { root, uuid }));
        }
        let uuid = witnessed_roots()?.get(&root).cloned();
        Ok(uuid.map(|uuid| Self { root, uuid }))
    }

    /// The project's account, minted when it is absent.
    ///
    /// Establishment is part of the first admission rather than a migration
    /// step, and it mints nothing: a new account starts at zero consumed under
    /// the window contract. §FS-rhei-budgets.5.4
    pub fn establish(project_root: &Path, audit: &Audit) -> Result<(Self, Journal)> {
        let account = match Self::locate(project_root)? {
            Some(account) => account,
            None => {
                let root = canonical(project_root)?;
                let uuid = uuid::Uuid::new_v4().to_string();
                record_root(&root, &uuid)?;
                Self { root, uuid }
            }
        };
        let journal = Journal::establish(&account.root, &account.uuid, audit)?;
        Ok((account, journal))
    }

    /// Open an account that must already exist, for a command that reads or
    /// adjusts rather than admits. §FS-rhei-budgets.10
    pub fn open(&self, writable: bool) -> Result<Journal> {
        Journal::open(&self.root, &self.uuid, writable)
    }

    /// The ticket identity string one ticket uuid takes inside this account.
    pub fn ticket_identity(&self, ticket_uuid: &str) -> String {
        format!("ticket:{}:{ticket_uuid}", self.uuid)
    }
}

fn canonical(root: &Path) -> Result<PathBuf> {
    crate::platform::canonical_path(root).map_err(|error| BudgetError::unreachable(root, &error))
}

/// The single uuid-named directory under this root's `budgets/`, if any.
///
/// More than one is corruption rather than a choice: nothing writes a second,
/// and picking one would be picking which history to forget.
fn local_uuid(root: &Path) -> Result<Option<String>> {
    let dir = root.join(ACCOUNT_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut found: Option<String> = None;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if super::types::uuid(&name).is_err() {
            continue;
        }
        if found.is_some() {
            return Err(BudgetError::corrupt(format!(
                "{} holds more than one account directory",
                dir.display()
            )));
        }
        found = Some(name);
    }
    Ok(found)
}

/// The witness index: every canonical root the witness directory has seen,
/// with the account uuid it belongs to. §FS-rhei-budgets.5.3
/// The same base the witness itself uses, resolved once for the process: an
/// index written beside one state directory and read beside another would
/// forget a root the witness remembers. §FS-rhei-budgets.5.3
fn roots_index_path() -> Result<PathBuf> {
    Ok(super::authority::authority_base()?.join("rhei/budget-authority/roots.json"))
}

fn witnessed_roots() -> Result<BTreeMap<PathBuf, String>> {
    let path = roots_index_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(BudgetError::unreachable(&path, &error)),
    }
}

fn record_root(root: &Path, uuid: &str) -> Result<()> {
    let path = roots_index_path()?;
    super::authority::durable_directories(path.parent().expect("index has a parent"))?;
    let mut index = witnessed_roots()?;
    index.insert(root.to_path_buf(), uuid.to_string());
    let pending = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&pending, serde_json::to_vec_pretty(&index)?)
        .map_err(|error| BudgetError::unreachable(&pending, &error))?;
    std::fs::rename(&pending, &path).map_err(|error| BudgetError::unreachable(&path, &error))?;
    Ok(())
}
