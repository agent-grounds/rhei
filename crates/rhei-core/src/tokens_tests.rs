//! [`TASK_METADATA_FIELDS`] against what reads a line as task metadata, in both
//! directions: every field the lexer or the tooling reader recognizes is in the
//! set, and every entry of the set reads back as that same field. Checking only
//! that each entry lexes as *some* metadata would pass a set that leaves a field
//! out, which is how `**Excludes:**` fell out of `rhei new`'s description guard.
//!
//! The lexer stands in for the parser: `parse` matches its own field branches
//! rather than calling `tokenize`, so a field read only there is not seen here.
//! The tooling fields are read through [`TOOLING_FIELDS`], which the set is
//! built from. §FS-rhei-plan-language.2 §FS-rhei-new.3.4.2

use super::*;
use crate::task_tooling::{MCP_SERVERS_FIELD, SKILLS_FIELD, TOOLING_FIELDS};
use crate::{parse, tokenize};

/// One line of each field, with a value the parser accepts: a line for every
/// `Metadata*` token, then the tooling fields, which are not tokens.
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

/// Whether `a` and `b` are the same bytes, which `==` cannot say in a constant.
const fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// Whether [`TASK_METADATA_FIELDS`] holds `marker`, decided at compile time.
const fn in_set(marker: &str) -> bool {
    let mut at = 0;
    while at < TASK_METADATA_FIELDS.len() {
        if same(TASK_METADATA_FIELDS[at].as_bytes(), marker.as_bytes()) {
            return true;
        }
        at += 1;
    }
    false
}

/// `Some(marker)`, as a constant that does not compile while the set lacks it.
macro_rules! field {
    ($marker:literal) => {
        Some(
            const {
                assert!(
                    in_set($marker),
                    concat!(
                        $marker,
                        " is lexed as task metadata, but TASK_METADATA_FIELDS does not hold it"
                    )
                );
                $marker
            },
        )
    };
}

/// The marker a metadata token is read from, and `None` for every other token.
///
/// There is no wildcard arm, so a variant added to [`Token`] does not compile
/// until it is placed here. Each metadata arm names its marker through `field!`,
/// which fails the test build, naming the marker, while [`TASK_METADATA_FIELDS`]
/// does not hold it: whether or not [`FIELD_LINES`] has a line for it, and
/// under `cargo build --all-targets` as well as `cargo test`.
fn field_of(token: &Token) -> Option<&'static str> {
    match token {
        Token::MetadataState { .. } => field!("**State:**"),
        Token::MetadataPrior { .. } => field!("**Prior:**"),
        Token::MetadataInherits { .. } => field!("**Inherits:**"),
        Token::MetadataProvides { .. } => field!("**Provides:**"),
        Token::MetadataConsumes { .. } => field!("**Consumes:**"),
        Token::MetadataExcludes { .. } => field!("**Excludes:**"),
        Token::MetadataAssignee { .. } => field!("**Assignee:**"),
        Token::MetadataModel { .. } => field!("**Model:**"),
        Token::MetadataTarget { .. } => field!("**Target:**"),
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

/// Reader to set: each line the lexer reads as metadata opens with the marker
/// [`field_of`] names for its token, which the set holds or this would not
/// compile, and each field the tooling reader matches is in the set — so the
/// description guard that reads the set refuses a line opening with it.
#[test]
fn every_field_read_as_metadata_is_in_the_set() {
    for line in FIELD_LINES {
        if let Some(field) = field_of(&token_of(line)) {
            assert!(line.starts_with(field), "{line:?} lexed as {field}");
        }
    }
    for field in TOOLING_FIELDS {
        assert!(
            TASK_METADATA_FIELDS.contains(&field),
            "the tooling reader matches {field}, but TASK_METADATA_FIELDS does not hold it"
        );
    }
}

/// Set to reader: every entry, given a value, reads back as that same field — a
/// token field as its own token, a tooling field into its own slot on the task.
#[test]
fn every_field_in_the_set_reads_back_as_itself() {
    for field in TASK_METADATA_FIELDS {
        let line = FIELD_LINES
            .into_iter()
            .find(|line| line.starts_with(field))
            .unwrap_or_else(|| panic!("{field} has no line in FIELD_LINES"));
        if !TOOLING_FIELDS.contains(&field) {
            assert_eq!(field_of(&token_of(line)), Some(field), "{line:?} must lex as {field}");
            continue;
        }
        // The slot is named before parsing, so a tooling field this test does
        // not know fails on that, not on whatever the reader made of its line.
        let is_mcp = match field {
            MCP_SERVERS_FIELD => true,
            SKILLS_FIELD => false,
            _ => panic!(
                "{field} is in TOOLING_FIELDS, but this test maps it to no slot of \
                 TaskTooling; name the slot it fills here"
            ),
        };
        let input =
            format!("# Rhei: Fields\n## Tasks\n\n### Task 1: One\n**State:** pending\n{line}\n");
        let plan = parse(&input).expect("a tooling field after the state parses");
        let tooling = &plan.tasks[0].tooling;
        let (own, other) = if is_mcp {
            (&tooling.mcp_servers, &tooling.skills)
        } else {
            (&tooling.skills, &tooling.mcp_servers)
        };
        assert!(!own.is_empty() && other.is_empty(), "{line:?} must be read as {field}");
    }
}
