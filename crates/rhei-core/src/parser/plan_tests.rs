use super::*;
use crate::ast::{ContentSection, TaskId, TaskIdSegment};
use serde_yaml::Value as YamlValue;

fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

#[test]
fn parses_minimal_plan_with_hierarchical_tasks() {
    let input = r#"# Rhei: Example
Some intro line

## Tasks

### Task 1: Alpha
**State:** pending
**Prior:** Task 2

#### Task 1.1: Do A
**State:** pending
Line A1
Line A2

#### Task 1.2: Do B
**State:** completed
```
code block
```
"#;

    let rhei = parse(input).expect("parse ok");

    assert_eq!(rhei.title, "Example");
    assert!(rhei.content_sections.is_empty());

    assert_eq!(rhei.tasks.len(), 1);
    let t1 = &rhei.tasks[0];
    assert_eq!(t1.kind, "task");
    assert_eq!(t1.id, TaskId::number(1));
    assert_eq!(t1.title, "Alpha");
    assert_eq!(t1.state, "pending");
    assert_eq!(t1.prior, vec![TaskId::number(2)]);

    assert_eq!(t1.children.len(), 2);
    assert_eq!(t1.children[0].title, "Do A");
    assert_eq!(t1.children[0].state, "pending");
    assert_eq!(
        t1.children[0].id,
        TaskId::from_segments(vec![TaskIdSegment::Number(1), TaskIdSegment::Number(1)])
    );
    assert!(t1.children[0].content.contains("Line A1"));
    assert!(t1.children[0].content.contains("Line A2"));

    assert_eq!(t1.children[1].state, "completed");
    assert!(t1.children[1].content.contains("```"));
    assert!(t1.children[1].content.contains("code block"));
}

#[test]
fn parses_task_execution_overrides() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Model:** claude-opus-4-7

### Task 2: Beta
**State:** pending
**Target:** codex[yolo]:openai:gpt-5-codex
"#;

    let rhei = parse(input).expect("parse ok");

    assert_eq!(rhei.tasks[0].model.as_deref(), Some("claude-opus-4-7"));
    assert_eq!(rhei.tasks[0].target, None);
    assert_eq!(rhei.tasks[1].model, None);
    assert_eq!(rhei.tasks[1].target.as_deref(), Some("codex[yolo]:openai:gpt-5-codex"));
}

#[test]
fn parses_plan_frontmatter_metadata() {
    let input = r#"# Rhei: Example

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
"#;

    let rhei = parse(input).expect("parse ok");
    let metadata = rhei.metadata.expect("metadata should be present");
    let metadata_section = metadata
        .get(yaml_key("metadata"))
        .and_then(YamlValue::as_mapping)
        .expect("metadata section");
    let tasks = metadata_section
        .get(yaml_key("tasks"))
        .and_then(YamlValue::as_mapping)
        .expect("tasks metadata");
    let task = tasks
        .get(YamlValue::Number(1u64.into()))
        .and_then(YamlValue::as_mapping)
        .expect("task 1 metadata");
    let state_visits =
        task.get(yaml_key("stateVisits")).and_then(YamlValue::as_mapping).expect("stateVisits");

    assert_eq!(state_visits.get(yaml_key("review")).and_then(YamlValue::as_u64), Some(2));
}

#[test]
fn parses_structure_frontmatter_with_custom_node_kinds_and_depth() {
    let input = r#"# Rhei: Example
---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Tasks

### Task 1: Parent
**State:** pending

#### Bug 1.1: Child
**State:** pending

##### Task 1.1.1: Grandchild
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    assert_eq!(rhei.structure.max_levels, 3);
    assert_eq!(rhei.structure.node_kinds, vec!["task".to_string(), "bug".to_string()]);
    assert_eq!(rhei.tasks.len(), 1);
    assert_eq!(rhei.tasks[0].children.len(), 1);
    assert_eq!(rhei.tasks[0].children[0].kind, "bug");
    assert_eq!(rhei.tasks[0].children[0].children.len(), 1);
    assert_eq!(rhei.tasks[0].children[0].children[0].kind, "task");
}

#[test]
fn error_when_missing_rhei_title() {
    let input = "## Tasks\n";
    let err = parse(input).unwrap_err();
    assert!(err.message.contains("Missing '# Rhei"));
    assert_eq!(err.line, Some(1));
}

#[test]
fn errors_when_frontmatter_appears_before_rhei_header() {
    let input = r#"---
structure:
  nodeKinds: [task, bug]
---

# Rhei: Example

## Tasks

### Task 1: Hi
**State:** pending
"#;
    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("YAML frontmatter must appear after the `# Rhei:` header"),
        "unexpected message: {}",
        err.message
    );
    assert_eq!(err.line, Some(1));
}

#[test]
fn errors_when_missing_tasks_section() {
    let input = "# Rhei: Example\n";
    let err = parse(input).unwrap_err();

    assert_eq!(err.message, "Missing '## Tasks' section");
    assert_eq!(err.line, None);
}

#[test]
fn allows_an_empty_tasks_section() {
    // §FS-rhei-plan-language.1.1: an empty rhei is valid in both formats.
    let input = "# Rhei: Example\n## Tasks\n";
    let rhei = parse(input).expect("an empty Tasks section is a valid, empty rhei");

    assert_eq!(rhei.title, "Example");
    assert!(rhei.tasks.is_empty());
}

#[test]
fn allows_arbitrary_h2_chapters_before_tasks_section() {
    let input = r#"# Rhei: Example

## Overview
High-level context.

## Requirements
- Preserve audit logs
- Support approvals

## Tasks

### Task 1: Alpha
**State:** pending
"#;
    let rhei = parse(input).expect("parse ok");

    assert_eq!(rhei.title, "Example");
    assert_eq!(rhei.tasks.len(), 1);
    assert_eq!(
        rhei.content_sections,
        vec![
            ContentSection {
                title: "Overview".to_string(),
                content: "High-level context.".to_string(),
                rhei: None,
            },
            ContentSection {
                title: "Requirements".to_string(),
                content: "- Preserve audit logs\n- Support approvals".to_string(),
                rhei: None,
            },
        ]
    );
}

#[test]
fn errors_when_tasks_section_is_not_final_h2_chapter() {
    let input = r#"# Rhei: Example

## Overview
Context before tasks.

## Tasks

### Task 1: Alpha
**State:** pending

## Appendix
Trailing chapter after tasks.
"#;
    let err = parse(input).unwrap_err();

    assert_eq!(
        err.message,
        "Tasks section must be the final '##' chapter and appear as '## Tasks'"
    );
    assert_eq!(err.line, Some(11));
}

#[test]
fn parses_named_task_ids_and_named_prior_dependencies() {
    let input = r#"# Rhei: Example
## Tasks

### Task build_api: Build API
**State:** in-progress
**Prior:** Task setup_db, Task 2

#### Task build_api.endpoint: Implement endpoint
**State:** pending
Body
"#;

    let rhei = parse(input).expect("parse ok");

    assert_eq!(rhei.tasks.len(), 1);
    let task = &rhei.tasks[0];
    assert_eq!(task.id, TaskId::named("build_api"));
    assert_eq!(task.title, "Build API");
    assert_eq!(task.state, "in-progress");
    assert_eq!(task.prior, vec![TaskId::named("setup_db"), TaskId::number(2)]);
    assert_eq!(task.children.len(), 1);
    assert_eq!(
        task.children[0].id,
        TaskId::from_segments(vec![
            TaskIdSegment::Named("build_api".to_string()),
            TaskIdSegment::Named("endpoint".to_string())
        ])
    );
}

#[test]
fn errors_when_metadata_after_content() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
Task description closes metadata window.
**Prior:** Task 2
"#;

    let err = parse(input).unwrap_err();

    assert_eq!(
        err.message,
        "Metadata fields must appear immediately after the task heading before task content"
    );
    assert_eq!(err.line, Some(7));
}

#[test]
fn prior_before_state_is_parse_error() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Prior:** Task 2
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("**State:** must appear before **Prior:**"));
}

#[test]
fn errors_on_malformed_task_heading_in_tasks_section() {
    // `### Tak 3:` parses as kind `Tak`, which is not declared.
    let input = r#"# Rhei: Example
## Tasks

### Tak 3: Broken heading
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("unknown node kind"));
    assert_eq!(err.line, Some(4));
}

#[test]
fn errors_on_unknown_node_kind() {
    let input = r#"# Rhei: Example
## Tasks

### Spike 1: Investigate
**State:** pending
"#;
    let err = parse(input).unwrap_err();
    assert!(err.message.contains("unknown node kind"));
}

#[test]
fn errors_on_child_id_that_does_not_extend_parent() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 2.1: Wrong parent
**State:** pending
"#;
    let err = parse(input).unwrap_err();
    assert!(err.message.contains("must extend parent id"));
}

#[test]
fn rejects_numeric_task_id_segments_with_leading_zeroes() {
    let input = r#"# Rhei: Example
## Tasks

### Task 01: Alpha
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("Malformed node heading"));
    assert_eq!(err.line, Some(4));
}

#[test]
fn rejects_prior_id_segments_with_leading_zeroes_instead_of_partial_match() {
    let input = r#"# Rhei: Example
## Tasks

### Task 0: Zero
**State:** pending

### Task 1: Alpha
**State:** pending
**Prior:** Task 01
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("Malformed **Prior:** reference 'Task 01'"),
        "the error must quote what was written; got: {}",
        err.message
    );
    assert_eq!(err.line, Some(9));
}

/// §FS-rhei-plan-language.5: every surface that prints a dependency prints the
/// bare id (`rhei list` renders `(prior: auth.2)`), so pasting that back must
/// parse. The kind keyword is decoration the parser already discarded.
#[test]
fn accepts_prior_references_with_and_without_the_kind_keyword() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

### Task 2: Beta
**State:** pending
**Prior:** 1

### Task 3: Gamma
**State:** pending
**Prior:** Task 1, 2
"#;

    let rhei = parse(input).expect("bare and prefixed prior refs both parse");
    assert_eq!(rhei.tasks[1].prior.iter().map(|id| id.to_string()).collect::<Vec<_>>(), ["1"]);
    assert_eq!(rhei.tasks[2].prior.iter().map(|id| id.to_string()).collect::<Vec<_>>(), ["1", "2"]);
}

#[test]
fn rejects_numeric_task_id_segments_outside_u32_range() {
    let input = r#"# Rhei: Example
## Tasks

### Task 4294967296: Alpha
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("malformed task id"));
    assert_eq!(err.line, Some(4));
}

#[test]
fn errors_on_depth_over_structure_max_levels() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending

#### Task 1.1: Beta
**State:** pending

##### Task 1.1.1: Too deep
**State:** pending
"#;
    let err = parse(input).unwrap_err();
    assert!(err.message.contains("exceeds `structure.maxLevels`"));
}

/// §FS-rhei-memory.4.2: a content section is pasted verbatim into a prompt, so
/// the blank lines between its paragraphs, lists, and fences are part of it.
#[test]
fn content_sections_keep_their_interior_blank_lines() {
    let input = "# Rhei: Example\n\n\
                 ## Overview\n\n\
                 First paragraph.\n\n\
                 Second paragraph.\n\n\
                 - item one\n\
                 - item two\n\n\
                 ```sh\n\
                 echo hi\n\n\
                 echo bye\n\
                 ```\n\n\
                 Tail paragraph.\n\n\n\
                 ## Tasks\n\n\
                 ### Task 1: Alpha\n\
                 **State:** pending\n";

    let rhei = parse(input).expect("parse ok");

    assert_eq!(
        rhei.content_sections,
        vec![ContentSection {
            title: "Overview".to_string(),
            // The trailing blanks only separate the section from `## Tasks`.
            content: "First paragraph.\n\nSecond paragraph.\n\n- item one\n- item two\n\n\
                      ```sh\necho hi\n\necho bye\n```\n\nTail paragraph."
                .to_string(),
            rhei: None,
        }]
    );
}

/// §FS-rhei-memory.4.2: the bytes a section holds are the bytes the author
/// wrote between its heading and the next one.
#[test]
fn a_content_section_round_trips_the_authored_block() {
    let block = "Paragraph one.\n\nParagraph two.\n\n- a\n- b\n\n```text\nfenced\n```";
    let input = format!(
        "# Rhei: Example\n\n## Notes\n\n{block}\n\n## Tasks\n\n\
         ### Task 1: Alpha\n**State:** pending\n"
    );

    let rhei = parse(&input).expect("parse ok");

    assert_eq!(rhei.content_sections[0].content, block);
}
