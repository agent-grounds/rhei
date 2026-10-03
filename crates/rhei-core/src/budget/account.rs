//! Resolving a project to its one account, and establishing an absent one.
//!
//! The account is per **project** and lives outside `runtime/`: a count that
//! lived under an execution root would be reset with it, and a count that lived
//! per rhei would let a project add a rhei to buy capacity.
//! §FS-rhei-budgets.5.1 §AR-neural-admission.7

use super::diagnosis::{
    foreign_tail, journal_path, Damage, Diagnosis, History, Inspection, Retired,
};
use super::journal::{Audit, Journal};
use super::roots::{held_elsewhere_refusal, record_root, retract_root, settle, witnessed_roots};
use super::types::BudgetError;
use super::Result;
use std::path::{Path, PathBuf};

/// Where an account's receipts live, relative to the project root.
pub const ACCOUNT_DIR: &str = ".agent-grounds/rhei/budgets";

/// A project's account: the root it belongs to and the uuid that names it.
#[derive(Clone, Debug)]
pub struct Account {
    root: PathBuf,
    uuid: String,
    /// The other root that still holds this uuid, where this root is a copy.
    /// §FS-rhei-budgets.5.4.1
    held_by: Option<PathBuf>,
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
    /// when the directory was deleted — from the witness index, which binds
    /// each uuid to the canonical root that holds it.
    ///
    /// That second lookup is what makes deleting `budgets/` a **damaged**
    /// account rather than a fresh one: the uuid is recovered, its witness is
    /// found, and the journal is missing. §FS-rhei-budgets.5.3
    ///
    /// A uuid found in the root's own directory is checked against the index
    /// before anything opens its journal: adopted, it is recorded; moved, it is
    /// rebound with one warning; presented by a copy while the holder still
    /// holds it, it resolves carrying the holder, and every charge on it is
    /// refused. §FS-rhei-budgets.5.4.1 §AR-neural-admission.4
    pub fn locate(project_root: &Path) -> Result<Option<Self>> {
        let root = canonical(project_root)?;
        if let Some(uuid) = local_uuid(&root)? {
            let held_by = settle(&root, &uuid)?;
            return Ok(Some(Self { root, uuid, held_by }));
        }
        let uuid = witnessed_roots()?.get(&root).cloned();
        Ok(uuid.map(|uuid| Self { root, uuid, held_by: None }))
    }

    /// The root that still holds this account, where this root is a copy.
    /// §FS-rhei-budgets.5.4.1
    pub fn held_by(&self) -> Option<&Path> {
        self.held_by.as_deref()
    }

    /// Refuse a copy before its journal is opened, so nothing reaches the
    /// holder's history. §FS-rhei-budgets.5.4.1
    fn refuse_copy(&self) -> Result<()> {
        match &self.held_by {
            Some(holder) => Err(held_elsewhere_refusal(&self.root, &self.uuid, holder)),
            None => Ok(()),
        }
    }

    /// The project's account, minted when it is absent.
    ///
    /// Establishment is part of the first charge of either kind — an admission
    /// or an applied edge — rather than a migration step, and it mints nothing:
    /// a new account starts at zero consumed under the window contract.
    /// §FS-rhei-budgets.5.4
    pub fn establish(project_root: &Path, audit: &Audit) -> Result<(Self, Journal)> {
        let account = match Self::locate(project_root)? {
            Some(account) => account,
            None => {
                let root = canonical(project_root)?;
                let uuid = uuid::Uuid::new_v4().to_string();
                record_root(&root, &uuid)?;
                Self { root, uuid, held_by: None }
            }
        };
        account.refuse_copy()?;
        let journal = Journal::establish(&account.root, &account.uuid, audit)?;
        Ok((account, journal))
    }

    /// Open an account that must already exist, for a command that reads or
    /// adjusts rather than admits. §FS-rhei-budgets.10
    pub fn open(&self, writable: bool) -> Result<Journal> {
        self.refuse_copy()?;
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
        if let Some(holder) = &self.held_by {
            // The holder's witness, read under its lock and never written: a
            // copy's report is the one thing it may have. §FS-rhei-budgets.5.4.1
            let authority = super::authority::Authority::lock(&self.root, &self.uuid)?;
            let history = History::read(&format!("panta:{}", self.uuid), authority.bytes());
            let diagnosis = Diagnosis::new(
                Damage::HeldElsewhere,
                &self.root,
                &self.uuid,
                authority.path(),
                history,
            );
            return Ok(Inspection::Damaged(Box::new(diagnosis.held_by(holder))));
        }
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
        )
        .with_foreign(damaged.foreign);
        Ok(Inspection::Damaged(Box::new(diagnosis)))
    }

    /// Retire this root: the operator's answer to the reading the tool cannot
    /// choose between.
    ///
    /// The guards are here rather than in the command, because what they
    /// protect is the account's and not a caller's to remember. Two refuse. An
    /// account whose journal verifies is refused, which is the guarantee of
    /// §FS-rhei-budgets.5.3. And so is a **damaged** account whose journal is
    /// still there: that journal is this project's own and its tail is what was
    /// lost, so the remedy is the restore, and retiring the root would give up
    /// an account that is genuinely this project's while leaving the journal
    /// that still fails to replay. Only a journal that is wholly absent is
    /// retired, which is the one sub-case a reused path can produce.
    /// §FS-rhei-budgets.5.4 §FS-rhei-budgets.10
    ///
    /// Classification happens under the **authority** lock alone, which is
    /// sound and is the only way it can be done once — that lock already
    /// excludes every writer, since every append takes it before the journal's,
    /// and taking the journal lock as well would mean locking the authority
    /// twice in one process. §AR-neural-admission.3
    ///
    /// Retiring is then three writes, all three are required, and all three
    /// happen while that lock is still held, so a retirement is one serialized
    /// act rather than one locked write followed by two unlocked ones. Their
    /// order is what a crash between them leaves. The index entry goes **first**
    /// — not optional: an entry left behind would hand the next project at this
    /// path the retired project's identity, and a retirement that leaves the
    /// identity in place has not retired the root. The account directory a
    /// refused establish left behind goes next, because until it is gone this
    /// path still resolves to the uuid locally. Only then does the witness move
    /// under a retired name, keeping every receipt, with a receipt of the
    /// retirement written beside it.
    ///
    /// So a crash leaves either a path that still resolves to this damaged
    /// account — `forget` run again finishes the job — or a path that resolves
    /// to nothing, which is the outcome a completed retirement gives, with
    /// every receipt still on disk. What it can never leave is an index entry
    /// pointing at a witness that has moved. §FS-rhei-budgets.5.3 §FS-rhei-budgets.10
    pub fn retire(&self, audit: &Audit) -> Result<Retirement> {
        audit.validate()?;
        if self.held_by.is_some() {
            return self.retire_claim(audit);
        }
        let mut authority = super::authority::Authority::lock(&self.root, &self.uuid)?;
        if authority.is_empty() {
            return Ok(Retirement::NothingToRetire);
        }
        let project_id = format!("panta:{}", self.uuid);
        let journal = read_journal(&journal_path(&self.root, &self.uuid))?;
        let Some(damage) =
            classify(&self.root, &project_id, journal.as_deref(), authority.bytes(), &authority)
        else {
            return Ok(Retirement::Sound);
        };
        let history = History::read(&project_id, authority.bytes());
        if damage.journal_exists() {
            // A tail written from another root is refused too, but it names
            // the set-aside rather than the restore. §FS-rhei-budgets.5.4.2
            let foreign = journal
                .as_deref()
                .filter(|_| damage == Damage::ForeignTail)
                .and_then(|journal| foreign_tail(&self.root, journal, authority.bytes()));
            return Ok(Retirement::RestoreInstead(Box::new(
                Diagnosis::new(damage, &self.root, &self.uuid, authority.path(), history)
                    .with_foreign(foreign),
            )));
        }
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
        retract_root(&self.root)?;
        self.drop_empty_account_directory(damage)?;
        let kept_at = authority.retire(&stamp(&audit.written_at), &receipt)?;
        Ok(Retirement::Retired(Retired {
            uuid: self.uuid.clone(),
            root: self.root.clone(),
            kept_at,
            damage,
            history,
        }))
    }

    /// Retire a copy's claim on an account another root still holds.
    ///
    /// The account is the holder's, so this touches neither the witness nor
    /// the index: it moves this root's `budgets/<uuid>/` to
    /// `budgets/retired/<uuid>-<stamp>/` in one `rename`, keeping every byte,
    /// and writes the audit receipt beside it. A crash between the two leaves
    /// the claim retired without its receipt, never a half-moved journal; the
    /// next charge at this root mints an account of its own.
    /// §FS-rhei-budgets.5.4.1 §FS-rhei-budgets.10
    fn retire_claim(&self, audit: &Audit) -> Result<Retirement> {
        let project_id = format!("panta:{}", self.uuid);
        let journal = read_journal(&journal_path(&self.root, &self.uuid))?.unwrap_or_default();
        let history = History::read(&project_id, &journal);
        let retired_base = self.root.join(ACCOUNT_DIR).join("retired");
        super::authority::durable_directories(&retired_base)?;
        let kept_at = retired_base.join(format!("{}-{}", self.uuid, stamp(&audit.written_at)));
        std::fs::rename(self.directory(), &kept_at)
            .map_err(|error| BudgetError::unreachable(&kept_at, &error))?;
        let receipt = serde_json::json!({
            "schema": "rhei.budget.retirement.v1",
            "uuid": self.uuid,
            "root": self.root,
            "damage": Damage::HeldElsewhere.as_str(),
            "held_by": self.held_by,
            "actor": audit.actor,
            "written_at": audit.written_at,
            "reason": audit.reason,
            "argv": audit.argv,
            "receipts": history.receipts,
            "invocations": history.invocations,
        });
        super::authority::write_durable(
            &kept_at.join("retirement.json"),
            &serde_json::to_vec_pretty(&receipt)?,
        )?;
        super::journal::sync_directory(&retired_base)?;
        Ok(Retirement::Retired(Retired {
            uuid: self.uuid.clone(),
            root: self.root.clone(),
            kept_at,
            damage: Damage::HeldElsewhere,
            history,
        }))
    }

    /// The account directory a refused establish leaves behind holds the lock
    /// file and nothing else, and leaving it would resolve the path straight
    /// back to the uuid just retired — the very thing the index retraction is
    /// for. Only where there is no journal, so nothing is destroyed: `retire`
    /// refuses every damage a journal is present for, and this is the same
    /// condition read a second time in front of a recursive remove.
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
    /// The journal is damaged but **present**, so it is this project's own and
    /// its tail is what was lost: the remedy is the restore this carries, and
    /// nothing was written. Under `foreign_tail` the remedy is the set-aside
    /// instead. §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.2
    RestoreInstead(Box<Diagnosis>),
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

/// The sub-cases of §FS-rhei-budgets.5.4 a present root can show, from the
/// bytes alone. `None` is a journal that replays and matches the witness: the
/// account is sound. A witness tail written from another root is
/// `foreign_tail` rather than `journal_truncated`. §FS-rhei-budgets.5.4.2
fn classify(
    root: &Path,
    project_id: &str,
    journal: Option<&[u8]>,
    witness: &[u8],
    authority: &super::authority::Authority,
) -> Option<Damage> {
    let Some(journal) = journal else { return Some(Damage::JournalAbsent) };
    let truncated = || match foreign_tail(root, journal, witness) {
        Some(_) => Damage::ForeignTail,
        None => Damage::JournalTruncated,
    };
    if journal.is_empty() {
        return Some(truncated());
    }
    if super::journal::replay_chain(project_id, journal).is_err() {
        return Some(Damage::ChainBroken);
    }
    if journal == witness {
        return None;
    }
    Some(if authority.continues(journal) { truncated() } else { Damage::ChainBroken })
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
pub(crate) fn local_uuid(root: &Path) -> Result<Option<String>> {
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
