use crate::ast::{ContentSection, Metadata, Structure, Task};
use crate::fence::FenceTracker;
use regex::Regex;

use super::{parse, parse_collect, parse_frontmatter, parse_structure, ParseError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIndex {
    pub title: String,
    pub states: String,
    pub states_declared: bool,
    pub structure: Structure,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PantaManifest {
    pub title: String,
    pub states: String,
    pub states_declared: bool,
    pub structure: Structure,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

/// Parse a workspace index file (`index.rhei.md`).
pub fn parse_workspace_index(input: &str) -> Result<WorkspaceIndex> {
    let parsed = parse_manifest(input, "Rhei", "workspace index")?;
    Ok(WorkspaceIndex {
        title: parsed.title,
        states: parsed.states,
        states_declared: parsed.states_declared,
        structure: parsed.structure,
        metadata: parsed.metadata,
        content_sections: parsed.content_sections,
    })
}

/// Parse a Panta project manifest file (`index.panta.md`). §FS-rhei-plan-language.1.5
pub fn parse_panta_manifest(input: &str) -> Result<PantaManifest> {
    let parsed = parse_manifest(input, "Panta", "Panta manifest")?;
    Ok(PantaManifest {
        title: parsed.title,
        states: parsed.states,
        states_declared: parsed.states_declared,
        structure: parsed.structure,
        metadata: parsed.metadata,
        content_sections: parsed.content_sections,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestParts {
    title: String,
    states: String,
    states_declared: bool,
    structure: Structure,
    metadata: Option<Metadata>,
    content_sections: Vec<ContentSection>,
}

fn parse_manifest(input: &str, header_name: &str, frontmatter_kind: &str) -> Result<ManifestParts> {
    let re_header =
        Regex::new(&format!(r#"^#\s+{}:\s+(.*)$"#, regex::escape(header_name))).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

    let mut title: Option<String> = None;
    let mut states: Option<String> = None;
    let mut states_checked = false;
    let mut metadata: Option<Metadata> = None;
    let mut structure: Structure = Structure::default();
    let mut frontmatter_checked = false;
    let mut in_frontmatter = false;
    let mut frontmatter_start_line = 0usize;
    let mut frontmatter_lines: Vec<String> = Vec::new();
    let mut header_seen = false;
    let mut content: Vec<ContentSection> = Vec::new();
    let mut fence = FenceTracker::default();

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if in_frontmatter {
            if line == "---" {
                let parsed = parse_frontmatter(
                    &frontmatter_lines,
                    frontmatter_start_line,
                    "workspace index",
                )?;
                structure = parse_structure(Some(&parsed), frontmatter_start_line)?;
                metadata = Some(parsed);
                in_frontmatter = false;
                continue;
            }
            frontmatter_lines.push(raw.to_string());
            continue;
        }

        // The index reads a fence by the language's one rule, so an index may
        // quote the plan format it documents. §FS-rhei-plan-language.2.1
        if fence.read(raw) || fence.is_open() {
            if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
                c.push_str(raw);
            }
            continue;
        }

        if line.is_empty() {
            // A content section is pasted verbatim (§FS-rhei-memory.4.2), and
            // a paragraph break is part of the text: dropping it welds
            // paragraphs and list items together.
            if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
                if !c.is_empty() {
                    c.push('\n');
                }
            }
            continue;
        }

        if !header_seen && line == "---" {
            return Err(ParseError::new(
                format!(
                    "YAML frontmatter must appear after the `# {header_name}:` header (and any `**States:**` declaration). Move the `---` block below the header."
                ),
                Some(line_number),
            ));
        }

        if !header_seen {
            if let Some(cap) = re_header.captures(line) {
                title = Some(cap.get(1).unwrap().as_str().to_string());
                header_seen = true;
                continue;
            }
            let is_h1 = line.starts_with('#') && !line.starts_with("##");
            if is_h1 {
                return Err(ParseError::new(
                    format!("Malformed heading: expected '# {header_name}: <title>'"),
                    Some(line_number),
                ));
            }
            continue;
        }

        if !states_checked {
            if let Some(cap) = re_states_decl.captures(line) {
                states = Some(cap.get(1).unwrap().as_str().trim().to_string());
                states_checked = true;
                continue;
            }
            states_checked = true;
        }

        if states_checked && !frontmatter_checked {
            if line == "---" {
                in_frontmatter = true;
                frontmatter_checked = true;
                frontmatter_start_line = line_number + 1;
                frontmatter_lines.clear();
                continue;
            }
            frontmatter_checked = true;
        }

        if re_tasks.is_match(line) {
            return Err(ParseError::new(
                format!(
                    "{frontmatter_kind} file must not contain a '## Tasks' section; tasks belong in child task files"
                ),
                Some(line_number),
            ));
        }

        if let Some(cap) = re_section_header.captures(line) {
            let section_title = cap.get(1).unwrap().as_str().trim().to_string();
            content.push(ContentSection {
                title: section_title,
                content: String::new(),
                rhei: None,
            });
            continue;
        }

        if let Some(ContentSection { content: ref mut c, .. }) = content.last_mut() {
            if !c.is_empty() {
                c.push('\n');
            }
            c.push_str(raw);
        }
    }

    if in_frontmatter {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(frontmatter_start_line.saturating_sub(1).max(1)),
        ));
    }

    let title = title.ok_or_else(|| {
        ParseError::new(format!("Missing '# {header_name}: <title>' header"), None)
    })?;

    let states_declared = states.is_some();
    // Interior blanks are the author's; the ones trailing a section only
    // separate it from the next heading. §FS-rhei-memory.4.2
    for section in &mut content {
        let trimmed = section.content.trim_end().to_string();
        section.content = trimmed;
    }
    Ok(ManifestParts {
        title,
        states: states.unwrap_or_else(|| "rhei".to_string()),
        states_declared,
        structure,
        metadata,
        content_sections: content,
    })
}

/// Parse a workspace task file (a file inside the `tasks/` directory).
pub fn parse_workspace_tasks(input: &str) -> Result<Vec<Task>> {
    parse_workspace_tasks_with_optional_structure(input, None)
}

/// Parse a workspace task file using the structure declared by its workspace index. §FS-rhei-authoring.3.3
pub fn parse_workspace_tasks_with_structure(
    input: &str,
    structure: &Structure,
) -> Result<Vec<Task>> {
    parse_workspace_tasks_with_optional_structure(input, Some(structure))
}

fn parse_workspace_tasks_with_optional_structure(
    input: &str,
    structure: Option<&Structure>,
) -> Result<Vec<Task>> {
    let prefix = workspace_task_synthetic_prefix(structure);
    let prefix_line_count = prefix.matches('\n').count();
    let synthetic = format!("{}{}", prefix, input);
    match parse(&synthetic) {
        Ok(rhei) => Ok(rhei.tasks),
        Err(mut e) => {
            adjust_workspace_task_error_line(&mut e, prefix_line_count);
            Err(e)
        }
    }
}

/// Parse a workspace task file and collect recoverable task-local parse errors.
///
/// This mirrors [`parse_workspace_tasks`] but preserves `parse_collect`'s
/// multi-error behavior for validation diagnostics. Reported line numbers are
/// adjusted back from the synthetic single-file wrapper to the task file.
pub fn parse_workspace_tasks_collect(input: &str) -> (Option<Vec<Task>>, Vec<ParseError>) {
    parse_workspace_tasks_collect_with_optional_structure(input, None)
}

/// Parse a workspace task file using the workspace index structure, collecting
/// recoverable task-local parse errors with task-file line numbers.
/// §FS-rhei-authoring.3.3
pub fn parse_workspace_tasks_collect_with_structure(
    input: &str,
    structure: &Structure,
) -> (Option<Vec<Task>>, Vec<ParseError>) {
    parse_workspace_tasks_collect_with_optional_structure(input, Some(structure))
}

fn parse_workspace_tasks_collect_with_optional_structure(
    input: &str,
    structure: Option<&Structure>,
) -> (Option<Vec<Task>>, Vec<ParseError>) {
    let prefix = workspace_task_synthetic_prefix(structure);
    let prefix_line_count = prefix.matches('\n').count();
    let synthetic = format!("{}{}", prefix, input);
    let (maybe_rhei, mut errors) = parse_collect(&synthetic);
    for error in &mut errors {
        adjust_workspace_task_error_line(error, prefix_line_count);
    }
    (maybe_rhei.map(|rhei| rhei.tasks), errors)
}

fn workspace_task_synthetic_prefix(structure: Option<&Structure>) -> String {
    let Some(structure) = structure else {
        return "# Rhei: _workspace_\n\n## Tasks\n\n".to_string();
    };

    let mut prefix = format!(
        "# Rhei: _workspace_\n\n---\nstructure:\n  maxLevels: {}\n  nodeKinds:\n",
        structure.max_levels
    );
    for kind in &structure.node_kinds {
        prefix.push_str("    - ");
        prefix.push_str(kind);
        prefix.push('\n');
    }
    prefix.push_str("---\n\n## Tasks\n\n");
    prefix
}

fn adjust_workspace_task_error_line(error: &mut ParseError, prefix_line_count: usize) {
    if let Some(ref mut line) = error.line {
        *line = line.saturating_sub(prefix_line_count);
        if *line == 0 {
            *line = 1;
        }
    }
}
