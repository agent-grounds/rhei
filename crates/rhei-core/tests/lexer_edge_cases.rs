use rhei_core::ast::TaskId;
use rhei_core::{tokenize, Token};

const TASK_KIND: &str = "Task";

#[test]
fn malformed_structure_near_misses_fall_back_to_text_tokens() {
    // `### task 1: Wrong Case` is still matched because kind matching is
    // case-insensitive at semantic layers; here the lexer emits a NodeHeader
    // with the authored casing and the parser is the one that applies the
    // case-insensitive kind lookup. `#### Task 1: Missing Decimal Component`
    // is a valid node heading lexically; the parser rejects it because a
    // level-4 heading requires a 2-segment id. All other lines are invalid.
    let input = r#"#Rhei: Missing Space
##Tasks
###Task 1: Missing Heading Space
### task 1: Wrong Case
#### Task 1: Missing Decimal Component
**State** pending
**Prior** Task 1
### Task -bad: Invalid Named Id Boundary
### Task _bad: Invalid Named Id Boundary
### Task 1bad: Invalid Numeric Boundary
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    assert_eq!(
        tokens,
        vec![
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::NodeHeader { level: 3, kind: "task".to_string(), id: TaskId::number(1) },
            Token::NodeHeader { level: 4, kind: TASK_KIND.to_string(), id: TaskId::number(1) },
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
        ]
    );
}

#[test]
fn distinguishes_valid_named_task_ids_from_invalid_boundaries() {
    let input = r#"
### Task build-1_ok: Valid
### Task build--stage: Also Valid
### Task -build: Invalid Leading Hyphen
### Task 1build: Invalid Numeric Prefix
### Task build$: Invalid Trailing Character
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    assert_eq!(
        tokens,
        vec![
            Token::NodeHeader {
                level: 3,
                kind: TASK_KIND.to_string(),
                id: TaskId::named("build-1_ok"),
            },
            Token::NodeHeader {
                level: 3,
                kind: TASK_KIND.to_string(),
                id: TaskId::named("build--stage"),
            },
            Token::TextContent,
            Token::TextContent,
            Token::TextContent,
        ]
    );
}

#[test]
fn malformed_structure_inside_fenced_code_blocks_is_not_tokenized() {
    let input = r#"# Rhei: Example

## Tasks

### Task 1: Title
**State:** pending

```
# Rhei: Hidden Rhei
## Tasks
### Task 999: Should Not Tokenize
### task 2: Wrong Case
#### Subtask 1: Missing Decimal Component
**State** code-block near-miss
**Prior** Task 2
```

#Rhei: Missing Space Outside Fence
##Tasks
###Task 2: Missing Heading Space Outside Fence
**State** pending outside fence
**Prior** Task 3 outside fence
**Prior:** Task 1, Task 2
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::RheiHeader,
        Token::TasksSection,
        Token::NodeHeader { level: 3, kind: TASK_KIND.to_string(), id: TaskId::number(1) },
        Token::MetadataState { state: "pending".to_string() },
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::TextContent,
        Token::MetadataPrior { task_ids: vec![TaskId::number(1), TaskId::number(2)] },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn assignee_metadata_with_various_values() {
    let input = "\
**Assignee:** alice
**Assignee:**    trimmed
**Assignee:** multi word name
";

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::MetadataAssignee { name: "alice".to_string() },
        Token::MetadataAssignee { name: "trimmed".to_string() },
        Token::MetadataAssignee { name: "multi word name".to_string() },
    ];

    assert_eq!(tokens, expected);
}

#[test]
fn assignee_without_colon_is_plain_text() {
    let input = "\
**Assignee** alice
";

    let tokens: Vec<Token> = tokenize(input).collect();

    assert_eq!(tokens, vec![Token::TextContent]);
}

#[test]
fn state_metadata_backtick_escaping() {
    let input = "\
**State:** `in progress`
**State:** `ready to ship`
";

    let tokens: Vec<Token> = tokenize(input).collect();

    let expected = vec![
        Token::MetadataState { state: "in progress".to_string() },
        Token::MetadataState { state: "ready to ship".to_string() },
    ];

    assert_eq!(tokens, expected);
}

/// §FS-rhei-plan-language.2.1: the tokenizer reads a fence by the language's one
/// rule — `~~~` opens a block, and a run shorter than the open does not close
/// one — so a plan that quotes the plan format tokenizes as text.
#[test]
fn tilde_and_nested_fences_are_not_tokenized_as_structure() {
    let input = r#"# Rhei: Example

## Tasks

### Task 1: Title
**State:** pending

~~~markdown
## Tasks
### Task 999: Quoted in a tilde fence
**State:** quoted
~~~

````markdown
```console
$ rhei validate plan.rhei.md
### Task 998: Quoted past a run too short to close
**State:** quoted
````
"#;

    let tokens: Vec<Token> = tokenize(input).collect();

    let structural: Vec<&Token> =
        tokens.iter().filter(|token| !matches!(token, Token::TextContent)).collect();

    assert_eq!(
        structural,
        vec![
            &Token::RheiHeader,
            &Token::TasksSection,
            &Token::NodeHeader { level: 3, kind: TASK_KIND.to_string(), id: TaskId::number(1) },
            &Token::MetadataState { state: "pending".to_string() },
        ],
        "only the authored task is structure; got {tokens:?}"
    );
}
