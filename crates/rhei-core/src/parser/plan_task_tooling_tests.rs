//! The task tooling fields `**MCP servers:**` and `**Skills:**`: accepted last
//! in the metadata block, and every malformed spelling refused with an error
//! that names the field. What the parsed entries hold is pinned beside the
//! task AST they land in.
//! §FS-rhei-plan-language.2 §FS-rhei-task-tooling.1

use super::*;

fn task_with(metadata: &str) -> String {
    format!("# Rhei: Tooling\n## Tasks\n\n### Task 1: Mail\n**State:** pending\n{metadata}Body\n")
}

fn refusal(metadata: &str) -> String {
    let input = task_with(metadata);
    match parse(&input) {
        Ok(_) => panic!("the plan must be refused:\n{input}"),
        Err(err) => err.message,
    }
}

/// Both fields are the last metadata, after the execution override, and an
/// entry may carry ` (optional)`.
#[test]
fn task_tooling_fields_are_accepted_last_in_the_block() {
    for metadata in [
        "**MCP servers:** thunderbird-mail\n",
        "**Skills:** release-notes\n",
        "**MCP servers:** thunderbird-mail, grafana (optional)\n**Skills:** release-notes (optional)\n",
        "**Assignee:** manual\n**Model:** deep\n**MCP servers:** thunderbird-mail\n**Skills:** release-notes\n",
        "**Prior:** Task 2\n**Target:** codex:openai:gpt-5\n**MCP servers:** grafana (optional)\n",
    ] {
        let input = task_with(metadata);
        let plan = parse(&input).unwrap_or_else(|err| panic!("{}\n{input}", err.message));
        assert_eq!(plan.tasks[0].content.trim(), "Body", "metadata leaked into content:\n{input}");
    }
}

/// The closed block lists every field it accepts, the two new ones included.
#[test]
fn unknown_metadata_error_lists_the_task_tooling_fields() {
    let message = refusal("**MCP server:** thunderbird-mail\n");
    assert!(message.contains("Unknown metadata field '**MCP server:**'"), "{message}");
    assert!(
        message.contains("**MCP servers:**") && message.contains("**Skills:**"),
        "the unknown-field error must list the task tooling fields: {message}"
    );
}

#[test]
fn a_task_tooling_field_twice_is_refused() {
    let message = refusal("**MCP servers:** thunderbird-mail\n**MCP servers:** grafana\n");
    assert!(message.contains("Duplicate **MCP servers:**"), "{message}");
    let message = refusal("**Skills:** release-notes\n**Skills:** release-notes\n");
    assert!(message.contains("Duplicate **Skills:**"), "{message}");
}

#[test]
fn an_empty_task_tooling_value_or_entry_is_refused() {
    for value in ["", "   ", ",grafana", "thunderbird-mail,, grafana", "grafana,"] {
        let message = refusal(&format!("**MCP servers:** {value}\n"));
        assert!(message.contains("Empty **MCP servers:**"), "{value:?}: {message}");
    }
    let message = refusal("**Skills:** \n");
    assert!(message.contains("Empty **Skills:**"), "{message}");
}

/// The same id twice in one field is refused, whatever its optional marker.
#[test]
fn a_duplicate_task_tooling_id_is_refused() {
    let message = refusal("**MCP servers:** grafana, thunderbird-mail, grafana (optional)\n");
    assert!(
        message.contains("**MCP servers:**")
            && message.contains("'grafana'")
            && message.to_lowercase().contains("duplicate"),
        "{message}"
    );
}

/// Only an id, optionally followed by exactly ` (optional)`, is an entry.
#[test]
fn a_malformed_task_tooling_entry_is_refused_naming_it() {
    for entry in ["grafana (optinal)", "grafana(optional)", "thunderbird mail", "(optional)"] {
        let message = refusal(&format!("**MCP servers:** {entry}\n"));
        assert!(
            message.contains("**MCP servers:**") && message.contains(entry),
            "{entry:?}: {message}"
        );
    }
    let message = refusal("**Skills:** release-notes (required)\n");
    assert!(
        message.contains("**Skills:**") && message.contains("release-notes (required)"),
        "{message}"
    );
}

/// Grammar order: `**State:**` first, the execution override before the
/// tooling, and `**MCP servers:**` before `**Skills:**`.
#[test]
fn a_task_tooling_field_out_of_order_is_refused() {
    let before_state =
        "# Rhei: Tooling\n## Tasks\n\n### Task 1: Mail\n**MCP servers:** grafana\n**State:** pending\n";
    let message = parse(before_state).expect_err("tooling before state").message;
    assert!(message.contains("**State:** must appear before **MCP servers:**"), "{message}");

    for (metadata, field) in [
        ("**Skills:** release-notes\n**MCP servers:** grafana\n", "**MCP servers:**"),
        ("**MCP servers:** grafana\n**Model:** deep\n", "**Model:**"),
        ("**MCP servers:** grafana\n**Target:** codex:openai:gpt-5\n", "**Target:**"),
        ("**Skills:** release-notes\n**Assignee:** manual\n", "**Assignee:**"),
        ("**MCP servers:** grafana\n**Prior:** Task 2\n", "**Prior:**"),
    ] {
        let message = refusal(metadata);
        assert!(
            message.contains("must appear") && message.contains(field),
            "{metadata:?}: {message}"
        );
    }
}
