use crate::ast::{ContentSection, Metadata, Structure, Task};
use crate::fence::FenceTracker;
use crate::header::{header_regex, PANTA_HEADER, RHEI_HEADER};
use crate::tokens::TASK_METADATA_FIELDS;
use regex::Regex;

use super::task_file_metadata::{
    basin_frontmatter_error, check_task_metadata_block, normalize_task_metadata_ids,
    split_leading_frontmatter,
};
use super::{
    is_retired_states_line, parse, parse_collect, parse_frontmatter, parse_structure,
    retired_states_line, ParseError, Result,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIndex {
    pub title: String,
    pub structure: Structure,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PantaManifest {
    pub title: String,
    pub structure: Structure,
    pub metadata: Option<Metadata>,
    pub content_sections: Vec<ContentSection>,
}

/// Parse a workspace index file (`index.rhei.md`).
pub fn parse_workspace_index(input: &str) -> Result<WorkspaceIndex> {
    let parsed = parse_manifest(input, RHEI_HEADER, "workspace index")?;
    Ok(WorkspaceIndex {
        title: parsed.title,
        structure: parsed.structure,
        metadata: parsed.metadata,
        content_sections: parsed.content_sections,
    })
}

/// Parse a Panta project manifest file (`index.panta.md`). §FS-rhei-plan-language.1.5
pub fn parse_panta_manifest(input: &str) -> Result<PantaManifest> {
    let parsed = parse_manifest(input, PANTA_HEADER, "Panta manifest")?;
    Ok(PantaManifest {
        title: parsed.title,
        structure: parsed.structure,
        metadata: parsed.metadata,
        content_sections: parsed.content_sections,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestParts {
    title: String,
    structure: Structure,
    metadata: Option<Metadata>,
    content_sections: Vec<ContentSection>,
}

fn parse_manifest(input: &str, header_name: &str, frontmatter_kind: &str) -> Result<ManifestParts> {
    // A leading mark would hide the header. §FS-rhei-language-reference.1 §REQ-cross-platform.9
    let input = crate::source::without_byte_order_mark(input);
    // §FS-rhei-plan-language.2.3
    let re_header = header_regex(&[header_name]);
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

    let mut title: Option<String> = None;
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

        if is_retired_states_line(line) {
            return Err(retired_states_line(line_number));
        }

        if !header_seen && line == "---" {
            return Err(ParseError::new(
                format!(
                    "YAML frontmatter must appear after the `# {header_name}:` header. Move the `---` block below the header."
                ),
                Some(line_number),
            ));
        }

        // Both manifests refuse task metadata outside YAML and fences, even before the header.
        // §FS-rhei-plan-language.1.2 §FS-rhei-plan-language.1.5
        if let Some(field) = TASK_METADATA_FIELDS.iter().find(|field| line.starts_with(**field)) {
            let task_files = if header_name == PANTA_HEADER {
                "a rhei's task files or basin/ task files"
            } else {
                "a task file under tasks/"
            };
            return Err(ParseError::new(
                format!(
                    "{frontmatter_kind} file must not contain task metadata ({field}); move the task into {task_files}"
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

        if !frontmatter_checked {
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

    // Interior blanks are the author's; the ones trailing a section only
    // separate it from the next heading. §FS-rhei-memory.4.2
    for section in &mut content {
        let trimmed = section.content.trim_end().to_string();
        section.content = trimmed;
    }
    Ok(ManifestParts { title, structure, metadata, content_sections: content })
}

/// A parsed bare task file: the nodes it defines, and the authored
/// `metadata.tasks.<id>` entries its own leading frontmatter block carried.
///
/// `metadata` is `None` for a file that opens with a node definition, which is
/// every workspace task file written before the block existed, and every file
/// under `basin/`. §FS-rhei-plan-language.1.4
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceTaskFile {
    pub tasks: Vec<Task>,
    pub metadata: Option<Metadata>,
}

/// Whether a bare task file may open with a metadata block.
///
/// The one difference between the `workspace_task_file` and `basin_task_file`
/// productions, carried as a flag on the shared helpers below rather than as a
/// check in each loader, so the two cannot drift apart.
/// §FS-rhei-plan-language.2
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataBlock {
    /// A file under `tasks/`: the block is admitted and checked.
    Allowed,
    /// A file under `basin/`: the basin's metadata document is the project
    /// manifest, so the block is refused. §FS-rhei-panta.2
    Refused,
}

/// Parse a workspace task file (a file inside the `tasks/` directory).
pub fn parse_workspace_tasks(input: &str) -> Result<Vec<Task>> {
    parse_workspace_task_file(input, None).map(|file| file.tasks)
}

/// Parse a workspace task file using the structure declared by its workspace index. §FS-rhei-authoring.3.3
pub fn parse_workspace_tasks_with_structure(
    input: &str,
    structure: &Structure,
) -> Result<Vec<Task>> {
    parse_workspace_task_file(input, Some(structure)).map(|file| file.tasks)
}

/// Parse a workspace task file, keeping the authored metadata block it may open
/// with. §FS-rhei-plan-language.1.4
pub fn parse_workspace_task_file(
    input: &str,
    structure: Option<&Structure>,
) -> Result<WorkspaceTaskFile> {
    parse_task_file(input, structure, MetadataBlock::Allowed)
}

/// Parse a `basin/` ticket file, which admits no frontmatter of any kind.
/// §FS-rhei-plan-language.2 §FS-rhei-panta.2
pub fn parse_basin_ticket_file(input: &str, structure: Option<&Structure>) -> Result<Vec<Task>> {
    parse_task_file(input, structure, MetadataBlock::Refused).map(|file| file.tasks)
}

/// Parse a workspace task file and collect recoverable task-local parse errors.
///
/// This mirrors [`parse_workspace_tasks`] but preserves `parse_collect`'s
/// multi-error behavior for validation diagnostics. Reported line numbers are
/// adjusted back from the synthetic single-file wrapper to the task file.
pub fn parse_workspace_tasks_collect(input: &str) -> (Option<Vec<Task>>, Vec<ParseError>) {
    let (file, errors) = parse_workspace_task_file_collect(input, None);
    (file.map(|file| file.tasks), errors)
}

/// Parse a workspace task file using the workspace index structure, collecting
/// recoverable task-local parse errors with task-file line numbers.
/// §FS-rhei-authoring.3.3
pub fn parse_workspace_tasks_collect_with_structure(
    input: &str,
    structure: &Structure,
) -> (Option<Vec<Task>>, Vec<ParseError>) {
    let (file, errors) = parse_workspace_task_file_collect(input, Some(structure));
    (file.map(|file| file.tasks), errors)
}

/// Collecting counterpart of [`parse_workspace_task_file`]: every diagnostic of
/// §FS-rhei-validate.4.4 that one file can hold, in one pass.
pub fn parse_workspace_task_file_collect(
    input: &str,
    structure: Option<&Structure>,
) -> (Option<WorkspaceTaskFile>, Vec<ParseError>) {
    parse_task_file_collect(input, structure, MetadataBlock::Allowed)
}

/// Collecting counterpart of [`parse_basin_ticket_file`].
pub fn parse_basin_ticket_file_collect(
    input: &str,
    structure: Option<&Structure>,
) -> (Option<Vec<Task>>, Vec<ParseError>) {
    let (file, errors) = parse_task_file_collect(input, structure, MetadataBlock::Refused);
    (file.map(|file| file.tasks), errors)
}

fn parse_task_file(
    input: &str,
    structure: Option<&Structure>,
    admits: MetadataBlock,
) -> Result<WorkspaceTaskFile> {
    let (leading, body) = split_leading_frontmatter(input)?;
    if let Some(block) = &leading {
        if admits == MetadataBlock::Refused {
            return Err(basin_frontmatter_error(block));
        }
    }
    let mut metadata = match &leading {
        Some(block) => {
            Some(parse_frontmatter(&block.lines, block.start_line, "workspace task file")?)
        }
        None => None,
    };

    let prefix = workspace_task_synthetic_prefix(structure);
    let prefix_line_count = prefix.matches('\n').count();
    let consumed = leading.as_ref().map_or(0, |block| block.consumed);
    let synthetic = format!("{prefix}{body}");
    let tasks = match parse(&synthetic) {
        Ok(rhei) => rhei.tasks,
        Err(mut e) => {
            adjust_workspace_task_error_line(&mut e, prefix_line_count, consumed);
            return Err(e);
        }
    };

    if let (Some(metadata), Some(block)) = (metadata.as_mut(), leading.as_ref()) {
        if let Some(error) = normalize_task_metadata_ids(metadata, block) {
            return Err(error);
        }
        if let Some(error) =
            check_task_metadata_block(metadata, block, Some(&tasks)).into_iter().next()
        {
            return Err(error);
        }
    }
    Ok(WorkspaceTaskFile { tasks, metadata })
}

fn parse_task_file_collect(
    input: &str,
    structure: Option<&Structure>,
    admits: MetadataBlock,
) -> (Option<WorkspaceTaskFile>, Vec<ParseError>) {
    let (leading, body) = match split_leading_frontmatter(input) {
        Ok(split) => split,
        Err(error) => return (None, vec![error]),
    };
    if let Some(block) = &leading {
        if admits == MetadataBlock::Refused {
            return (None, vec![basin_frontmatter_error(block)]);
        }
    }

    let mut errors = Vec::new();
    let mut metadata = match &leading {
        Some(block) => {
            match parse_frontmatter(&block.lines, block.start_line, "workspace task file") {
                Ok(metadata) => Some(metadata),
                Err(error) => {
                    errors.push(error);
                    None
                }
            }
        }
        None => None,
    };

    let prefix = workspace_task_synthetic_prefix(structure);
    let prefix_line_count = prefix.matches('\n').count();
    let consumed = leading.as_ref().map_or(0, |block| block.consumed);
    let synthetic = format!("{prefix}{body}");
    let (maybe_rhei, mut body_errors) = parse_collect(&synthetic);
    for error in &mut body_errors {
        adjust_workspace_task_error_line(error, prefix_line_count, consumed);
    }
    errors.append(&mut body_errors);
    let tasks = maybe_rhei.map(|rhei| rhei.tasks);

    if let (Some(metadata), Some(block)) = (metadata.as_mut(), leading.as_ref()) {
        errors.extend(normalize_task_metadata_ids(metadata, block));
        errors.extend(check_task_metadata_block(metadata, block, tasks.as_deref()));
    }
    (tasks.map(|tasks| WorkspaceTaskFile { tasks, metadata }), errors)
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

/// Move a synthetic-document line back onto the task file it came from: past
/// the prefix the node parser was handed, and forward over the frontmatter
/// block split off the front. A block therefore changes what the loader reads
/// before the body without changing where it says a problem is.
/// §FS-rhei-validate.4.4
fn adjust_workspace_task_error_line(
    error: &mut ParseError,
    prefix_line_count: usize,
    leading_line_count: usize,
) {
    if let Some(ref mut line) = error.line {
        *line = line.saturating_sub(prefix_line_count).saturating_add(leading_line_count);
        if *line == 0 {
            *line = 1;
        }
    }
}
