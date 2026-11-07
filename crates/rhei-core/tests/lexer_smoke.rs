use rhei_core::ast::{TaskId, TaskIdSegment};
use rhei_core::{tokenize, Token};

#[test]
fn tokenizes_basic_structure() {
    let input = r#"# Rhei: User Authentication System

## Tasks

### Task 1: Database Schema Design
**State:** completed

#### Task 1.1: Define User Table
Some description line

**Prior:** Task 1, Task 2
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::NodeHeader { level: 3, kind: "Task".to_string(), id: TaskId::number(1) },
        Token::MetadataState { state: "completed".to_string() },
        Token::NodeHeader {
            level: 4,
            kind: "Task".to_string(),
            id: TaskId::from_segments(vec![TaskIdSegment::Number(1), TaskIdSegment::Number(1)]),
        },
        Token::TextContent,
        Token::MetadataPrior { task_ids: vec![TaskId::number(1), TaskId::number(2)] },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn tokenizes_assignee_after_state_and_prior() {
    let input = r#"# Rhei: Assigned
## Tasks

### Task 1: Alpha
**State:** in-progress
**Prior:** Task 2
**Assignee:** alice
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::NodeHeader { level: 3, kind: "Task".to_string(), id: TaskId::number(1) },
        Token::MetadataState { state: "in-progress".to_string() },
        Token::MetadataPrior { task_ids: vec![TaskId::number(2)] },
        Token::MetadataAssignee { name: "alice".to_string() },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn tokenizes_task_execution_overrides() {
    let input = r#"# Rhei: Overrides
## Tasks

### Task 1: Alpha
**State:** pending
**Model:** claude-opus-4-7

### Task 2: Beta
**State:** pending
**Target:** codex[yolo]:openai:gpt-5-codex
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::NodeHeader { level: 3, kind: "Task".to_string(), id: TaskId::number(1) },
        Token::MetadataState { state: "pending".to_string() },
        Token::MetadataModel { model: "claude-opus-4-7".to_string() },
        Token::NodeHeader { level: 3, kind: "Task".to_string(), id: TaskId::number(2) },
        Token::MetadataState { state: "pending".to_string() },
        Token::MetadataTarget { target: "codex[yolo]:openai:gpt-5-codex".to_string() },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn tokenizes_named_task_ids_and_prior_references() {
    let input = r#"# Rhei: Named Tasks

## Tasks

### Task bootstrap_env: Bootstrap environments
**State:** in-progress
**Prior:** Task seed_data, Task 2

### Task seed_data: Seed database
**State:** pending
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::NodeHeader {
            level: 3,
            kind: "Task".to_string(),
            id: TaskId::named("bootstrap_env"),
        },
        Token::MetadataState { state: "in-progress".to_string() },
        Token::MetadataPrior { task_ids: vec![TaskId::named("seed_data"), TaskId::number(2)] },
        Token::NodeHeader { level: 3, kind: "Task".to_string(), id: TaskId::named("seed_data") },
        Token::MetadataState { state: "pending".to_string() },
    ];

    assert_eq!(tokens, expected);
}

// §FS-rhei-plan-language.2.1: a frontmatter value is text, whatever it looks like.
#[test]
fn a_frontmatter_value_opens_no_fence_and_declares_no_node() {
    let input = "# Rhei: Plan\n\n---\nnote: \"\n```\n\n### Task 9: decoy\n\"\n---\n\n## Tasks\n\n### Task 1: A\n";
    let nodes: Vec<String> = tokenize(input)
        .filter_map(|token| match token {
            Token::NodeHeader { id, .. } => Some(id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(nodes, ["1"]);
}
