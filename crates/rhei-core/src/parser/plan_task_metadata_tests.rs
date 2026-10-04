//! The task metadata fields that follow `**State:**` and `**Prior:**` — the
//! `**Assignee:**`, the exports `**Provides:**` and `**Consumes:**`, and the
//! read boundary `**Excludes:**` — and where each may sit in the block.
//! §FS-rhei-plan-language.2 §FS-rhei-plan-language.3.12 §FS-rhei-plan-language.3.13

use super::*;
use crate::ast::TaskId;

#[test]
fn parses_assignee_when_present() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** in-progress
**Prior:** Task 2
**Assignee:** alice
Body
"#;

    let rhei = parse(input).expect("parse ok");
    let task = &rhei.tasks[0];
    assert_eq!(task.state, "in-progress");
    assert_eq!(task.prior, vec![TaskId::number(2)]);
    assert_eq!(task.assignee.as_deref(), Some("alice"));
    assert!(task.content.contains("Body"));
}

#[test]
fn parses_assignee_without_prior() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Assignee:** bob
"#;

    let rhei = parse(input).expect("parse ok");
    assert_eq!(rhei.tasks[0].prior, Vec::<TaskId>::new());
    assert_eq!(rhei.tasks[0].assignee.as_deref(), Some("bob"));
}

#[test]
fn parses_task_without_assignee_leaves_none() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
"#;

    let rhei = parse(input).expect("parse ok");
    assert_eq!(rhei.tasks[0].assignee, None);
}

#[test]
fn errors_when_assignee_before_state() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Assignee:** alice
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("**State:** must appear before **Assignee:**"),
        "unexpected message: {}",
        err.message
    );
}

#[test]
fn errors_when_duplicate_assignee() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Assignee:** alice
**Assignee:** bob
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("Duplicate **Assignee:**"), "unexpected message: {}", err.message);
}

#[test]
fn errors_when_assignee_after_content() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
Body line closes metadata window.
**Assignee:** alice
"#;

    let err = parse(input).unwrap_err();
    assert_eq!(
        err.message,
        "Metadata fields must appear immediately after the task heading before task content"
    );
}

#[test]
fn errors_when_assignee_outside_task() {
    let input = r#"# Rhei: Example

**Assignee:** alice

## Tasks

### Task 1: Alpha
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert_eq!(err.message, "Metadata field appears outside a task");
}

/// Exports are a plan-level handoff: the producer names what it publishes and
/// the consumer names what it reads, both in task metadata.
/// §FS-rhei-plan-language.3.12
#[test]
fn parses_provides_and_consumes_metadata() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Design the API
**State:** completed
**Provides:** api-contract, error-codes

### Task 2: Implement the client
**State:** pending
**Prior:** Task 1
**Consumes:** 1:api-contract, 1:error-codes
"#;

    let rhei = parse(input).expect("exports parse");

    assert_eq!(rhei.tasks[0].provides, vec!["api-contract", "error-codes"]);
    assert!(rhei.tasks[0].consumes.is_empty());

    let consumer = &rhei.tasks[1];
    assert!(consumer.provides.is_empty());
    assert_eq!(
        consumer.consumes.iter().map(|c| c.task.to_string()).collect::<Vec<_>>(),
        ["1", "1"]
    );
    assert_eq!(
        consumer.consumes.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["api-contract", "error-codes"]
    );
}

/// The read boundary is a closed metadata field after `**Consumes:**` and
/// accepts checkout, artifact, and graph-resolved export forms.
/// §FS-rhei-plan-language.2 §FS-rhei-plan-language.3.13
#[test]
fn parses_exclusions_after_consumes_and_before_assignee() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Publish
**State:** completed
**Provides:** statement

### Task 2: Review blind
**State:** pending
**Consumes:** 1:statement
**Excludes:** checkout=notes/private file.md, artifact=runtime/reviews/, 1:other
**Assignee:** manual
"#;

    parse(input).expect("all exclusion forms parse in metadata order");
}

#[test]
fn exclusions_before_consumes_is_a_parse_error() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Review
**State:** pending
**Excludes:** checkout=private.md
**Consumes:** 2:statement
"#;

    let err = parse(input).expect_err("Excludes must follow Consumes");
    assert!(
        err.message.contains("**Consumes:** must appear before **Excludes:**"),
        "error should state metadata order, got: {}",
        err.message
    );
}

#[test]
fn provides_before_state_is_parse_error() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Provides:** api-contract
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("**State:** must appear before **Provides:**"));
}

#[test]
fn consumes_before_state_is_parse_error() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**Consumes:** 2:api-contract
**State:** pending
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("**State:** must appear before **Consumes:**"));
}

/// A consumer that names no export has nothing to resolve, and the shape it is
/// most often confused with is `**Prior:**`.
#[test]
fn errors_on_consumes_reference_without_an_export_name() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Consumes:** 2
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("expected '<task-id>:<export-name>'"),
        "error should show the reference shape, got: {}",
        err.message
    );
}

/// An export name keys a path segment, so spaces and slashes cannot be part of
/// one.
#[test]
fn errors_on_provides_name_that_cannot_key_a_path() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Provides:** api contract
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("Malformed **Provides:** name"),
        "error should name the bad export, got: {}",
        err.message
    );
}

/// Two exports under one name leave a consumer no way to say which it meant.
#[test]
fn errors_on_duplicate_provided_export_name() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Provides:** api-contract, api-contract
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("provides 'api-contract' more than once"),
        "error should point at the duplicate, got: {}",
        err.message
    );
}

#[test]
fn errors_on_duplicate_consumed_export() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Consumes:** 2:api-contract, 2:api-contract
"#;

    let err = parse(input).unwrap_err();
    assert!(
        err.message.contains("consumes '2:api-contract' more than once"),
        "error should point at the duplicate, got: {}",
        err.message
    );
}

/// The metadata block is closed, so the new fields have to be listed in the
/// error that enumerates it. §FS-rhei-plan-language.2
#[test]
fn unknown_metadata_error_lists_the_export_fields() {
    let input = r#"# Rhei: Example
## Tasks

### Task 1: Alpha
**State:** pending
**Provide:** api-contract
"#;

    let err = parse(input).unwrap_err();
    assert!(err.message.contains("Unknown metadata field '**Provide:**'"), "{}", err.message);
    assert!(
        err.message.contains("**Provides:**")
            && err.message.contains("**Consumes:**")
            && err.message.contains("**Excludes:**"),
        "error should list exports and exclusions, got: {}",
        err.message
    );
}

/// Every separator needs an entry; surrounding whitespace remains legal.
/// §FS-rhei-plan-language.2
#[test]
fn exclusions_reject_empty_list_elements() {
    for entries in
        [",checkout=a", "checkout=a,,artifact=b", "checkout=a,", "checkout=a,   ,artifact=b", "   "]
    {
        let input = format!("# Rhei: Empty entries\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Excludes:** {entries}\n");
        let error = parse(&input).expect_err("empty exclusion entry must fail");
        assert!(error.message.contains("Empty **Excludes:** entry"), "{}", error.message);
    }
}

/// §FS-rhei-plan-language.2
#[test]
fn exclusions_accept_whitespace_around_real_entries() {
    let input = "# Rhei: Whitespace\n## Tasks\n\n### Task 1: Work\n**State:** pending\n**Excludes:**   checkout=a  ,   artifact=b/  \n";
    let plan = parse(input).expect("surrounding whitespace is allowed");
    assert_eq!(plan.tasks[0].excludes.len(), 2);
}
