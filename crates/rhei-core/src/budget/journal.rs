//! The hash-chained receipt log, its lock, append-and-sync, and `adjust`'s
//! floor.
//!
//! Opening verifies the whole chain: contiguous sequence, each line's
//! `previous_hash` equal to the SHA-256 of the exact preceding line's bytes,
//! and one project identity throughout. A partial write cannot yield capacity:
//! the append fails, and the next opener refuses the incomplete chain.
//! §FS-rhei-budgets.5.2 §AR-neural-admission.4

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
        uuid(project_uuid)?;
        let authority = super::authority::Authority::lock(root, project_uuid)?;
        let dir = root.join(".agent-grounds/rhei/budgets").join(project_uuid);
        if create {
            super::authority::durable_directories(&dir)?;
        }
        let path = dir.join("journal.jsonl");
        let lock = OpenOptions::new()
            .create(create)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("journal.jsonl.lock"))?;
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
        ledger.load()?;
        ledger.day = super::window::effective_day(
            &super::window::day_key(super::window::now()?),
            ledger.state.highest_day.as_deref(),
        );
        Ok(ledger)
    }

    /// The three ledger states of §FS-rhei-budgets.5.4, told apart by name.
    fn load(&mut self) -> Result<()> {
        let bytes = match File::open(&self.path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                bytes
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        if bytes.is_empty() {
            // **Absent** when the witness agrees there is nothing; **damaged**
            // when the witness knows of receipts this root no longer has.
            if !self.authority.is_empty() {
                return Err(BudgetError::corrupt(format!(
                    "the committed history at {} records receipts the project journal at {} \
                     does not have; restore the journal by copying the witness back over it",
                    self.authority.path().display(),
                    self.path.display()
                )));
            }
            return Ok(());
        }
        // Verify the chain on its own terms first, so an **adopted** journal is
        // one this machine has checked rather than one it merely inherited.
        self.replay(&bytes)?;
        if self.authority.is_empty() {
            return self.authority.adopt(&bytes);
        }
        self.authority.verify(&bytes, &self.path)
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

    /// The day key this transaction draws against. §FS-rhei-budgets.3.3
    pub fn day(&self) -> &str {
        &self.day
    }

    /// True when this open wrote the witness from a journal it had never seen.
    /// §FS-rhei-budgets.5.4
    pub fn adopted(&self) -> bool {
        self.authority.adopted()
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
        let mut file = options.open(&self.path)?;
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
        if bytes.is_empty() || !bytes.ends_with(b"\n") {
            return Err(BudgetError::corrupt("missing or truncated receipt"));
        }
        let mut ids = BTreeSet::new();
        for line in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
            let receipt: Receipt = serde_json::from_slice(line)?;
            if receipt.schema != "rhei.budget.receipt.v1"
                || receipt.project_id != self.project_id
                || receipt.sequence != add(self.receipts.len() as u64, 1)?
                || receipt.previous_hash != self.last_hash
                || !ids.insert(receipt.receipt_id.clone())
                || receipt.actor.is_empty()
                || receipt.written_at.is_empty()
                || !receipt.receipt_id.starts_with("receipt:")
            {
                return Err(BudgetError::corrupt("invalid identity, sequence, or hash chain"));
            }
            self.state.apply(&receipt)?;
            self.receipts.push(receipt);
            self.last_hash = Some(digest(line));
        }
        Ok(())
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Sync directory entries after creating durable state, so a directory that
/// holds a receipt cannot vanish under a crash. §FS-rhei-budgets.5.1
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
