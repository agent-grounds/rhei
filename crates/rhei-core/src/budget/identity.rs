//! Ticket identity bindings survive missing metadata writes and relocated roots.
//! §FS-rhei-budgets.2.3

use super::{Audit, BudgetError, Journal, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

impl Journal {
    pub fn identities(&self) -> &BTreeMap<String, Value> {
        &self.state.identities
    }

    /// Include sources from every registered copy, not just the latest root.
    /// The driver locks these paths before reading any binding. §FS-rhei-budgets.2.3
    pub fn identity_sources(&self) -> Result<BTreeSet<PathBuf>> {
        let mut paths = BTreeSet::new();
        for binding in self.state.identities.values() {
            paths.extend(sources(binding)?);
        }
        Ok(paths)
    }

    /// Validate even identities absent from the selected root under the
    /// driver's held source locks. §FS-rhei-budgets.2.3
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

    /// The driver holds sorted metadata/source locks. A second live source
    /// must contain the same bytes; a missing old source permits a move.
    /// §FS-rhei-budgets.2.3 §AR-neural-admission.3
    pub fn bind_ticket(
        &mut self,
        ticket: &str,
        display: &str,
        source: &Path,
        pending_write: bool,
        audit: &Audit,
    ) -> Result<()> {
        let prefix = format!("ticket:{}:", self.project_id.trim_start_matches("panta:"));
        let id = ticket
            .strip_prefix(&prefix)
            .ok_or_else(|| BudgetError::corrupt("ticket belongs to another allowance"))?;
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
            "status": if pending_write { "pending" } else { "installed" }}),
            audit,
        )
    }

    pub fn finish_ticket_binding(&mut self, ticket: &str, audit: &Audit) -> Result<()> {
        let mut binding = self
            .state
            .identities
            .get(ticket)
            .ok_or_else(|| BudgetError::corrupt("missing identity intent"))?
            .clone();
        if binding["status"] == "installed" {
            return Ok(());
        }
        binding["status"] = "installed".into();
        self.append("identity", binding, audit)
    }
}

/// Legacy receipts accumulated bindings too; replay must not discard them.
/// §FS-rhei-budgets.2.3
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
    if paths.iter().any(|p| !p.is_absolute()) {
        return Err(BudgetError::corrupt("identity source path must be absolute"));
    }
    Ok(paths)
}

fn read_live_source(path: &Path) -> Result<Option<String>> {
    match crate::source::read_to_string(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn conflict() -> BudgetError {
    BudgetError::new("identity_conflict", "ticket identity has conflicting live content")
}
