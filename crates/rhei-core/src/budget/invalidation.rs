//! Cross-project admission fences for breached tuples, under the same external
//! authority as the committed-history witnesses. §FS-rhei-budgets.9

use super::{BudgetError, Journal, Result};
use fs2::FileExt;
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub(super) struct TupleLock {
    _file: File,
    path: PathBuf,
}

impl Journal {
    fn invalidation_path(&self, tuple: &Value) -> Result<PathBuf> {
        let hash = super::journal::digest(&serde_json::to_vec(tuple)?);
        let base = self
            .authority
            .path()
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| BudgetError::corrupt("missing external tuple authority"))?;
        Ok(base.join("tuple-invalidations").join(format!("{}.json", &hash[7..])))
    }

    pub(super) fn lock_tuples(&self, tuples: &[Value]) -> Result<Vec<TupleLock>> {
        let paths = tuples
            .iter()
            .map(|t| self.invalidation_path(t))
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let mut locks = Vec::new();
        for path in paths {
            super::authority::durable_directories(path.parent().expect("tuple parent"))?;
            let file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path.with_extension("lock"))?;
            file.lock_exclusive()?;
            locks.push(TupleLock { _file: file, path });
        }
        Ok(locks)
    }

    pub(super) fn check_tuple(&self, tuple: &Value) -> Result<()> {
        let path = self.invalidation_path(tuple)?;
        match std::fs::read(path) {
            Ok(bytes) => {
                let record: Value = serde_json::from_slice(&bytes)?;
                if record["schema"] != "rhei.tuple-invalidation.v1"
                    || record["breach"]["qualification"] != *tuple
                {
                    return Err(BudgetError::corrupt("invalid external tuple invalidation"));
                }
                Err(BudgetError::new("breach", "qualification tuple invalidated by a provider-spend breach in this authority; new reviewed qualification is required"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

impl TupleLock {
    /// Write ahead of the local receipt; a crash or failed append still stops
    /// admissions in other projects. The full actual charge remains retained.
    /// §FS-rhei-budgets.9 §AR-neural-admission.6
    pub(super) fn invalidate(
        &self,
        project: &str,
        payload: &Value,
        audit: &super::Audit,
    ) -> Result<()> {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "rhei.tuple-invalidation.v1", "project_id": project, "breach": payload, "audit": audit
        }))?;
        if self.path.exists() {
            // Another project's first breach already fences this exact tuple.
            // Its record is never replaced by a later one.
            let existing: Value = serde_json::from_slice(&std::fs::read(&self.path)?)?;
            if existing["schema"] != "rhei.tuple-invalidation.v1"
                || existing["breach"]["qualification"] != payload["qualification"]
            {
                return Err(BudgetError::corrupt("conflicting tuple invalidation authority"));
            }
            return Ok(());
        }
        let pending = self.path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&pending)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(pending, &self.path)?;
        super::journal::sync_directory(self.path.parent().expect("tuple parent"))
    }
}
