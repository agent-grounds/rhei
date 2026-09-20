//! Recovery restores only complete independently witnessed receipt bytes.
//! §FS-rhei-budgets.8 §AR-neural-admission.3

use super::journal::{digest, sync_directory};
use super::{Audit, BudgetError, Journal, Receipt, Result};
use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

impl Journal {
    /// The candidate must match the external witness exactly; a self-consistent
    /// chain or hand-authored total is not provenance. §FS-rhei-budgets.8
    pub fn recover(root: &Path, uuid: &str, candidate: &Path, audit: &Audit) -> Result<()> {
        audit.validate()?;
        let mut ledger = Self::locked(root, uuid, true, false, true)?;
        let bytes = std::fs::read(candidate)?;
        ledger.authority.verify(&bytes)?;
        if same_file(candidate, &ledger.path)? {
            return Err(BudgetError::corrupt("recovery requires an independent complete copy"));
        }
        // Even a damaged target may contain intact evidence that this witness
        // cannot explain. Never discard such financial records. §FS-rhei-budgets.8
        match std::fs::read(&ledger.path) {
            Ok(damaged) => {
                for line in damaged.split(|b| *b == b'\n') {
                    if let Ok(receipt) = serde_json::from_slice::<Receipt>(line) {
                        if receipt.project_id != ledger.project_id
                            || !bytes.split(|b| *b == b'\n').any(|trusted| trusted == line)
                        {
                            return Err(BudgetError::corrupt(
                                "intact target receipt is absent from recovery authority",
                            ));
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        // Build and validate the audit receipt without using normal append,
        // which requires an already trustworthy local mirror. §FS-rhei-budgets.8
        let receipt = Receipt {
            schema: "rhei.budget.receipt.v1".into(),
            project_id: ledger.project_id.clone(),
            sequence: super::types::add(ledger.receipts.len() as u64, 1)?,
            receipt_id: format!("receipt:{}", uuid::Uuid::new_v4()),
            previous_hash: ledger.last_hash.clone(),
            written_at: audit.written_at.clone(),
            actor: audit.actor.clone(),
            kind: "recover".into(),
            payload: json!({"evidence_hash": digest(&bytes), "evidence_path": candidate,
                "authority_path": ledger.authority.path(), "audit": audit}),
        };
        ledger.state.apply(&receipt)?;
        let mut line = serde_json::to_vec(&receipt)?;
        line.push(b'\n');
        let authority_candidate = same_file(candidate, ledger.authority.path())?;
        ledger.authority.append(&line)?;
        let mut recovered = bytes;
        recovered.extend_from_slice(&line);
        // Crash after the witness commit is conservative: the complete new
        // witness remains the recovery source, never the old prefix.
        // §FS-rhei-budgets.3.2
        if !authority_candidate {
            atomic_replace(candidate, &recovered)?;
        }
        atomic_replace(&ledger.path, &recovered)
    }

    pub fn authority_path(&self) -> &Path {
        self.authority.path()
    }
}

fn same_file(a: &Path, b: &Path) -> Result<bool> {
    if !b.exists() {
        return Ok(false);
    }
    if std::fs::canonicalize(a)? == std::fs::canonicalize(b)? {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let a = std::fs::metadata(a)?;
        let b = std::fs::metadata(b)?;
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| BudgetError::corrupt("invalid recovery path"))?;
    let temp = parent.join(format!(".budget-recovery-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, path)?;
    sync_directory(parent)
}
