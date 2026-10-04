use super::*;
use serde_yaml::Value as YamlValue;

fn yaml_key(name: &str) -> YamlValue {
    YamlValue::String(name.to_string())
}

#[test]
fn parses_workspace_index_frontmatter_metadata() {
    let input = r#"# Rhei: Workspace

---
metadata:
  tasks:
    setup:
      retryCount: 1
---

## Overview
Context
"#;

    let index = parse_workspace_index(input).expect("parse ok");
    let metadata = index.metadata.expect("metadata should be present");
    let metadata_section = metadata
        .get(yaml_key("metadata"))
        .and_then(YamlValue::as_mapping)
        .expect("metadata section");
    let tasks = metadata_section
        .get(yaml_key("tasks"))
        .and_then(YamlValue::as_mapping)
        .expect("tasks metadata");
    let setup =
        tasks.get(yaml_key("setup")).and_then(YamlValue::as_mapping).expect("setup metadata");

    assert_eq!(setup.get(yaml_key("retryCount")).and_then(YamlValue::as_u64), Some(1));
}

#[test]
fn parses_panta_manifest_header_and_structure() {
    let input = r#"# Panta: Product Suite

---
structure:
  maxLevels: 3
  nodeKinds: [task, bug]
---

## Context
Shared project context.
"#;

    let manifest = parse_panta_manifest(input).expect("panta manifest parses");

    assert_eq!(manifest.title, "Product Suite");
    assert_eq!(manifest.structure.max_levels, 3);
    assert_eq!(manifest.structure.node_kinds, vec!["task", "bug"]);
    assert_eq!(manifest.content_sections[0].title, "Context");
}

#[test]
fn panta_manifest_rejects_tasks_section() {
    let err = parse_panta_manifest("# Panta: Project\n\n## Tasks\n").unwrap_err();

    assert!(
        err.message.contains("Panta manifest file must not contain a '## Tasks' section"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn errors_when_workspace_index_frontmatter_appears_before_header() {
    let input = r#"---
metadata:
  tasks:
    setup: {}
---

# Rhei: Workspace

## Overview
Context
"#;
    let err = parse_workspace_index(input).unwrap_err();
    assert!(
        err.message.contains("YAML frontmatter must appear after the `# Rhei:` header"),
        "unexpected message: {}",
        err.message
    );
    assert_eq!(err.line, Some(1));
}

#[test]
fn workspace_task_collect_reports_multiple_recoverable_errors_with_task_file_lines() {
    let input = r#"### Task 1: Missing state

### Task 2: Prior typo
**Prior** Task 1
**State:** pending

### Task 3: State typo
**State** pending
"#;

    let (tasks, errors) = parse_workspace_tasks_collect(input);

    assert!(tasks.is_some(), "recoverable errors should still produce remaining tasks");
    assert_eq!(errors.len(), 3);
    assert!(errors[0].message.contains("missing mandatory **State:**"));
    assert_eq!(errors[0].line, Some(1));
    assert!(errors[1].message.contains("Malformed metadata field"));
    assert_eq!(errors[1].line, Some(4));
    assert!(errors[2].message.contains("Malformed metadata field"));
    assert_eq!(errors[2].line, Some(8));
}

#[test]
fn workspace_task_parser_uses_index_structure_for_nested_tasks() {
    let index = parse_workspace_index(
        r#"# Rhei: Workspace

---
structure:
  maxLevels: 3
  nodeKinds: [task]
---

## Overview
Context
"#,
    )
    .expect("index parses");
    let input = r#"### Task feature: Feature
**State:** pending

#### Task feature.api: API
**State:** pending

##### Task feature.api.contract: Contract
**State:** pending
"#;

    let tasks = parse_workspace_tasks_with_structure(input, &index.structure)
        .expect("workspace task file should inherit index structure");

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].children.len(), 1);
    assert_eq!(tasks[0].children[0].children.len(), 1);
    assert_eq!(tasks[0].children[0].children[0].id.to_string(), "feature.api.contract");
}

/// §FS-rhei-memory.4.2: a manifest's content sections are pasted verbatim into
/// `### Project Context`, so their paragraph breaks survive the parse.
#[test]
fn manifest_content_sections_keep_their_interior_blank_lines() {
    let input = "# Panta: Product Suite\n\n\
                 ## Context\n\n\
                 First paragraph.\n\n\
                 Second paragraph.\n\n\
                 - item one\n\
                 - item two\n\n\
                 ```sh\n\
                 echo hi\n\n\
                 echo bye\n\
                 ```\n\n\
                 Tail paragraph.\n\n\n";

    let manifest = parse_panta_manifest(input).expect("panta manifest parses");

    assert_eq!(
        manifest.content_sections[0].content,
        "First paragraph.\n\nSecond paragraph.\n\n- item one\n- item two\n\n\
         ```sh\necho hi\n\necho bye\n```\n\nTail paragraph."
    );
}

/// §FS-rhei-memory.4.2: the same rule for a Directory Workspace's index, whose
/// sections become `### Rhei Context`.
#[test]
fn workspace_index_content_sections_keep_their_interior_blank_lines() {
    let input = "# Rhei: Workspace\n\n## Overview\n\nOne.\n\nTwo.\n\n## Rules\n\nOnly rule.\n";

    let index = parse_workspace_index(input).expect("workspace index parses");

    assert_eq!(index.content_sections[0].content, "One.\n\nTwo.");
    assert_eq!(index.content_sections[1].content, "Only rule.");
}
