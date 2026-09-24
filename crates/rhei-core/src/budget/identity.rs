//! A ticket's identity survives a relocated root and a rewritten plan file.
//!
//! The binding is by source path **and** content hash, so several live sources
//! are tolerated only while their bytes agree. That is what makes a copied or
//! moved ticket keep its travel, and what makes two documents claiming one
//! identity with different content a conflict rather than a fork.
//! §FS-rhei-budgets.5.2

use super::journal::{Audit, Journal};
use super::types::BudgetError;
use super::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

impl Journal {
    pub fn identities(&self) -> &BTreeMap<String, Value> {
        &self.state.identities
    }

    /// Whether this ticket identity is durably installed and may be reserved
    /// against. §FS-rhei-budgets.6.1
    pub fn identity_installed(&self, ticket: &str) -> bool {
        self.state
            .identities
            .get(ticket)
            .is_some_and(|binding| binding["status"] == "installed")
    }

    /// Every registered copy's path, not just the latest root. The driver locks
    /// these before reading any binding. §AR-neural-admission.3
    pub fn identity_sources(&self) -> Result<BTreeSet<PathBuf>> {
        let mut paths = BTreeSet::new();
        for binding in self.state.identities.values() {
            paths.extend(sources(binding)?);
        }
        Ok(paths)
    }

    /// Validate every identity, including ones absent from the selected root.
    /// §FS-rhei-budgets.5.2
    pub fn validate_identity_sources(&self) -> Result<()> {
        for binding in self.state.identities.values() {
            let mut content = None;
            for source in sources(binding)? {
                if let Some(bytes) = read_live_source(&source)? {
                    if content.as_ref().is_some_and(|old| old != &bytes) {
                        return Err(conflict());
                    }
                    content = Some(bytes);
                }
            }
        }
        Ok(())
    }

    /// Record where this ticket identity lives.
    ///
    /// A second live source must contain the same bytes; a missing old source
    /// permits a move. Rebinding the same path is idempotent, which is what
    /// lets an ordinary plan rewrite — a state change, a visit counter, `rhei
    /// reset` restoring the authored state — leave the identity alone.
    /// §FS-rhei-budgets.5.2
    pub fn bind_ticket(
        &mut self,
        ticket: &str,
        display: &str,
        source: &Path,
        audit: &Audit,
    ) -> Result<()> {
        let prefix = format!("ticket:{}:", self.project_id.trim_start_matches("panta:"));
        let id = ticket
            .strip_prefix(&prefix)
            .ok_or_else(|| BudgetError::corrupt("ticket belongs to another account"))?;
        super::types::uuid(id)?;
        let source = std::fs::canonicalize(source)?;
        let bytes = crate::source::read_to_string(&source)?;
        let mut live = BTreeSet::from([source.clone()]);
        if let Some(previous) = self.state.identities.get(ticket) {
            let previous_sources = sources(previous)?;
            for old in &previous_sources {
                if let Some(existing) = read_live_source(old)? {
                    if existing != bytes {
                        return Err(conflict());
                    }
                    live.insert(old.clone());
                }
            }
            if previous_sources == live && previous["display_id"] == display {
                return Ok(());
            }
        }
        self.append(
            "identity",
            json!({"ticket_identity": ticket, "display_id": display,
            "source_path": source, "source_paths": live,
            "source_hash": super::journal::digest(bytes.as_bytes()),
            "status": "installed"}),
            audit,
        )
    }
}

/// Earlier receipts accumulated bindings too; replay must not discard them.
/// §FS-rhei-budgets.5.2
pub(crate) fn replay_binding(previous: Option<&Value>, payload: &Value) -> Result<Value> {
    let mut binding = payload.clone();
    let mut paths = sources(payload)?;
    if payload.get("source_paths").is_none() {
        if let Some(previous) = previous {
            paths.extend(sources(previous)?);
        }
    }
    binding["source_paths"] = serde_json::to_value(paths)?;
    Ok(binding)
}

fn sources(binding: &Value) -> Result<BTreeSet<PathBuf>> {
    let current = super::replay::string(binding, "source_path")?;
    let mut paths = match binding.get("source_paths") {
        Some(value) => serde_json::from_value::<BTreeSet<PathBuf>>(value.clone())?,
        None => BTreeSet::new(),
    };
    paths.insert(current.into());
    if paths.iter().any(|path| !path.is_absolute()) {
        return Err(BudgetError::corrupt("identity source path must be absolute"));
    }
    Ok(paths)
}

fn read_live_source(path: &Path) -> Result<Option<String>> {
    match crate::source::read_to_string(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn conflict() -> BudgetError {
    BudgetError::new("identity_conflict", "ticket identity has conflicting live content")
}
