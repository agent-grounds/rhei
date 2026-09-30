//! The hash-chained receipt log, its lock, append-and-sync, and `adjust`'s
//! floor.
//!
//! Opening verifies the whole chain: contiguous sequence, each line's
//! `previous_hash` equal to the SHA-256 of the exact preceding line's bytes,
//! and one project identity throughout. A partial write cannot yield capacity:
//! the append fails, and the next opener refuses the incomplete chain.
//! §FS-rhei-budgets.5.2 §AR-neural-admission.4

use super::diagnosis::{absent_journal_refusal, Damage, Damaged, History};
use super::replay::State;
use super::types::{add, uuid, BudgetError, Contract, Snapshot};
use super::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Supplied by the owning driver, which authenticates its local operator.
/// §FS-rhei-budgets.10
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audit {
    pub actor: String,
    pub written_at: String,
    pub reason: String,
    pub argv: Vec<String>,
}

impl Audit {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.actor.trim().is_empty()
            || self.written_at.is_empty()
            || self.reason.trim().is_empty()
        {
            return Err(BudgetError::bounds("actor, UTC time, and a nonempty reason are required"));
        }
        Ok(())
    }
}

/// Public v1 envelope. Payload semantics are verified during replay.
/// §FS-rhei-budgets.5.2
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: String,
    pub project_id: String,
    pub sequence: u64,
    pub receipt_id: String,
    pub previous_hash: Option<String>,
    pub written_at: String,
    pub actor: String,
    pub kind: String,
    /// The UTC day key, on the kinds where one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
    pub payload: Value,
}

/// A verified ledger with its exclusive lock held. The data file is opened
/// only *after* the lock, including for read-only inspection.
/// §AR-neural-admission.3
pub struct Journal {
    pub(crate) authority: super::authority::Authority,
    _lock: File,
    pub(crate) path: PathBuf,
    pub(crate) project_id: String,
    pub(crate) state: State,
    pub(crate) receipts: Vec<Receipt>,
    pub(crate) last_hash: Option<String>,
    pub(crate) writable: bool,
    /// The day capacity is drawn against for the whole transaction, settled
    /// once on open so two reads inside one admission cannot straddle
    /// midnight. §FS-rhei-budgets.3.3
    pub(crate) day: String,
}

/// The path and the identity, never the chain: a debug print of a ledger is
/// for saying *which* ledger, and the receipts are read through `receipts`.
impl std::fmt::Debug for Journal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Journal")
            .field("project_id", &self.project_id)
            .field("path", &self.path)
            .field("receipts", &self.receipts.len())
            .field("day", &self.day)
            .finish()
    }
}

impl Journal {
    /// Open an account that already exists. Never initializes or repairs one.
    /// §FS-rhei-budgets.5.4
    pub fn open(root: &Path, project_uuid: &str, writable: bool) -> Result<Self> {
        Self::locked(root, project_uuid, writable, false)
    }

    /// Open the account, establishing it when it is **absent** — no journal and
    /// no witness claiming this root. That is lawful and silent: the first
    /// admission mints it inside the transaction it is already holding.
    /// §FS-rhei-budgets.5.4
    pub fn establish(root: &Path, project_uuid: &str, audit: &Audit) -> Result<Self> {
        audit.validate()?;
        let mut ledger = Self::locked(root, project_uuid, true, true)?;
        if ledger.state.contract.is_none() {
            ledger.append(
                "initialize",
                json!({"contract": Contract::Window.to_json(),
                "audit": audit}),
                audit,
            )?;
            sync_directory(ledger.path.parent().expect("journal has a parent"))?;
        }
        Ok(ledger)
    }

    fn locked(root: &Path, project_uuid: &str, writable: bool, create: bool) -> Result<Self> {
        let (ledger, damaged) = Self::locked_or_damaged(root, project_uuid, writable, create)?;
        match damaged {
            // A caller that only wanted the account gets exactly the words it
            // got before the classification became a value.
            // §FS-rhei-budgets.5.4
            Some(damaged) => Err(damaged.error),
            None => Ok(ledger),
        }
    }

    /// Open the account and say whether it is damaged, rather than refusing on
    /// its behalf.
    ///
    /// The one entry point `rhei budget show` uses, so the report it renders and
    /// the refusal a run raises are the same judgement. §FS-rhei-budgets.10
    ///
    /// `create` is about the **account** — the directory and the receipts — and
    /// not about the lock file, which is this transaction's own handle. So a
    /// journal that is *there* is opened whether or not a lock file is beside it:
    /// a journal restored by hand, which is the remedy §FS-rhei-budgets.5.4
    /// offers and a refusal prints, arrives without one, and refusing to open it
    /// would make the offered restore a command that fixes nothing. Nothing of
    /// the account is created by that, and where the account directory itself is
    /// absent this still fails exactly as it did before. §FS-rhei-budgets.10
    pub(crate) fn locked_or_damaged(
        root: &Path,
        project_uuid: &str,
        writable: bool,
        create: bool,
    ) -> Result<(Self, Option<Damaged>)> {
        uuid(project_uuid)?;
        let authority = super::authority::Authority::lock(root, project_uuid)?;
        let dir = root.join(".agent-grounds/rhei/budgets").join(project_uuid);
        if create {
            super::authority::durable_directories(&dir)?;
        }
        let path = dir.join("journal.jsonl");
        let lock_path = dir.join("journal.jsonl.lock");
        // A journal that is there is lockable, restored by hand or not.
        let lock = OpenOptions::new()
            .create(create || path.exists())
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| BudgetError::unreachable(&lock_path, &error))?;
        lock.lock_exclusive()?;
        let mut ledger = Self {
            authority,
            _lock: lock,
            path,
            project_id: format!("panta:{project_uuid}"),
            state: State::default(),
            receipts: Vec::new(),
            last_hash: None,
            writable,
            day: String::new(),
        };
        let damaged = ledger.load()?;
        // A kind this build does not understand is not a broken account: it is
        // a newer one. It reports, and it may not append.
        // §FS-rhei-budgets.5.2
        if !ledger.state.unknown_kinds.is_empty() {
            ledger.writable = false;
        }
        ledger.day = super::window::effective_day(
            &super::window::day_key(super::window::now()?),
            ledger.state.highest_day.as_deref(),
        );
        Ok((ledger, damaged))
    }

    /// The three ledger states of §FS-rhei-budgets.5.4, told apart by name.
    ///
    /// Damage is returned rather than only raised, and it is returned
    /// *classified*: a journal that is wholly absent is not the same accident
    /// as one whose tail was lost, and only the first is indistinguishable from
    /// a path a different project used before this one.
    /// §FS-rhei-budgets.5.4
    fn load(&mut self) -> Result<Option<Damaged>> {
        let present = match File::open(&self.path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                Some(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        // The presence is kept as a flag and the bytes are **moved**: the two
        // branches below need to know whether there was a file, not to read its
        // contents a second time, and this is every admission's path.
        let existed = present.is_some();
        let bytes = present.unwrap_or_default();
        if bytes.is_empty() {
            // **Absent** when the witness agrees there is nothing; **damaged**
            // when the witness knows of receipts this root no longer has.
            if self.authority.is_empty() {
                return Ok(None);
            }
            // The file itself is the seam: no file at all is the one case
            // whose history may belong to another project, so the copy that
            // adopts it may not be the remedy. §FS-rhei-budgets.5.4
            return Ok(Some(if existed {
                Damaged { damage: Damage::JournalTruncated, error: self.witness_ahead_refusal() }
            } else {
                Damaged {
                    damage: Damage::JournalAbsent,
                    error: absent_journal_refusal(self.authority.path(), &self.recorded_history()),
                }
            }));
        }
        // Verify the chain on its own terms first, so an **adopted** journal is
        // one this machine has checked rather than one it merely inherited.
        if let Err(error) = self.replay(&bytes) {
            return Ok(Some(Damaged { damage: Damage::ChainBroken, error }));
        }
        if self.authority.is_empty() {
            self.authority.adopt(&bytes)?;
            return Ok(None);
        }
        match self.authority.verify(&bytes, &self.path) {
            Ok(()) => Ok(None),
            // A journal the witness merely continues lost its tail; one that
            // diverges was rolled back or edited, which is the chain's own
            // failure however it was spelled. §FS-rhei-budgets.5.4
            Err(error) if self.authority.continues(&bytes) => {
                Ok(Some(Damaged { damage: Damage::JournalTruncated, error }))
            }
            Err(error) => Ok(Some(Damaged { damage: Damage::ChainBroken, error })),
        }
    }

    /// Today's words for a witness that knows of receipts the journal does not
    /// have. Unchanged, because a journal that is present is this project's and
    /// copying the witness back over it restores this project's own history.
    /// §FS-rhei-budgets.5.4
    fn witness_ahead_refusal(&self) -> BudgetError {
        BudgetError::corrupt(format!(
            "the committed history at {} records receipts the project journal at {} \
             does not have; restore the journal by copying the witness back over it",
            self.authority.path().display(),
            self.path.display()
        ))
    }

    /// What the committed history holds, for a caller that has no journal to
    /// read. §FS-rhei-budgets.5.3
    pub(crate) fn recorded_history(&self) -> History {
        History::read(&self.project_id, self.authority.bytes())
    }

    pub(crate) fn authority_path(&self) -> &Path {
        self.authority.path()
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.state.snapshot(&self.project_id, &self.day)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The account uuid this ledger belongs to, without the `panta:` prefix the
    /// receipt envelope carries it under. This is the name an ancestry
    /// descriptor's minting account is compared with, so it has to be the same
    /// string `Account::uuid` hands a caller. §FS-rhei-budgets.5.1
    /// §FS-rhei-budgets.7.1
    pub fn account_uuid(&self) -> &str {
        self.project_id.strip_prefix("panta:").unwrap_or(&self.project_id)
    }

    /// The day key this transaction draws against. §FS-rhei-budgets.3.3
    pub fn day(&self) -> &str {
        &self.day
    }

    /// True when this open wrote the witness from a journal it had never seen.
    /// §FS-rhei-budgets.5.4
    pub fn adopted(&self) -> bool {
        self.authority.adopted()
    }

    /// Whether this transaction may append at all. A caller that asked for a
    /// writable account and got a read-only one is reading a chain newer than
    /// itself. §FS-rhei-budgets.5.2
    pub fn writable(&self) -> bool {
        self.writable
    }

    pub fn receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    pub fn contract(&self) -> Result<Contract> {
        self.state
            .contract
            .clone()
            .ok_or_else(|| BudgetError::corrupt("missing account initialization"))
    }

    /// Move to a lifetime allowance, or change the one in force.
    ///
    /// Allowances change; consumption does not. A request below
    /// `consumed + outstanding` is refused, because it would put a project in
    /// deficit for work already done. A request above the machine's ceiling is
    /// the caller's to clamp before it gets here — being high is never a
    /// refusal. §FS-rhei-budgets.10
    pub fn adjust(&mut self, allowance: u64, audit: &Audit) -> Result<()> {
        if allowance == 0 {
            return Err(BudgetError::bounds("an invocation allowance must be positive"));
        }
        let before = self.contract()?;
        let spent = self.snapshot()?.lifetime_invocations.exposure()?;
        if allowance < spent {
            return Err(BudgetError::bounds(format!(
                "cannot set an allowance of {allowance} below the {spent} invocations already \
                 consumed or outstanding"
            )));
        }
        let next = Contract::Lifetime { allowance };
        self.append(
            "adjust",
            json!({"old": before.to_json(), "new": next.to_json(), "audit": audit}),
            audit,
        )
    }

    pub(crate) fn append(&mut self, kind: &str, payload: Value, audit: &Audit) -> Result<()> {
        self.append_in_window(kind, payload, audit, None)
    }

    pub(crate) fn append_in_window(
        &mut self,
        kind: &str,
        payload: Value,
        audit: &Audit,
        window: Option<String>,
    ) -> Result<()> {
        if !self.writable {
            if let Some(unknown) = self.state.unknown_kinds.iter().next() {
                return Err(BudgetError::corrupt(format!(
                    "this account holds a '{unknown}' receipt this build does not understand; \
                     it can be read but not appended to until the build is upgraded"
                )));
            }
            return Err(BudgetError::corrupt("read-only budget transaction cannot append"));
        }
        audit.validate()?;
        let receipt = Receipt {
            schema: "rhei.budget.receipt.v1".into(),
            project_id: self.project_id.clone(),
            sequence: add(self.receipts.len() as u64, 1)?,
            receipt_id: format!("receipt:{}", uuid::Uuid::new_v4()),
            previous_hash: self.last_hash.clone(),
            written_at: audit.written_at.clone(),
            actor: audit.actor.clone(),
            kind: kind.into(),
            window,
            payload,
        };
        let mut next = self.state.clone();
        next.apply(&receipt)?;
        let mut line = serde_json::to_vec(&receipt)?;
        let hash = digest(&line);
        line.push(b'\n');
        // The witness first, then the journal: a crash between them leaves the
        // witness ahead, which refuses rather than losing a receipt.
        // §AR-neural-admission.4
        let mut options = OpenOptions::new();
        options.write(true).append(true);
        if self.receipts.is_empty() {
            options.create_new(true);
        }
        self.writable = false;
        self.authority.append(&line)?;
        let mut file = options
            .open(&self.path)
            .map_err(|error| BudgetError::unreachable(&self.path, &error))?;
        file.write_all(&line)?;
        file.sync_all()?;
        self.state = next;
        self.receipts.push(receipt);
        self.last_hash = Some(hash);
        self.writable = true;
        if let Some(window) = self.state.highest_day.clone() {
            if window > self.day {
                self.day = window;
            }
        }
        Ok(())
    }

    pub(crate) fn replay(&mut self, bytes: &[u8]) -> Result<()> {
        let replayed = replay_chain(&self.project_id, bytes)?;
        self.state = replayed.state;
        self.receipts = replayed.receipts;
        self.last_hash = replayed.last_hash;
        Ok(())
    }
}

/// One verified chain, held apart from the transaction that opened it.
/// §FS-rhei-budgets.5.2
#[derive(Default)]
pub(crate) struct Replayed {
    pub(crate) state: State,
    pub(crate) receipts: Vec<Receipt>,
    pub(crate) last_hash: Option<String>,
}

/// Verify a chain of receipt bytes under one project identity.
///
/// A free function rather than a method, because the **witness** is
/// byte-identical to a journal and a report that described it with a second
/// parser could disagree with the account about what it holds.
/// §FS-rhei-budgets.5.2 §FS-rhei-budgets.5.3
pub(crate) fn replay_chain(project_id: &str, bytes: &[u8]) -> Result<Replayed> {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(BudgetError::corrupt("missing or truncated receipt"));
    }
    let mut replayed = Replayed::default();
    let mut ids = BTreeSet::new();
    for line in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
        let receipt: Receipt = serde_json::from_slice(line)?;
        if receipt.schema != "rhei.budget.receipt.v1"
            || receipt.project_id != project_id
            || receipt.sequence != add(replayed.receipts.len() as u64, 1)?
            || receipt.previous_hash != replayed.last_hash
            || !ids.insert(receipt.receipt_id.clone())
            || receipt.actor.is_empty()
            || receipt.written_at.is_empty()
            || !receipt.receipt_id.starts_with("receipt:")
        {
            return Err(BudgetError::corrupt("invalid identity, sequence, or hash chain"));
        }
        replayed.state.apply(&receipt)?;
        replayed.receipts.push(receipt);
        replayed.last_hash = Some(digest(line));
    }
    Ok(replayed)
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Sync directory entries after creating durable state, so a directory that
/// holds a receipt cannot vanish under a crash. §FS-rhei-budgets.5.1
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| BudgetError::unreachable(path, &error))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
