//! The hash-chained receipt log, its lock, append-and-sync, and `adjust`'s
//! floor.
//!
//! Opening verifies the whole chain: contiguous sequence, each line's
//! `previous_hash` equal to the SHA-256 of the exact preceding line's bytes,
//! and one project identity throughout. A partial write cannot yield capacity:
//! the append fails, and the next opener refuses the incomplete chain.
//! §FS-rhei-budgets.5.2 §AR-neural-admission.4

use super::authority::{cut, write_whole, Writer};
use super::classify::{classify, Reading, Sides};
use super::diagnosis::{Diagnosis, Opening};
use super::replay::State;
use super::types::{add, uuid, BudgetError, Contract, Snapshot};
use super::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::Read;
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
    /// The canonical project root this account was opened at.
    root: PathBuf,
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
    /// charge of either kind — an admission or an applied edge — mints it
    /// inside the transaction it is already holding. §FS-rhei-budgets.5.4
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

    /// Refuse damaged history with the same classification inspection reports.
    /// §FS-rhei-budgets.5.4
    fn locked(root: &Path, project_uuid: &str, writable: bool, create: bool) -> Result<Self> {
        match Self::locked_or_damaged(root, project_uuid, writable, create)? {
            Opening::Verified(ledger) => Ok(*ledger),
            Opening::Damaged(_, error) | Opening::Absent(error) => Err(error),
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
    /// the account is created by that. Where the journal and lock are absent,
    /// classify the missing history under the authority alone, which excludes
    /// every writer in §AR-neural-admission.3's lock order. That result carries
    /// no usable journal, and creates no directory or lock. §FS-rhei-budgets.5.4
    pub(crate) fn locked_or_damaged(
        root: &Path,
        project_uuid: &str,
        writable: bool,
        create: bool,
    ) -> Result<Opening> {
        uuid(project_uuid)?;
        let authority = super::authority::Authority::lock(root, project_uuid)?;
        // Resolved, because a foreign tail is told by whether a source path lies
        // under it, and sources are recorded resolved. §FS-rhei-budgets.5.4.2
        let resolved = crate::platform::canonical_path(root).unwrap_or_else(|_| root.to_path_buf());
        let dir = root.join(".agent-grounds/rhei/budgets").join(project_uuid);
        if create {
            super::authority::durable_directories(&dir)?;
        }
        let path = dir.join("journal.jsonl");
        let lock_path = dir.join("journal.jsonl.lock");
        // A journal that is there is lockable, restored by hand or not.
        let lock = match OpenOptions::new()
            .create(create || path.exists())
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(lock) => lock,
            Err(error)
                if !create
                    && error.kind() == std::io::ErrorKind::NotFound
                    && matches!(std::fs::metadata(&path), Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound) =>
            {
                let sides = Sides {
                    root: &resolved,
                    uuid: project_uuid,
                    witness_path: authority.path(),
                    witness: authority.bytes(),
                    journal: None,
                };
                return Ok(match classify(&sides) {
                    Reading::Damaged(diagnosis, error) => Opening::Damaged(diagnosis, error),
                    // A journal that is not there verifies nothing.
                    Reading::Absent | Reading::Verified(_) => {
                        Opening::Absent(BudgetError::unreachable(&lock_path, &error))
                    }
                });
            }
            Err(error) => return Err(BudgetError::unreachable(&lock_path, &error)),
        };
        lock.lock_exclusive()?;
        let mut ledger = Self {
            authority,
            _lock: lock,
            root: resolved,
            path,
            project_id: format!("panta:{project_uuid}"),
            state: State::default(),
            receipts: Vec::new(),
            last_hash: None,
            writable,
            day: String::new(),
        };
        if let Some((diagnosis, error)) = ledger.load()? {
            return Ok(Opening::Damaged(diagnosis, error));
        }
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
        Ok(Opening::Verified(Box::new(ledger)))
    }

    /// The three ledger states of §FS-rhei-budgets.5.4, told apart by name.
    ///
    /// Damage is returned rather than only raised, and it is returned
    /// *classified* — by the one classifier every caller shares — so a journal
    /// that is wholly absent, one whose tail was lost and a witness that ends
    /// mid-receipt are each the accident they are. What is left here is what
    /// only an open does: replay the verified chain, and adopt a journal this
    /// machine has no witness for. §FS-rhei-budgets.5.4 §FS-rhei-budgets.5.4.3
    fn load(&mut self) -> Result<Option<(Box<Diagnosis>, BudgetError)>> {
        let journal = match File::open(&self.path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                Some(bytes)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let sides = Sides {
            root: &self.root,
            uuid: self.account_uuid(),
            witness_path: self.authority.path(),
            witness: self.authority.bytes(),
            journal: journal.as_deref(),
        };
        let replayed = match classify(&sides) {
            Reading::Absent => return Ok(None),
            Reading::Damaged(diagnosis, error) => return Ok(Some((diagnosis, error))),
            Reading::Verified(replayed) => replayed,
        };
        self.state = replayed.state;
        self.receipts = replayed.receipts;
        self.last_hash = replayed.last_hash;
        if self.authority.is_empty() {
            self.authority.adopt(journal.as_deref().unwrap_or_default())?;
        }
        Ok(None)
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
        self.writable = false;
        self.write_receipt(&line, write_whole)?;
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

    /// The witness first, then the journal: a crash between them leaves the
    /// witness ahead by whole receipts, which refuses rather than losing one.
    ///
    /// An append that *fails* leaves neither. The witness cuts itself back; a
    /// journal that fails cuts the journal back and then the witness, so both
    /// are as they were before the edge. Both cuts or neither: a journal that
    /// cannot be cut back leaves the witness a whole receipt ahead, which the
    /// copy restores. `write` is how the line reaches the journal, passed in so
    /// a test can cut it short. §AR-neural-admission.4
    pub(super) fn write_receipt(&mut self, line: &[u8], write: Writer) -> Result<()> {
        let witnessed = self.authority.bytes().len() as u64;
        self.authority.append(line)?;
        let Err((error, journal_cut)) =
            append_line(&self.path, self.receipts.is_empty(), line, write)
        else {
            return Ok(());
        };
        if journal_cut.map_or(Ok(()), |length| cut(&self.path, length)).is_ok() {
            let _ = self.authority.cut_back(witnessed);
        }
        Err(error)
    }
}

/// Append one line to the journal, which the account's first receipt creates.
///
/// A failure carries the length to cut the journal back to: none where the
/// append never opened it or it is still at its old length, because then it
/// holds nothing of the line; and the length of none where this append created
/// it, so it is removed. §AR-neural-admission.4
fn append_line(
    path: &Path,
    first: bool,
    line: &[u8],
    write: Writer,
) -> std::result::Result<(), (BudgetError, Option<u64>)> {
    let mut options = OpenOptions::new();
    options.write(true).append(true);
    if first {
        options.create_new(true);
    }
    let file =
        options.open(path).map_err(|error| (BudgetError::unreachable(path, &error), None))?;
    let before = match first {
        true => 0,
        false => file.metadata().map_err(|error| (error.into(), None))?.len(),
    };
    write(&file, line).and_then(|()| file.sync_all()).map_err(|error| {
        let now = file.metadata().map(|metadata| metadata.len()).ok();
        (error.into(), (first || now != Some(before)).then_some(before))
    })
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
