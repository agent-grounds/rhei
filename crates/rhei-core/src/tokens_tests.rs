//! [`TASK_METADATA_FIELDS`] against the parser that reads it, in both
//! directions: every field the lexer or the tooling reader recognizes is in the
//! set, and every entry of the set reads back as that same field. Checking only
//! that each entry lexes as *some* metadata would pass a set that leaves a field
//! out, which is how `**Excludes:**` fell out of `rhei new`'s description guard.
//! §FS-rhei-plan-language.2 §FS-rhei-new.3.4.2

use super::*;
use crate::{parse, tokenize};

/// One line of each field, with a value the parser accepts: a line for every
/// `Metadata*` token, then the two tooling fields, which are not tokens.
const FIELD_LINES: [&str; 11] = [
    "**State:** pending",
    "**Prior:** Task 2",
    "**Inherits:** reviewed from prior",
    "**Provides:** api-contract",
    "**Consumes:** 2:api-contract",
    "**Excludes:** checkout=secret.md",
    "**Assignee:** agent-1",
    "**Model:** deep",
    "**Target:** codex:openai:gpt-5",
    "**MCP servers:** thunderbird-mail",
    "**Skills:** release-notes",
];

/// The marker a metadata token is read from, and `None` for every other token.
///
/// There is no wildcard arm. A variant added to [`Token`] does not compile until
/// it is placed here, and one placed as metadata also needs its line in
/// [`FIELD_LINES`], which is what the checks below lex.
fn field_of(token: &Token) -> Option<&'static str> {
    match token {
        Token::MetadataState { .. } => Some("**State:**"),
        Token::MetadataPrior { .. } => Some("**Prior:**"),
        Token::MetadataInherits { .. } => Some("**Inherits:**"),
        Token::MetadataProvides { .. } => Some("**Provides:**"),
        Token::MetadataConsumes { .. } => Some("**Consumes:**"),
        Token::MetadataExcludes { .. } => Some("**Excludes:**"),
        Token::MetadataAssignee { .. } => Some("**Assignee:**"),
        Token::MetadataModel { .. } => Some("**Model:**"),
        Token::MetadataTarget { .. } => Some("**Target:**"),
        Token::RheiHeader
        | Token::TasksSection
        | Token::SectionHeader { .. }
        | Token::NodeHeader { .. }
        | Token::TextContent => None,
    }
}

/// The one token `line` lexes to.
fn token_of(line: &str) -> Token {
    match tokenize(line).collect::<Vec<_>>().as_slice() {
        [token] => token.clone(),
        other => panic!("{line:?} must lex as one token, got {other:?}"),
    }
}

/// Parser to set: every field the lexer tokenizes, and both fields the tooling
/// reader takes, is in the set — so the description guard that reads the set
/// refuses a line opening with it.
#[test]
fn every_field_the_parser_reads_is_in_the_set() {
    for line in FIELD_LINES {
        let Some(field) = field_of(&token_of(line)) else {
            continue;
        };
        assert!(line.starts_with(field), "{line:?} lexed as {field}");
        assert!(
            TASK_METADATA_FIELDS.contains(&field),
            "the lexer reads {field} as task metadata, but TASK_METADATA_FIELDS does not hold it"
        );
    }
    for field in [MCP_SERVERS_FIELD, SKILLS_FIELD] {
        assert!(
            TASK_METADATA_FIELDS.contains(&field),
            "the parser reads {field} as task metadata, but TASK_METADATA_FIELDS does not hold it"
        );
    }
}

/// Set to parser: every entry, given a value, reads back as that same field — a
/// token field as its own token, a tooling field into its own slot on the task.
#[test]
fn every_field_in_the_set_reads_back_as_itself() {
    for field in TASK_METADATA_FIELDS {
        let line = FIELD_LINES
            .into_iter()
            .find(|line| line.starts_with(field))
            .unwrap_or_else(|| panic!("{field} has no line in FIELD_LINES"));
        if field != MCP_SERVERS_FIELD && field != SKILLS_FIELD {
            assert_eq!(field_of(&token_of(line)), Some(field), "{line:?} must lex as {field}");
            continue;
        }
        let input =
            format!("# Rhei: Fields\n## Tasks\n\n### Task 1: One\n**State:** pending\n{line}\n");
        let plan = parse(&input).expect("a tooling field after the state parses");
        let tooling = &plan.tasks[0].tooling;
        let (own, other) = if field == MCP_SERVERS_FIELD {
            (&tooling.mcp_servers, &tooling.skills)
        } else {
            (&tooling.skills, &tooling.mcp_servers)
        };
        assert!(!own.is_empty() && other.is_empty(), "{line:?} must be read as {field}");
    }
}
