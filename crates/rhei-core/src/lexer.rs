//! Line-oriented tokenizer producing a stream of tokens from markdown input.
//!
//! The tokenizer emits `NodeHeader` tokens for every H3..=H6 heading that
//! matches the `<kind> <id>: <title>` shape, plus section, state, and prior
//! metadata tokens.

use crate::ast::{ConsumedExport, TaskId};
use crate::text::parse_task_id;
use crate::tokens::Token;
use regex::Regex;

/// Streaming tokenizer over markdown input.
pub struct Tokenizer<'a> {
    lines: std::str::Lines<'a>,

    re_rhei: Regex,
    re_tasks: Regex,
    re_node_header: Regex,
    re_prior_ref: Regex,
    re_consumes_ref: Regex,
    re_states: Regex,
    re_state: Regex,
    re_assignee: Regex,
    re_model: Regex,
    re_target: Regex,

    in_code_block: bool,
}

impl<'a> Tokenizer<'a> {
    /// Construct a new tokenizer over the provided input.
    pub fn new(input: &'a str) -> Self {
        let re_rhei = Regex::new(r#"^#\s+Rhei:\s+.*$"#).unwrap();
        let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();

        // Node headers at H3..=H6: `### Task 1: Title`, `#### Bug 1.1: Title`.
        // Captures: 1=hashes, 2=kind, 3=id (dotted path of NUMBER|IDENTIFIER
        // segments), 4=title. Each id segment is either all digits or starts
        // with a letter, matching the grammar.
        let task_id_segment = r#"(?:[A-Za-z][A-Za-z0-9_-]*|0|[1-9][0-9]*)"#;
        let task_id_pattern = format!(r#"{task_id_segment}(?:\.{task_id_segment})*"#);
        let re_node_header = Regex::new(&format!(
            r#"^(#{{3,6}})\s+([A-Za-z][A-Za-z0-9_-]*)\s+({task_id_pattern}):\s+(.*)$"#
        ))
        .unwrap();

        // For "**Prior:** Task 1, Bug 1.2, Task api.cache" — captures kind + id pairs.
        let re_prior_ref =
            Regex::new(&format!(r#"^([A-Za-z][A-Za-z0-9_-]*)\s+({task_id_pattern})$"#)).unwrap();

        // For "**Consumes:** auth.1:api-contract" — captures task id + export
        // name. §FS-rhei-plan-language.3.12
        let re_consumes_ref =
            Regex::new(&format!(r#"^({task_id_pattern}):([A-Za-z0-9][A-Za-z0-9._-]*)$"#)).unwrap();

        // For "**States:** name" (must be checked before re_state)
        let re_states = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();

        // For "**State:** value"
        let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();

        // For "**Assignee:** value"
        let re_assignee = Regex::new(r#"^\*\*Assignee:\*\*\s*(.+)$"#).unwrap();

        // For "**Model:** value"
        let re_model = Regex::new(r#"^\*\*Model:\*\*\s*(.+)$"#).unwrap();

        // For "**Target:** value"
        let re_target = Regex::new(r#"^\*\*Target:\*\*\s*(.+)$"#).unwrap();

        Self {
            lines: input.lines(),
            re_rhei,
            re_tasks,
            re_node_header,
            re_prior_ref,
            re_consumes_ref,
            re_states,
            re_state,
            re_assignee,
            re_model,
            re_target,
            in_code_block: false,
        }
    }

    /// Unescape a state value: supports backtick wrapping only.
    fn unescape_state(input: &str) -> String {
        if input.starts_with('`') && input.ends_with('`') && input.len() >= 2 {
            return input[1..input.len() - 1].to_string();
        }
        input.to_string()
    }

    fn is_fence(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("```")
    }
}

/// Convenience constructor to obtain a streaming tokenizer.
pub fn tokenize(input: &str) -> Tokenizer<'_> {
    Tokenizer::new(input)
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        for raw in self.lines.by_ref() {
            let line = raw.trim();

            if Self::is_fence(raw) {
                self.in_code_block = !self.in_code_block;
                return Some(Token::TextContent);
            }

            if line.is_empty() {
                continue;
            }

            if self.in_code_block {
                return Some(Token::TextContent);
            }

            if self.re_rhei.is_match(line) {
                return Some(Token::RheiHeader);
            }

            if self.re_tasks.is_match(line) {
                return Some(Token::TasksSection);
            }

            // Section header: ## <title> (non-Tasks)
            if line.starts_with("## ") && !self.re_tasks.is_match(line) {
                let title = line.strip_prefix("## ").unwrap_or("").trim().to_string();
                if !title.is_empty() {
                    return Some(Token::SectionHeader { title });
                }
            }

            if let Some(caps) = self.re_node_header.captures(line) {
                let hashes = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let level = hashes.len() as u8;
                let kind = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                let id_str = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                if let Some(id) = parse_task_id(id_str) {
                    return Some(Token::NodeHeader { level, kind, id });
                }
                // Malformed id — fall through to text content.
                return Some(Token::TextContent);
            }

            // Metadata: States declaration (must be checked before State)
            if let Some(caps) = self.re_states.captures(line) {
                let name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                return Some(Token::MetadataStates { name: name.to_string() });
            }

            // Metadata: State (with unescaping or backtick stripping)
            if let Some(caps) = self.re_state.captures(line) {
                let state_raw = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                let state = Self::unescape_state(state_raw);
                return Some(Token::MetadataState { state });
            }

            // Metadata: Prior
            if line.starts_with("**Prior:**") {
                let ids = line
                    .strip_prefix("**Prior:**")
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|item| self.re_prior_ref.captures(item.trim()))
                    .filter_map(|c| c.get(2).and_then(|m| parse_task_id(m.as_str())))
                    .collect::<Vec<TaskId>>();
                return Some(Token::MetadataPrior { task_ids: ids });
            }

            // Metadata: Provides §FS-rhei-plan-language.3.12
            if line.starts_with("**Provides:**") {
                let names = line
                    .strip_prefix("**Provides:**")
                    .unwrap_or_default()
                    .split(',')
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                    .map(|item| item.to_string())
                    .collect::<Vec<String>>();
                return Some(Token::MetadataProvides { names });
            }

            // Metadata: Consumes §FS-rhei-plan-language.3.12
            if line.starts_with("**Consumes:**") {
                let exports = line
                    .strip_prefix("**Consumes:**")
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|item| self.re_consumes_ref.captures(item.trim()))
                    .filter_map(|c| {
                        let task = c.get(1).and_then(|m| parse_task_id(m.as_str()))?;
                        let name = c.get(2)?.as_str().to_string();
                        Some(ConsumedExport { task, name })
                    })
                    .collect::<Vec<ConsumedExport>>();
                return Some(Token::MetadataConsumes { exports });
            }

            // Metadata: Excludes. Detailed validation belongs to the parser.
            // §FS-rhei-plan-language.4
            if line.starts_with("**Excludes:**") {
                let entries = line
                    .strip_prefix("**Excludes:**")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty())
                    .map(str::to_string)
                    .collect();
                return Some(Token::MetadataExcludes { entries });
            }

            // Metadata: Assignee
            if let Some(caps) = self.re_assignee.captures(line) {
                let name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                return Some(Token::MetadataAssignee { name: name.to_string() });
            }

            // Metadata: Model
            if let Some(caps) = self.re_model.captures(line) {
                let model = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                return Some(Token::MetadataModel { model: model.to_string() });
            }

            // Metadata: Target
            if let Some(caps) = self.re_target.captures(line) {
                let target = caps.get(1).map(|m| m.as_str().trim()).unwrap_or_default();
                return Some(Token::MetadataTarget { target: target.to_string() });
            }

            return Some(Token::TextContent);
        }

        None
    }
}
