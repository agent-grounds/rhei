//! A plan, a workspace index and a task file saved with a leading byte order mark
//! read exactly as the same bytes without it. §REQ-cross-platform.9

use super::*;

const MARK: &str = "\u{feff}";

const PLAN: &str = "# Rhei: Example

---
metadata:
  tasks:
    1:
      stateVisits:
        review: 2
---

## Tasks

### Task 1: Alpha
**State:** pending
";

const INDEX: &str = "# Rhei: Workspace

---
metadata:
  tasks:
    setup:
      retryCount: 1
---

## Overview
Context
";

const TASK_FILE: &str = "---
metadata:
  tasks:
    a:
      priority: high
---

### Task a: Alpha
**State:** pending
";

#[test]
fn a_plan_with_a_leading_byte_order_mark_reads_as_the_same_plan() {
    let plain = parse(PLAN).expect("the plan parses without the mark");
    let marked = parse(&format!("{MARK}{PLAN}")).expect("the plan parses with one leading mark");
    assert_eq!(format!("{marked:?}"), format!("{plain:?}"));
}

#[test]
fn a_workspace_index_with_a_leading_byte_order_mark_reads_as_the_same_index() {
    let plain = parse_workspace_index(INDEX).expect("the index parses without the mark");
    let marked =
        parse_workspace_index(&format!("{MARK}{INDEX}")).expect("the index parses with the mark");
    assert_eq!(format!("{marked:?}"), format!("{plain:?}"));
}

#[test]
fn a_task_file_with_a_leading_byte_order_mark_keeps_its_frontmatter() {
    let plain = parse_workspace_task_file(TASK_FILE, None).expect("parses without the mark");
    assert!(plain.metadata.is_some(), "the fixture's block is read without the mark");
    let marked = parse_workspace_task_file(&format!("{MARK}{TASK_FILE}"), None)
        .expect("parses with the mark");
    assert_eq!(format!("{:?}", marked.metadata), format!("{:?}", plain.metadata));
    assert_eq!(format!("{:?}", marked.tasks), format!("{:?}", plain.tasks));
}
