//! The tooling one task names for its own agent invocations: the
//! `**MCP servers:**` and `**Skills:**` metadata fields, and the one reader of
//! their entry form, which the plan parser and `rhei new` both use.
//! §FS-rhei-task-tooling.1 §FS-rhei-plan-language.2

/// The metadata marker naming the MCP servers a task's agent needs.
pub const MCP_SERVERS_FIELD: &str = "**MCP servers:**";
/// The metadata marker naming the skills a task's agent needs.
pub const SKILLS_FIELD: &str = "**Skills:**";

/// The tooling fields, in the order the grammar places them last in the
/// metadata block: the one list the parser's tooling reader matches a line
/// against, and the tail of [`crate::tokens::TASK_METADATA_FIELDS`], so a field
/// added here is refused from a `rhei new` description by the same edit.
/// §FS-rhei-plan-language.2 §FS-rhei-new.3.4.2
pub const TOOLING_FIELDS: [&str; 2] = [MCP_SERVERS_FIELD, SKILLS_FIELD];

/// The one spelling that marks an entry optional; any other suffix is refused.
const OPTIONAL_SUFFIX: &str = " (optional)";

/// One registry id a task names, and whether it carries ` (optional)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskToolingEntry {
    /// Key in the merged `mcp_servers` or `skills` settings registry.
    pub id: String,
    /// Whether the entry may fail to start without failing the spawn.
    pub optional: bool,
}

impl TaskToolingEntry {
    /// Reads one entry: a registry id with no whitespace, `,`, `(` or `)`,
    /// optionally followed by exactly ` (optional)`. §FS-rhei-task-tooling.1
    pub fn parse(text: &str) -> Option<Self> {
        let (id, optional) = match text.strip_suffix(OPTIONAL_SUFFIX) {
            Some(id) => (id, true),
            None => (text, false),
        };
        let is_id = !id.is_empty()
            && !id.chars().any(|c| c.is_whitespace() || matches!(c, ',' | '(' | ')'));
        is_id.then(|| Self { id: id.to_string(), optional })
    }

    /// The entry as a plan spells it.
    pub fn authored(&self) -> String {
        if self.optional {
            format!("{}{OPTIONAL_SUFFIX}", self.id)
        } else {
            self.id.clone()
        }
    }
}

/// The registry ids a task adds to its own invocations, in authored order.
/// Children do not inherit them. §FS-rhei-task-tooling.2
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskTooling {
    /// Entries from `**MCP servers:**`; empty when the field is absent.
    pub mcp_servers: Vec<TaskToolingEntry>,
    /// Entries from `**Skills:**`; empty when the field is absent.
    pub skills: Vec<TaskToolingEntry>,
}

impl TaskTooling {
    /// Whether the task names nothing of either kind.
    pub fn is_empty(&self) -> bool {
        self.mcp_servers.is_empty() && self.skills.is_empty()
    }
}

/// Reads one field's value, a list separated by `, ` as `**Provides:**` is.
/// Every refusal names `field`. §FS-rhei-task-tooling.1
pub fn parse_tooling_value(field: &str, value: &str) -> Result<Vec<TaskToolingEntry>, String> {
    if value.trim().is_empty() {
        return Err(format!(
            "Empty {field} value: name at least one registry id, or drop the line"
        ));
    }
    parse_tooling_entries(field, value.split(','))
}

/// Reads entries already split one per item, refusing an empty item, a
/// malformed one, and an id named twice whatever its optional marker.
/// §FS-rhei-task-tooling.1
pub fn parse_tooling_entries<'a>(
    field: &str,
    items: impl IntoIterator<Item = &'a str>,
) -> Result<Vec<TaskToolingEntry>, String> {
    let mut entries: Vec<TaskToolingEntry> = Vec::new();
    for item in items {
        let text = item.trim();
        if text.is_empty() {
            return Err(format!(
                "Empty {field} entry: name a registry id between separators, or drop the separator"
            ));
        }
        let entry = TaskToolingEntry::parse(text).ok_or_else(|| {
            format!(
                "Malformed {field} entry '{text}': expected a registry id, optionally \
                 followed by ` (optional)`"
            )
        })?;
        if entries.iter().any(|held| held.id == entry.id) {
            return Err(format!("Duplicate {field} id '{}': name each registry id once", entry.id));
        }
        entries.push(entry);
    }
    Ok(entries)
}
