//! The task tooling fields, `**MCP servers:**` then `**Skills:**`, read as the
//! last lines of a task's metadata block. Kept beside the plan parser so the
//! grammar's ordering rules for them live in one place.
//! §FS-rhei-plan-language.2 §FS-rhei-task-tooling.1

use crate::task_tooling::{parse_tooling_value, MCP_SERVERS_FIELD, SKILLS_FIELD};

use super::builder::NodeBuilder;
use super::{ParseError, Result};

/// Every metadata marker the grammar places before the tooling fields.
const EARLIER_FIELDS: [&str; 9] = [
    "**State:**",
    "**Prior:**",
    "**Inherits:**",
    "**Provides:**",
    "**Consumes:**",
    "**Excludes:**",
    "**Assignee:**",
    "**Model:**",
    "**Target:**",
];

/// Reads `line` when it is a task tooling field, and refuses an earlier field
/// authored after one. Returns whether the line was consumed; anything else is
/// left to the rest of the parser.
pub(super) fn read_line(
    top: Option<&mut NodeBuilder>,
    line: &str,
    line_number: usize,
) -> Result<bool> {
    let field = [MCP_SERVERS_FIELD, SKILLS_FIELD].into_iter().find(|f| line.starts_with(f));
    let Some(top) = top else {
        return match field {
            Some(_) => {
                Err(ParseError::new("Metadata field appears outside a task", Some(line_number)))
            }
            None => Ok(false),
        };
    };
    let Some(field) = field else {
        return refuse_field_after_tooling(top, line, line_number).map(|()| false);
    };
    // A blank line leaves the block open, as for every recognized field; only
    // task content closes it, and a field after content is refused. §FS-rhei-task-tooling.1
    let refuse = |message: String| Err(ParseError::new(message, Some(line_number)));
    if top.metadata_closed {
        return refuse(
            "Metadata fields must appear immediately after the task heading before task content"
                .to_string(),
        );
    }
    if top.state.is_none() {
        return refuse(format!("**State:** must appear before {field} for Task {}", top.id));
    }
    let is_mcp = field == MCP_SERVERS_FIELD;
    if is_mcp && !top.tooling.skills.is_empty() {
        return refuse(format!(
            "{MCP_SERVERS_FIELD} must appear before {SKILLS_FIELD} for Task {}",
            top.id
        ));
    }
    let id = top.id.clone();
    let slot = if is_mcp { &mut top.tooling.mcp_servers } else { &mut top.tooling.skills };
    if !slot.is_empty() {
        return refuse(format!("Duplicate {field} metadata for Task {id}"));
    }
    match parse_tooling_value(field, &line[field.len()..]) {
        Ok(entries) => *slot = entries,
        Err(message) => return refuse(format!("{message} (Task {id})")),
    }
    Ok(true)
}

/// A field the grammar places earlier, written after a tooling field while the
/// block is still open, is out of order. §FS-rhei-task-tooling.1
fn refuse_field_after_tooling(top: &NodeBuilder, line: &str, line_number: usize) -> Result<()> {
    if top.metadata_closed {
        return Ok(());
    }
    let tooling = if !top.tooling.mcp_servers.is_empty() {
        MCP_SERVERS_FIELD
    } else if !top.tooling.skills.is_empty() {
        SKILLS_FIELD
    } else {
        return Ok(());
    };
    match EARLIER_FIELDS.iter().find(|marker| line.starts_with(*marker)) {
        Some(marker) => Err(ParseError::new(
            format!("{marker} must appear before {tooling} for Task {}", top.id),
            Some(line_number),
        )),
        None => Ok(()),
    }
}
