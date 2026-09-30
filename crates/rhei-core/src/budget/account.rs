//! Resolving a project to its one account, and establishing an absent one.
//!
//! The account is per **project** and lives outside `runtime/`: a count that
//! lived under an execution root would be reset with it, and a count that lived
//! per rhei would let a project add a rhei to buy capacity.
//! §FS-rhei-budgets.5.1 §AR-neural-admission.7

use super::diagnosis::{journal_path, Damage, Diagnosis, History, Inspection, Retired};
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

    /// Read the account, describing a damaged one rather than failing to open
    /// it.
    ///
    /// This is what makes `rhei budget show` runnable in the state a refusal
    /// sends an operator to. Where the account directory is **absent** there is
    /// no journal lock to take and none is taken: the authority sits above the
    /// journal in §AR-neural-admission.3's lock order and already excludes the
    /// only writer that could create one, so classifying under it alone is
    /// correctly serialized. Wherever the directory exists the journal lock is
    /// taken exactly as before. §FS-rhei-budgets.10
    pub fn inspect(&self) -> Result<Inspection> {
        if !self.directory().exists() {
            let authority = super::authority::Authority::lock(&self.root, &self.uuid)?;
            if authority.is_empty() {
                return Ok(Inspection::Absent);
            }
            let history = History::read(&format!("panta:{}", self.uuid), authority.bytes());
            return Ok(Inspection::Damaged(Box::new(Diagnosis::new(
                Damage::JournalAbsent,
                &self.root,
                &self.uuid,
                authority.path(),
                history,
            ))));
        }
        let (journal, damaged) = Journal::locked_or_damaged(&self.root, &self.uuid, false, false)?;
        let Some(damaged) = damaged else {
            return Ok(Inspection::Verified(Box::new(journal)));
        };
        let diagnosis = Diagnosis::new(
            damaged.damage,
            &self.root,
            &self.uuid,
            journal.authority_path(),
            journal.recorded_history(),
        );
        Ok(Inspection::Damaged(Box::new(diagnosis)))
    }

    /// Retire this root: the operator's answer to the reading the tool cannot
    /// choose between.
    ///
    /// The guard is here rather than in the command, because the guarantee of
    /// §FS-rhei-budgets.5.3 is the account's and not a caller's to remember: an
    /// account whose journal verifies is refused, and nothing is written.
    /// Classification happens under the **authority** lock alone, which is
    /// sound and is the only way it can be done once — that lock already
    /// excludes every writer, since every append takes it before the journal's,
    /// and taking the journal lock as well would mean locking the authority
    /// twice in one process. §AR-neural-admission.3
    ///
    /// Retiring is then three writes, and all three are required. The witness
    /// moves under a retired name, keeping every receipt. A receipt of the
    /// retirement is written beside it. And this root's entry leaves the
    /// witness index — not optional: an entry left behind would hand the next
    /// project at this path the retired project's identity, and a retirement
    /// that leaves the identity in place has not retired the root.
    /// §FS-rhei-budgets.5.3 §FS-rhei-budgets.10
    pub fn retire(&self, audit: &Audit) -> Result<Retirement> {
        audit.validate()?;
        let authority = super::authority::Authority::lock(&self.root, &self.uuid)?;
        if authority.is_empty() {
            return Ok(Retirement::NothingToRetire);
        }
        let project_id = format!("panta:{}", self.uuid);
        let journal = read_journal(&journal_path(&self.root, &self.uuid))?;
        let Some(damage) = classify(&project_id, journal.as_deref(), authority.bytes(), &authority)
        else {
            return Ok(Retirement::Sound);
        };
        let history = History::read(&project_id, authority.bytes());
        let receipt = serde_json::json!({
            "schema": "rhei.budget.retirement.v1",
            "uuid": self.uuid,
            "root": self.root,
            "damage": damage.as_str(),
            "actor": audit.actor,
            "written_at": audit.written_at,
            "reason": audit.reason,
            "argv": audit.argv,
            "receipts": history.receipts,
            "invocations": history.invocations,
        });
        let kept_at = authority.retire(&stamp(&audit.written_at), &receipt)?;
        retract_root(&self.root)?;
        self.drop_empty_account_directory(damage)?;
        Ok(Retirement::Retired(Retired {
            uuid: self.uuid.clone(),
            root: self.root.clone(),
            kept_at,
            damage,
            history,
        }))
    }

    /// The account directory a refused establish leaves behind holds the lock
    /// file and nothing else, and leaving it would resolve the path straight
    /// back to the uuid just retired — the very thing the index retraction is
    /// for. Only where there is no journal, so nothing is destroyed.
    /// §FS-rhei-budgets.10
    fn drop_empty_account_directory(&self, damage: Damage) -> Result<()> {
        if damage.journal_exists() {
            return Ok(());
        }
        let directory = self.directory();
        if !directory.exists() || journal_path(&self.root, &self.uuid).exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(&directory)
            .map_err(|error| BudgetError::unreachable(&directory, &error))
    }
}

/// What `retire` found, so the command can say which of the three cases it was
/// without classifying the account a second time. §FS-rhei-budgets.10
#[derive(Debug)]
pub enum Retirement {
    Retired(Retired),
    /// The journal verifies: retiring it would recreate capacity, so nothing
    /// was written. §FS-rhei-budgets.5.3
    Sound,
    /// No witness claims this root — the lawful **absent** state has nothing
    /// to retire. §FS-rhei-budgets.5.4
    NothingToRetire,
}

fn read_journal(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(BudgetError::unreachable(path, &error)),
    }
}

/// The three sub-cases of §FS-rhei-budgets.5.4, from the bytes alone. `None`
/// is a journal that replays and matches the witness: the account is sound.
fn classify(
    project_id: &str,
    journal: Option<&[u8]>,
    witness: &[u8],
    authority: &super::authority::Authority,
) -> Option<Damage> {
    let Some(journal) = journal else { return Some(Damage::JournalAbsent) };
    if journal.is_empty() {
        return Some(Damage::JournalTruncated);
    }
    if super::journal::replay_chain(project_id, journal).is_err() {
        return Some(Damage::ChainBroken);
    }
    if journal == witness {
        return None;
    }
    Some(if authority.continues(journal) { Damage::JournalTruncated } else { Damage::ChainBroken })
}

/// The UTC instant compacted into something that names a directory on every
/// platform: `2026-09-30T13:44:02Z` becomes `20260930T134402Z`.
/// §REQ-cross-platform.5
fn stamp(written_at: &str) -> String {
    written_at.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
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

/// The canonical root the witness index holds for an account uuid, where this
/// machine has ever seen that account.
///
/// The one way to name a project this run does not own: an ancestry descriptor
/// carries a uuid, and a note that printed the uuid would name the account on
/// disk rather than a directory a reader recognizes. `None` is an account this
/// machine has never witnessed, which is not an error — it is a note that falls
/// back to the uuid. §FS-rhei-budgets.5.3 §FS-rhei-budgets.7.2
pub fn witnessed_root(uuid: &str) -> Result<Option<PathBuf>> {
    Ok(witnessed_roots()?.into_iter().find(|(_, held)| held == uuid).map(|(root, _)| root))
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
    let mut index = witnessed_roots()?;
    index.insert(root.to_path_buf(), uuid.to_string());
    write_roots(&index)
}

/// Drop a root from the witness index, so the path resolves to no account and
/// the next admission mints a fresh identity for it. §FS-rhei-budgets.5.3
fn retract_root(root: &Path) -> Result<()> {
    let mut index = witnessed_roots()?;
    if index.remove(root).is_none() {
        return Ok(());
    }
    write_roots(&index)
}

/// The same write-pending-then-rename both directions use, so a retraction is
/// exactly as durable as the record it undoes. §FS-rhei-budgets.5.1
fn write_roots(index: &BTreeMap<PathBuf, String>) -> Result<()> {
    let path = roots_index_path()?;
    super::authority::durable_directories(path.parent().expect("index has a parent"))?;
    let pending = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&pending, serde_json::to_vec_pretty(index)?)
        .map_err(|error| BudgetError::unreachable(&pending, &error))?;
    std::fs::rename(&pending, &path).map_err(|error| BudgetError::unreachable(&path, &error))?;
    Ok(())
}
