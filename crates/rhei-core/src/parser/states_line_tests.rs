//! The retired `**States:**` line across the three documents that could carry
//! it — a single-file plan, a workspace index, a project manifest — and the
//! places it was never anything but text. §FS-rhei-plan-language.2.2

use super::*;

/// The retired line is a parse error carrying its line and the one remedy,
/// wherever it sits outside a fence. §FS-rhei-plan-language.2.2
#[test]
fn refuses_the_retired_states_line_with_its_line_number() {
    for (input, line) in [
        ("# Rhei: Example\n**States:** custom\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n", 2),
        ("# Rhei: Example\n\n## Notes\nText\n**States:** custom\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n", 5),
    ] {
        let err = parse(input).unwrap_err();
        assert_eq!(err.line, Some(line), "for:\n{input}");
        assert!(err.message.contains("delete this line"), "unexpected message: {}", err.message);
    }
}

/// In a task body the line is text, as it always was. §FS-rhei-plan-language.2.2
#[test]
fn a_states_line_in_a_task_body_is_text() {
    let rhei = parse("# Rhei: Example\n## Tasks\n\n### Task 1: Alpha\n**State:** pending\n\nQuote:\n**States:** custom\n")
        .expect("a body line parses");
    assert!(rhei.tasks[0].content.contains("**States:** custom"));
}

/// The workspace index and the project manifest refuse the retired line like a
/// single-file plan does, naming its line. §FS-rhei-plan-language.2.2
#[test]
fn index_and_manifest_refuse_the_retired_states_line() {
    let index = parse_workspace_index("# Rhei: Workspace\n**States:** custom\n").unwrap_err();
    assert_eq!(index.line, Some(2));
    assert!(index.message.contains("delete this line"), "unexpected message: {}", index.message);

    let manifest =
        parse_panta_manifest("# Panta: Project\n\n## Context\nText\n**States:** custom\n")
            .unwrap_err();
    assert_eq!(manifest.line, Some(5));
    assert!(
        manifest.message.contains("delete this line"),
        "unexpected message: {}",
        manifest.message
    );
}

/// A fenced quotation of the old syntax is content. §FS-rhei-plan-language.2.1
#[test]
fn a_fenced_states_line_in_an_index_is_content() {
    parse_workspace_index(
        "# Rhei: Workspace\n\n## History\n```markdown\n**States:** custom\n```\n",
    )
    .expect("a fenced line is content");
    parse_panta_manifest("# Panta: Project\n\n## History\n~~~\n**States:** custom\n~~~\n")
        .expect("a fenced line is content");
}
