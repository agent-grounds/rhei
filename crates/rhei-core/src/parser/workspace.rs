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

/// The leading `---` block of a bare task file, split off before the file is
/// wrapped in the synthetic single-file plan the node parser reads.
struct LeadingFrontmatter {
    /// The lines between the delimiters, as `parse_frontmatter` wants them.
    lines: Vec<String>,
    /// The opening `---`, 1-based in the task file.
    open_line: usize,
    /// The first line *inside* the block, 1-based in the task file.
    start_line: usize,
    /// How many of the task file's lines the block and the blanks above it
    /// occupy, so the remainder's first line is number `consumed + 1`.
    consumed: usize,
}

/// Split an optional leading frontmatter block off a bare task file, returning
/// it and the remainder the node parser reads.
///
/// The block is parsed *before* the synthetic prefix is prepended rather than
/// merged into it: that prefix already carries a frontmatter block of its own
/// when the index declares `structure`, so merging would let a task file set a
/// plan-wide setting through the plan's own frontmatter position, and the
/// manifest parser refuses a `---` above the `# Rhei:` header. Splitting leaves
/// the prefix byte-for-byte what it was, which is what keeps
/// [`adjust_workspace_task_error_line`] correct.
/// §FS-rhei-plan-language.2 §FS-rhei-validate.4.4
fn split_leading_frontmatter(input: &str) -> Result<(Option<LeadingFrontmatter>, String)> {
    let lines: Vec<&str> = input.lines().collect();
    let mut open = 0usize;
    while open < lines.len() && lines[open].trim().is_empty() {
        open += 1;
    }
    if open >= lines.len() || lines[open].trim() != "---" {
        return Ok((None, input.to_string()));
    }

    let mut body = Vec::new();
    let mut cursor = open + 1;
    while cursor < lines.len() && lines[cursor].trim() != "---" {
        body.push(lines[cursor].to_string());
        cursor += 1;
    }
    if cursor >= lines.len() {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(open + 1),
        ));
    }

    let consumed = cursor + 1;
    let mut remainder = lines[consumed..].join("\n");
    if input.ends_with('\n') && !remainder.is_empty() {
        remainder.push('\n');
    }
    let block =
        LeadingFrontmatter { lines: body, open_line: open + 1, start_line: open + 2, consumed };
    Ok((Some(block), remainder))
}

/// The refusal for frontmatter in a file under `basin/`. §FS-rhei-validate.4.4
fn basin_frontmatter_error(block: &LeadingFrontmatter) -> ParseError {
    ParseError::new(
        "a basin ticket file carries no frontmatter — the basin's manifest is synthetic, so \
         basin ticket metadata lives in `index.panta.md` under project-qualified ids. Move the \
         block there, or rename the directory to make it an ordinary rhei with its own id.",
        Some(block.open_line),
    )
}

/// The line a top-level key of the block sits on, for the code frame.
///
/// Unindented and followed by `:`, which is what a top-level key of a YAML
/// mapping looks like; `None` when the block spells it some other way, and then
/// the error carries the block's own opening line instead.
fn top_level_key_line(block: &LeadingFrontmatter, key: &str) -> Option<usize> {
    block
        .lines
        .iter()
        .position(|line| line.starts_with(key) && line[key.len()..].trim_start().starts_with(':'))
        .map(|offset| block.start_line + offset)
}

/// Every node id a task file defines, as that file spells it — the ids its own
/// `metadata.tasks` entries may name. §FS-rhei-plan-language.1.4
fn defined_ids(tasks: &[Task]) -> Vec<String> {
    fn visit(task: &Task, out: &mut Vec<String>) {
        out.push(task.id.to_string());
        for child in &task.children {
            visit(child, out);
        }
    }
    let mut ids = Vec::new();
    for task in tasks {
        visit(task, &mut ids);
    }
    ids
}

/// A `metadata.tasks` key as a task id: the ids a numbered ticket authors are
/// YAML numbers, and `3` names the same ticket as `"3"`.
fn metadata_task_id(key: &serde_yaml::Value) -> Option<String> {
    match key {
        serde_yaml::Value::String(id) => Some(id.clone()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Hold a task file's frontmatter block to the two rules one file can settle on
/// its own: metadata only, and its own tasks only.
///
/// `tasks` is `None` when the body did not parse, and then the entries name no
/// ids this can check — the body error is the one worth reporting.
/// §FS-rhei-plan-language.1.4 §FS-rhei-validate.4.4
fn check_task_metadata_block(
    metadata: &Metadata,
    block: &LeadingFrontmatter,
    tasks: Option<&[Task]>,
) -> Vec<ParseError> {
    let mut errors = Vec::new();

    for (key, value) in metadata {
        let name = match key {
            serde_yaml::Value::String(name) => name.clone(),
            other => serde_yaml::to_string(other).unwrap_or_default().trim().to_string(),
        };
        if name == "metadata" {
            if !matches!(value, serde_yaml::Value::Mapping(_)) {
                errors.push(ParseError::new(
                    "`metadata` in a workspace task file's frontmatter must be a mapping \
                     carrying `tasks.<id>` entries for the tasks this file defines.",
                    top_level_key_line(block, &name).or(Some(block.open_line)),
                ));
            }
            continue;
        }
        let line = top_level_key_line(block, &name).or(Some(block.open_line));
        // `structure` earns its own wording because it is the one plan-wide key
        // an author plausibly writes into the wrong file. §FS-rhei-validate.4.4
        errors.push(if name == "structure" {
            ParseError::new(
                "`structure` is a plan-wide setting and belongs in `index.rhei.md`, which \
                 declares it once for every task file under `tasks/`. A workspace task file's \
                 frontmatter carries only `metadata.tasks.<id>` for the tasks it defines.",
                line,
            )
        } else {
            ParseError::new(
                format!(
                    "`{name}` is not allowed in a workspace task file's frontmatter: its one \
                     permitted top-level key is `metadata`, carrying `metadata.tasks.<id>` \
                     entries for the tasks this file defines. A plan-wide setting belongs in \
                     `index.rhei.md`."
                ),
                line,
            )
        });
    }

    let Some(tasks) = tasks else {
        return errors;
    };
    let Some(entries) = metadata
        .get(serde_yaml::Value::String("metadata".to_string()))
        .and_then(|section| section.get("tasks"))
    else {
        return errors;
    };
    let serde_yaml::Value::Mapping(entries) = entries else {
        errors.push(ParseError::new(
            "`metadata.tasks` in a workspace task file's frontmatter must be a mapping of task \
             id to that task's metadata.",
            top_level_key_line(block, "metadata").or(Some(block.open_line)),
        ));
        return errors;
    };

    let defined = defined_ids(tasks);
    for key in entries.keys() {
        let Some(id) = metadata_task_id(key) else {
            continue;
        };
        if defined.iter().any(|defined| *defined == id) {
            continue;
        }
        errors.push(ParseError::new(
            format!(
                "`metadata.tasks.{id}` names no task defined in this file. Move the entry to \
                 the file that defines `{id}`, or to `index.rhei.md`."
            ),
            top_level_key_line(block, "metadata").or(Some(block.open_line)),
        ));
    }

    errors
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
    let metadata = match &leading {
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

    if let (Some(metadata), Some(block)) = (metadata.as_ref(), leading.as_ref()) {
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
    let metadata = match &leading {
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

    if let (Some(metadata), Some(block)) = (metadata.as_ref(), leading.as_ref()) {
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
