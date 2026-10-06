//! The project's retirement record: the ids `rhei remove` took back, which no
//! later create, placement or hand edit may make live again.
//!
//! The record is one map, `metadata.retiredTickets`, keyed by project-qualified
//! id in the project's one metadata document — the manifest of a Panta project,
//! the lone rhei's own document otherwise (§FS-rhei-remove.5.1). It is read
//! here and nowhere else, so every surface that honours retirement — numbering,
//! placement, validation — agrees on what a well-formed record is.

use std::collections::BTreeMap;

use serde_yaml::{Mapping, Value};

use crate::ast::Metadata;
use crate::metadata::BUDGET_TICKET_ID_KEY;

/// The reserved key under `metadata` that holds the record. §FS-rhei-remove.5.1
pub const RETIRED_TICKETS_KEY: &str = "retiredTickets";

/// One retired id's record: empty unless the ticket had a budget identity,
/// which moves into it unchanged. §FS-rhei-remove.5.1
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetiredTicket {
    pub budget_ticket_id: Option<String>,
}

/// Every retired id, keyed by its project-qualified id.
pub type RetiredTickets = BTreeMap<String, RetiredTicket>;

/// Read the record out of a frontmatter mapping.
///
/// An absent key is an empty record. A key whose value is not a retirement map
/// — not a mapping, a non-string key, an entry that is not a mapping, an entry
/// carrying anything but a string `budgetTicketId` — is an error naming the
/// offending part, because the key is reserved and a record rhei cannot read is
/// one it cannot honour. §FS-rhei-remove.5.1 §FS-rhei-validate.4
pub fn retired_tickets(metadata: Option<&Metadata>) -> Result<RetiredTickets, String> {
    let Some(value) = metadata
        .and_then(|root| root.get("metadata"))
        .and_then(Value::as_mapping)
        .and_then(|section| section.get(RETIRED_TICKETS_KEY))
    else {
        return Ok(RetiredTickets::new());
    };
    if value.is_null() {
        return Ok(RetiredTickets::new());
    }
    let Some(map) = value.as_mapping() else {
        return Err(format!(
            "metadata.{RETIRED_TICKETS_KEY} is reserved for the retirement record and must be a \
             map keyed by ticket id"
        ));
    };
    let mut out = RetiredTickets::new();
    for (key, entry) in map {
        let Some(id) = key.as_str() else {
            return Err(format!(
                "metadata.{RETIRED_TICKETS_KEY} has a key that is not a ticket id: {}",
                describe(key)
            ));
        };
        out.insert(id.to_string(), retired_entry(id, entry)?);
    }
    Ok(out)
}

fn retired_entry(id: &str, entry: &Value) -> Result<RetiredTicket, String> {
    if entry.is_null() {
        return Ok(RetiredTicket::default());
    }
    let Some(fields) = entry.as_mapping() else {
        return Err(format!(
            "metadata.{RETIRED_TICKETS_KEY}.{id} must be a map (`{{}}` or `{{{BUDGET_TICKET_ID_KEY}: \
             <uuid>}}`), not {}",
            describe(entry)
        ));
    };
    let mut record = RetiredTicket::default();
    for (field, value) in fields {
        match (field.as_str(), value.as_str()) {
            (Some(BUDGET_TICKET_ID_KEY), Some(identity)) => {
                record.budget_ticket_id = Some(identity.to_string());
            }
            _ => {
                return Err(format!(
                    "metadata.{RETIRED_TICKETS_KEY}.{id} may hold only a string \
                     `{BUDGET_TICKET_ID_KEY}`; found `{}`",
                    describe(field)
                ));
            }
        }
    }
    Ok(record)
}

fn describe(value: &Value) -> String {
    serde_yaml::to_string(value).map(|text| text.trim().to_string()).unwrap_or_default()
}

/// Add one retirement to a frontmatter mapping, creating `metadata` and the
/// record as needed and keeping every other key and entry as it was.
///
/// Refuses, rather than overwrites, a key that is not already a retirement map.
/// §FS-rhei-remove.5.1
pub fn record_retirement(
    metadata: &mut Metadata,
    id: &str,
    record: &RetiredTicket,
) -> Result<(), String> {
    retired_tickets(Some(metadata))?;
    let section = metadata
        .entry(Value::String("metadata".into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if section.is_null() {
        *section = Value::Mapping(Mapping::new());
    }
    let Some(section) = section.as_mapping_mut() else {
        return Err("frontmatter `metadata` is not a map".to_string());
    };
    let retired = section
        .entry(Value::String(RETIRED_TICKETS_KEY.into()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if retired.is_null() {
        *retired = Value::Mapping(Mapping::new());
    }
    let retired = retired.as_mapping_mut().expect("validated as a retirement map above");
    let mut entry = Mapping::new();
    if let Some(identity) = &record.budget_ticket_id {
        entry.insert(BUDGET_TICKET_ID_KEY.into(), Value::String(identity.clone()));
    }
    retired.insert(Value::String(id.to_string()), Value::Mapping(entry));
    Ok(())
}

/// The id segments retired directly under one parent of one rhei — what
/// generated numbering must count as taken and an explicit id may not reuse. `parent_local` is the parent's
/// rhei-local id, `None` for the rhei's top level. §FS-rhei-new.4
pub fn retired_sibling_segments(
    retired: &RetiredTickets,
    rhei_id: &str,
    parent_local: Option<&str>,
) -> Vec<String> {
    let prefix = match parent_local {
        Some(parent) => format!("{rhei_id}.{parent}."),
        None => format!("{rhei_id}."),
    };
    retired
        .keys()
        .filter_map(|id| id.strip_prefix(&prefix))
        .filter(|rest| !rest.is_empty() && !rest.contains('.'))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "retired_tests.rs"]
mod tests;
