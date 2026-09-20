//! Stable sidecar ownership and hash-linked, durable receipts.
//! §FS-rhei-budgets.3.1 §FS-rhei-budgets.3.2

use super::replay::State;
use super::types::{add, uuid};
use super::{Allowance, BudgetError, Result, Snapshot};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Supplied by the owning driver, which authenticates its local operator.
/// §FS-rhei-budgets.8
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
/// §FS-rhei-budgets.3.2
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
    pub payload: Value,
}

/// A verified ledger with its stable exclusive lock held. File handles are
/// opened *after* acquiring the sidecar, including for read-only inspection.
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
    pub(crate) event_sink: Option<std::sync::Arc<dyn super::BudgetEventSink>>,
}

impl Journal {
    /// Opening never initializes or repairs a missing ledger.
    /// §FS-rhei-budgets.7
    pub fn open(root: &Path, project_uuid: &str, writable: bool) -> Result<Self> {
        Self::locked(root, project_uuid, writable, false, false)
    }

    pub(crate) fn locked(
        root: &Path,
        project_uuid: &str,
        writable: bool,
        create: bool,
        recovery: bool,
    ) -> Result<Self> {
        uuid(project_uuid)?;
        let authority = super::authority::Authority::lock(root, project_uuid, create)?;
        let dir = root.join(".agent-grounds/rhei/budgets").join(project_uuid);
        if create {
            super::authority::durable_directories(&dir)?;
        }
        let path = dir.join("journal.jsonl");
        let lock = OpenOptions::new()
            .create(true)
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
            event_sink: None,
        };
        if recovery {
            let bytes = ledger.authority.bytes().to_vec();
            ledger.replay(&bytes)?;
            return Ok(ledger);
        }
        match File::open(&ledger.path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                if create
                    && ledger.authority.bytes().starts_with(&bytes)
                    && ledger.authority.bytes() != bytes
                    && !ledger.authority.bytes().is_empty()
                {
                    let witnessed = ledger.authority.bytes().to_vec();
                    ledger.replay(&witnessed)?;
                    if ledger.receipts.len() != 1 || ledger.receipts[0].kind != "initialize" {
                        return Err(BudgetError::corrupt(
                            "existing financial history needs audited recovery",
                        ));
                    }
                    drop(file);
                    super::recovery::atomic_replace(&ledger.path, &witnessed)?;
                    return Ok(ledger);
                }
                ledger.authority.verify(&bytes)?;
                ledger.replay(&bytes)?;
            }
            Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {
                // Only the initial receipt can be resumed by init. Financial
                // history is never recreated through this path. §FS-rhei-budgets.8
                let witnessed = ledger.authority.bytes().to_vec();
                if !witnessed.is_empty() {
                    ledger.replay(&witnessed)?;
                    if ledger.receipts.len() != 1 || ledger.receipts[0].kind != "initialize" {
                        return Err(BudgetError::corrupt(
                            "existing financial history needs audited recovery",
                        ));
                    }
                    super::recovery::atomic_replace(&ledger.path, &witnessed)?;
                }
            }
            Err(error) => return Err(error.into()),
        }
        Ok(ledger)
    }

    /// The caller must have established that no historical work is missing.
    /// A pre-existing identity does not imply an empty historical balance.
    /// §FS-rhei-budgets.8
    pub fn initialize_empty(
        root: &Path,
        project_uuid: &str,
        allowance: Allowance,
        audit: &Audit,
    ) -> Result<Self> {
        audit.validate()?;
        allowance.validate()?;
        let mut ledger = Self::locked(root, project_uuid, true, true, false)?;
        if !ledger.receipts.is_empty() {
            let initial = &ledger.receipts[0].payload;
            if initial.get("allowance") == Some(&serde_json::to_value(&allowance)?)
                && initial.get("history") == Some(&json!({"status": "complete", "records": 0}))
            {
                return Ok(ledger);
            }
            return Err(BudgetError::bounds(
                "budget already initialized with different bounds or history",
            ));
        }
        ledger.append(
            "initialize",
            json!({
                "allowance": allowance,
                "history": {"status": "complete", "records": 0},
                "audit": audit,
            }),
            audit,
        )?;
        sync_directory(ledger.path.parent().expect("journal has a parent"))?;
        Ok(ledger)
    }

    /// Initialize from a complete provider-authoritative lifetime rather than
    /// minting a fresh balance over unknown work. §FS-rhei-budgets.8.1
    pub fn initialize_imported(
        root: &Path,
        project_uuid: &str,
        allowance: Allowance,
        history: super::VerifiedHistory,
        audit: &Audit,
    ) -> Result<Self> {
        audit.validate()?;
        allowance.validate()?;
        if history.scope.project_id != format!("panta:{project_uuid}")
            || history.scope.currency != allowance.spend.currency
        {
            return Err(super::provider_evidence::refused(
                "history belongs to another project or currency",
            ));
        }
        if history.invocations > allowance.invocations
            || history.charge_micro > allowance.spend.amount_micro
        {
            return Err(BudgetError::bounds(
                "imported lifetime exposure exceeds the requested allowance",
            ));
        }
        let mut ledger = Self::locked(root, project_uuid, true, true, false)?;
        let history_payload = json!({
            "status": "imported",
            "evidence_hash": history.evidence_hash,
            "invocations": history.invocations,
            "charge_micro": history.charge_micro,
            "records": history.envelope.records,
            "authority": history.scope,
            "coverage_start": history.envelope.coverage_start,
            "coverage_end": history.envelope.coverage_end,
        });
        if !ledger.receipts.is_empty() {
            let initial = &ledger.receipts[0].payload;
            if initial.get("allowance") == Some(&serde_json::to_value(&allowance)?)
                && initial.get("history") == Some(&history_payload)
            {
                return Ok(ledger);
            }
            return Err(BudgetError::bounds(
                "budget already initialized with different bounds or history",
            ));
        }
        ledger.append(
            "initialize",
            json!({"allowance": allowance, "history": history_payload, "audit": audit}),
            audit,
        )?;
        sync_directory(ledger.path.parent().expect("journal has a parent"))?;
        Ok(ledger)
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        self.state.snapshot(&self.project_id)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn receipts(&self) -> &[Receipt] {
        &self.receipts
    }

    /// Ceilings change, consumption does not. Breaches survive adjustment.
    /// §FS-rhei-budgets.8
    pub fn adjust(&mut self, allowance: Allowance, audit: &Audit) -> Result<()> {
        allowance.validate()?;
        let before = self.snapshot()?;
        if allowance.spend.currency != before.allowance.spend.currency {
            return Err(BudgetError::bounds("an allowance's currency cannot change"));
        }
        if allowance.invocations < add(before.consumed.invocations, before.reserved.invocations)?
            || allowance.spend.amount_micro
                < add(before.consumed.spend.amount_micro, before.reserved.spend.amount_micro)?
        {
            return Err(BudgetError::bounds(
                "cannot decrease allowance below settled plus outstanding exposure",
            ));
        }
        self.append(
            "adjust",
            json!({"old": before.allowance, "new": allowance, "audit": audit}),
            audit,
        )
    }

    pub(crate) fn append(&mut self, kind: &str, payload: Value, audit: &Audit) -> Result<()> {
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
            payload,
        };
        let mut next = self.state.clone();
        next.apply(&receipt)?;
        let snapshot = next.snapshot(&self.project_id)?;
        let mut line = serde_json::to_vec(&receipt)?;
        let hash = digest(&line);
        line.push(b'\n');
        // A partial write cannot yield a launch capability: the append fails,
        // and the next opener refuses the incomplete/hash-invalid chain.
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
        self.emit_receipt(self.receipts.last().expect("receipt was appended"), snapshot);
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

/// Sync directory entries after creating durable financial state.
/// §FS-rhei-budgets.3.1
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod evidence_tests;

#[cfg(test)]
#[path = "ancestry_tests.rs"]
mod ancestry_tests;
