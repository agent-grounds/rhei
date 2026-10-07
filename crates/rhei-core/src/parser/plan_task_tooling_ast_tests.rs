//! What the task tooling fields leave on the task AST: each entry's id and
//! marker in authored order, and empty lists where the fields are absent.
//! §FS-rhei-task-tooling.1 §FS-rhei-task-tooling.2

use super::*;
use crate::ast::TaskToolingEntry;

fn entries(pairs: &[(&str, bool)]) -> Vec<TaskToolingEntry> {
    pairs
        .iter()
        .map(|(id, optional)| TaskToolingEntry { id: id.to_string(), optional: *optional })
        .collect()
}

const TOOLED: &str = "# Rhei: Tooling
## Tasks

### Task 1: Mail
**State:** pending
**MCP servers:** thunderbird-mail, grafana (optional)
**Skills:** release-notes, triage (optional)
Body

### Task 2: Plain
**State:** pending
";

#[test]
fn task_mcp_entries_keep_their_order_and_marker() {
    let plan = parse(TOOLED).expect("parse");
    assert_eq!(
        plan.tasks[0].tooling.mcp_servers,
        entries(&[("thunderbird-mail", false), ("grafana", true)])
    );
}

#[test]
fn task_skill_entries_keep_their_order_and_marker() {
    let plan = parse(TOOLED).expect("parse");
    assert_eq!(
        plan.tasks[0].tooling.skills,
        entries(&[("release-notes", false), ("triage", true)])
    );
}

#[test]
fn a_task_without_the_fields_has_both_lists_empty() {
    let plan = parse(TOOLED).expect("parse");
    assert!(plan.tasks[1].tooling.mcp_servers.is_empty());
    assert!(plan.tasks[1].tooling.skills.is_empty());
}

/// Children name their own tooling; nothing flows down from the parent.
#[test]
fn a_child_does_not_inherit_its_parents_tooling() {
    let plan = parse(
        "# Rhei: Tooling\n## Tasks\n\n### Task 1: Parent\n**State:** pending\n\
         **MCP servers:** grafana\n\n#### Task 1.1: Child\n**State:** pending\n",
    )
    .expect("parse");
    assert!(plan.tasks[0].children[0].tooling.is_empty());
}

/// Past the blank line that closes the block, the line is task content.
#[test]
fn a_tooling_line_after_the_block_is_content() {
    let plan = parse(
        "# Rhei: Tooling\n## Tasks\n\n### Task 1: Mail\n**State:** pending\n\n\
         **MCP servers:** grafana\n",
    )
    .expect("parse");
    assert!(plan.tasks[0].tooling.is_empty());
    assert!(plan.tasks[0].content.contains("**MCP servers:** grafana"));
}
