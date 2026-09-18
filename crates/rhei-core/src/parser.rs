//! Recursive-descent parser for Markdown plans.
//!
//! Responsibilities:
//! - Extract the rhei title and pre-Tasks content sections.
//! - Parse YAML frontmatter, including the plan-level `structure` block
//!   (`maxLevels`, `nodeKinds`).
//! - Build the recursive task tree from H3..=H6 node headings, using the
//!   configured `nodeKinds` to accept the leading keyword.
//! - Attach child nodes to parents based on id-segment extension and heading
//!   depth.

mod builder;
mod plan;
mod recovery;
mod workspace;

pub use plan::parse;
pub use recovery::parse_collect;
pub use workspace::{
    parse_panta_manifest, parse_workspace_index, parse_workspace_tasks,
    parse_workspace_tasks_collect, parse_workspace_tasks_collect_with_structure,
    parse_workspace_tasks_with_structure, PantaManifest, WorkspaceIndex,
};

use crate::ast::{Metadata, Structure, DEFAULT_MAX_LEVELS, DEFAULT_NODE_KIND, MAX_ALLOWED_LEVELS};
use serde_yaml::Value as YamlValue;

/// Parser error with a message, an optional line number, and — once the error
/// has crossed a file boundary — the file the line belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParseError {
    pub message: String,
    pub line: Option<usize>,
    /// The file `line` refers to, set when an error from one document is
    /// re-raised while loading another (a rhei inside a project, a task file
    /// inside a workspace). `None` means the line belongs to the document the
    /// caller passed in. Diagnostics need this to render a code frame: a
    /// message alone strands the reader with no line and no source.
    pub file: Option<std::path::PathBuf>,
}

impl ParseError {
    pub fn new<M: Into<String>>(msg: M, line: Option<usize>) -> Self {
        Self { message: msg.into(), line, file: None }
    }

    /// Attach the source file this error's line belongs to.
    pub fn in_file<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.file = Some(path.into());
        self
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;

fn parse_structure(metadata: Option<&Metadata>, start_line: usize) -> Result<Structure> {
    let Some(metadata) = metadata else {
        return Ok(Structure::default());
    };
    let structure_key = YamlValue::String("structure".to_string());
    let Some(block) = metadata.get(&structure_key) else {
        return Ok(Structure::default());
    };
    let mapping = block.as_mapping().ok_or_else(|| {
        ParseError::new("plan frontmatter `structure` must be a mapping", Some(start_line))
    })?;

    let mut max_levels: u8 = DEFAULT_MAX_LEVELS;
    if let Some(v) = mapping.get(YamlValue::String("maxLevels".to_string())) {
        let n = v.as_u64().ok_or_else(|| {
            ParseError::new(
                "plan frontmatter `structure.maxLevels` must be a positive integer",
                Some(start_line),
            )
        })?;
        if n == 0 || n > MAX_ALLOWED_LEVELS as u64 {
            return Err(ParseError::new(
                format!(
                    "plan frontmatter `structure.maxLevels` must be in 1..={}",
                    MAX_ALLOWED_LEVELS
                ),
                Some(start_line),
            ));
        }
        max_levels = n as u8;
    }

    let mut node_kinds: Vec<String> = vec![DEFAULT_NODE_KIND.to_string()];
    if let Some(v) = mapping.get(YamlValue::String("nodeKinds".to_string())) {
        let seq = v.as_sequence().ok_or_else(|| {
            ParseError::new(
                "plan frontmatter `structure.nodeKinds` must be a sequence of strings",
                Some(start_line),
            )
        })?;
        if seq.is_empty() {
            return Err(ParseError::new(
                "plan frontmatter `structure.nodeKinds` must not be empty",
                Some(start_line),
            ));
        }
        let mut out = Vec::with_capacity(seq.len());
        for entry in seq {
            let name = entry.as_str().ok_or_else(|| {
                ParseError::new(
                    "plan frontmatter `structure.nodeKinds` entries must be strings",
                    Some(start_line),
                )
            })?;
            let canonical = name.trim().to_ascii_lowercase();
            if canonical.is_empty() {
                return Err(ParseError::new(
                    "plan frontmatter `structure.nodeKinds` entries must be non-empty",
                    Some(start_line),
                ));
            }
            if canonical == "panta" || canonical == "rhei" {
                return Err(ParseError::new(
                    format!(
                        "`{canonical}` is a reserved node kind and must not appear in `structure.nodeKinds`"
                    ),
                    Some(start_line),
                ));
            }
            if out.iter().any(|k: &String| k == &canonical) {
                return Err(ParseError::new(
                    format!(
                        "plan frontmatter `structure.nodeKinds` contains duplicate entry `{canonical}`"
                    ),
                    Some(start_line),
                ));
            }
            out.push(canonical);
        }
        node_kinds = out;
    }

    Ok(Structure { max_levels, node_kinds })
}

fn parse_frontmatter(lines: &[String], start_line: usize, kind: &str) -> Result<Metadata> {
    if lines.is_empty() || lines.iter().all(|line| line.trim().is_empty()) {
        return Ok(Metadata::new());
    }

    let yaml = lines.join("\n");
    let value: YamlValue = serde_yaml::from_str(&yaml).map_err(|err| {
        ParseError::new(format!("failed to parse {kind} YAML frontmatter: {err}"), Some(start_line))
    })?;

    match value {
        YamlValue::Mapping(mapping) => Ok(mapping),
        _ => Err(ParseError::new(
            format!("{kind} YAML frontmatter must be a mapping"),
            Some(start_line),
        )),
    }
}

fn unescape_state(input: &str) -> String {
    if input.starts_with('`') && input.ends_with('`') && input.len() >= 2 {
        return input[1..input.len() - 1].to_string();
    }
    input.to_string()
}

#[cfg(test)]
mod plan_snapshot_prior_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod workspace_tests;
