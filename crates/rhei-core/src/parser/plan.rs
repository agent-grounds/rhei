use crate::ast::{
    ConsumedExport, ContentSection, Metadata, Rhei, Structure, Task, TaskExclusion,
    MAX_ALLOWED_LEVELS,
};
use crate::text::parse_task_id;
use regex::Regex;

use super::builder::{title_case_kind, unwind_to_level, NodeBuilder};
use super::{parse_frontmatter, parse_structure, unescape_state, ParseError, Result};

pub fn parse(input: &str) -> Result<Rhei> {
    let re_rhei = Regex::new(r#"^#\s+Rhei:\s+(.*)$"#).unwrap();
    let re_tasks = Regex::new(r#"^##\s+Tasks\s*$"#).unwrap();
    let task_id_segment = r#"(?:[A-Za-z][A-Za-z0-9_-]*|0|[1-9][0-9]*)"#;
    let task_id_pattern = format!(r#"{task_id_segment}(?:\.{task_id_segment})*"#);
    let re_node_header = Regex::new(&format!(
        r#"^(#{{3,6}})\s+([A-Za-z][A-Za-z0-9_-]*)\s+({task_id_pattern}):\s+(.*)$"#
    ))
    .unwrap();
    let re_any_h3_to_h6 = Regex::new(r#"^#{3,6}\s+\S.*$"#).unwrap();
    let re_states_decl = Regex::new(r#"^\*\*States:\*\*\s+(.+)$"#).unwrap();
    let re_state = Regex::new(r#"^\*\*State:\*\*\s*(.+)$"#).unwrap();
    let re_state_like = Regex::new(r#"^\*\*State\b.*$"#).unwrap();
    // The kind keyword is optional decoration; accepting the bare form lets an
    // author paste back the id every surface prints. §FS-rhei-plan-language.5
    let re_prior_ref =
        Regex::new(&format!(r#"^(?:([A-Za-z][A-Za-z0-9_-]*)\s+)?({task_id_pattern})$"#)).unwrap();
    let re_prior_like = Regex::new(r#"^\*\*Prior\b.*$"#).unwrap();
    // Export names key a file on disk, so they are restricted to what is safe
    // in a path segment. §FS-rhei-plan-language.3.12
    let export_name_pattern = r#"[A-Za-z0-9][A-Za-z0-9._-]*"#;
    let re_export_name = Regex::new(&format!(r#"^{export_name_pattern}$"#)).unwrap();
    let re_consumes_ref =
        Regex::new(&format!(r#"^({task_id_pattern}):({export_name_pattern})$"#)).unwrap();
    let re_provides_like = Regex::new(r#"^\*\*Provides\b.*$"#).unwrap();
    let re_consumes_like = Regex::new(r#"^\*\*Consumes\b.*$"#).unwrap();
    let re_excludes_like = Regex::new(r#"^\*\*Excludes\b.*$"#).unwrap();
    let re_assignee = Regex::new(r#"^\*\*Assignee:\*\*\s*(.+)$"#).unwrap();
    let re_assignee_like = Regex::new(r#"^\*\*Assignee\b.*$"#).unwrap();
    let re_model = Regex::new(r#"^\*\*Model:\*\*\s*(.+)$"#).unwrap();
    let re_model_like = Regex::new(r#"^\*\*Model(?::\*\*\s*|\*\*.*)$"#).unwrap();
    let re_target = Regex::new(r#"^\*\*Target:\*\*\s*(.+)$"#).unwrap();
    let re_target_like = Regex::new(r#"^\*\*Target(?::\*\*\s*|\*\*.*)$"#).unwrap();
    // Any `**Word:**` line opening a task's metadata block. Recognized fields
    // are matched before this, so what reaches it is a field name Rhei does
    // not know — almost always a misspelling of one it does.
    let re_unknown_metadata = Regex::new(r#"^\*\*([A-Za-z][A-Za-z0-9 _-]*):\*\*"#).unwrap();
    let re_h2_heading = Regex::new(r#"^##\s+\S.*$"#).unwrap();
    let re_section_header = Regex::new(r#"^##\s+(.+)$"#).unwrap();

    let mut in_code_block = false;
    let mut in_tasks_section = false;
    let mut tasks_section_line: Option<usize> = None;
    let mut pre_tasks_h2_seen = false;
    let mut first_nonempty_line: Option<usize> = None;

    let mut rhei_title: Option<String> = None;
    let mut rhei_header_seen = false;
    let mut rhei_states: Option<String> = None;
    let mut rhei_states_checked = false;
    let mut rhei_metadata: Option<Metadata> = None;
    let mut frontmatter_checked = false;
    let mut in_frontmatter = false;
    let mut frontmatter_start_line = 0usize;
    let mut frontmatter_lines: Vec<String> = Vec::new();
    let mut rhei_content: Vec<ContentSection> = Vec::new();
    let mut tasks: Vec<Task> = Vec::new();
    let mut node_stack: Vec<NodeBuilder> = Vec::new();

    // Structure is finalised once the frontmatter boundary is passed.
    let mut structure: Structure = Structure::default();
    let mut structure_finalised = false;

    for (idx, raw) in input.lines().enumerate() {
        let line_number = idx + 1;
        let line = raw.trim();

        if !line.is_empty() {
            first_nonempty_line.get_or_insert(line_number);
        }

        if in_frontmatter {
            if line == "---" {
                let metadata =
                    parse_frontmatter(&frontmatter_lines, frontmatter_start_line, "plan")?;
                structure = parse_structure(Some(&metadata), frontmatter_start_line)?;
                structure_finalised = true;
                rhei_metadata = Some(metadata);
                in_frontmatter = false;
                continue;
            }
            frontmatter_lines.push(raw.to_string());
            continue;
        }

        let trimmed_start = raw.trim_start();
        let is_fence = trimmed_start.starts_with("```");
        if is_fence {
            in_code_block = !in_code_block;

            if in_tasks_section {
                if let Some(top) = node_stack.last_mut() {
                    top.metadata_closed = true;
                    if !top.content.is_empty() {
                        top.content.push('\n');
                    }
                    top.content.push_str(raw);
                }
            } else if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            continue;
        }

        if line.is_empty() {
            if in_tasks_section {
                if let Some(top) = node_stack.last_mut() {
                    top.blank_line_seen = true;
                    top.content.push('\n');
                }
            } else if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                // A content section is pasted verbatim into a prompt
                // (§FS-rhei-memory.4.2), so a paragraph break is part of the
                // text — inside a fence and outside one alike.
                if !content.is_empty() {
                    content.push('\n');
                }
            }
            continue;
        }

        if !rhei_header_seen && !in_code_block && line == "---" {
            return Err(ParseError::new(
                "YAML frontmatter must appear after the `# Rhei:` header (and any `**States:**` declaration). Move the `---` block below the header.",
                Some(line_number),
            ));
        }

        if !in_tasks_section && !in_code_block {
            if let Some(cap) = re_rhei.captures(line) {
                rhei_title = Some(cap.get(1).unwrap().as_str().to_string());
                rhei_header_seen = true;
                continue;
            }

            if rhei_header_seen && !rhei_states_checked {
                if let Some(cap) = re_states_decl.captures(line) {
                    rhei_states = Some(cap.get(1).unwrap().as_str().trim().to_string());
                    rhei_states_checked = true;
                    continue;
                }
                rhei_states_checked = true;
            }

            if rhei_header_seen && rhei_states_checked && !frontmatter_checked {
                if line == "---" {
                    in_frontmatter = true;
                    frontmatter_checked = true;
                    frontmatter_start_line = line_number + 1;
                    frontmatter_lines.clear();
                    continue;
                }
                frontmatter_checked = true;
            }

            if !structure_finalised && frontmatter_checked {
                // No frontmatter block was opened — apply defaults.
                structure_finalised = true;
            }

            if rhei_states_checked && re_states_decl.is_match(line) {
                return Err(ParseError::new(
                    "**States:** declaration must be the first non-empty line after the Rhei header",
                    Some(line_number),
                ));
            }

            let is_top_level_h1 = line.starts_with('#') && !line.starts_with("##");
            if !rhei_header_seen && is_top_level_h1 {
                return Err(ParseError::new(
                    "Malformed rhei heading: expected '# Rhei: <title>'",
                    Some(line_number),
                ));
            }

            if re_tasks.is_match(line) {
                in_tasks_section = true;
                tasks_section_line = Some(line_number);
                continue;
            }

            if re_h2_heading.is_match(line) {
                pre_tasks_h2_seen = true;
                if let Some(cap) = re_section_header.captures(line) {
                    let section_title = cap.get(1).unwrap().as_str().trim().to_string();
                    rhei_content.push(ContentSection {
                        title: section_title,
                        content: String::new(),
                        rhei: None,
                    });
                }
                continue;
            }

            if re_state_like.is_match(line)
                || re_prior_like.is_match(line)
                || re_provides_like.is_match(line)
                || re_consumes_like.is_match(line)
                || re_excludes_like.is_match(line)
                || re_assignee_like.is_match(line)
                || re_model_like.is_match(line)
                || re_target_like.is_match(line)
            {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            }

            if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            continue;
        }

        if in_code_block {
            if in_tasks_section {
                if let Some(top) = node_stack.last_mut() {
                    if !top.content.is_empty() {
                        top.content.push('\n');
                    }
                    top.content.push_str(raw);
                }
            } else if let Some(ContentSection { content, .. }) = rhei_content.last_mut() {
                // Outside `## Tasks` the node stack is empty, so a fenced
                // block's body used to be dropped between its two fence lines.
                // A content section is pasted verbatim. §FS-rhei-memory.4.2
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(raw);
            }
            continue;
        }

        if let Some(caps) = re_node_header.captures(line) {
            let hashes = caps.get(1).unwrap().as_str();
            let level = hashes.len() as u8;
            let kind_raw = caps.get(2).unwrap().as_str();
            let id_str = caps.get(3).unwrap().as_str();
            let title = caps.get(4).unwrap().as_str().to_string();

            let kind_canonical = kind_raw.to_ascii_lowercase();
            if !structure.accepts_kind(&kind_canonical) {
                // Naming the fault without the fix sends the author hunting:
                // node kinds are declared in the plan's own frontmatter, which
                // is not where a heading error makes anyone look.
                return Err(ParseError::new(
                    format!(
                        "unknown node kind `{}` at heading; this plan declares {:?}. \
                         Add it to `structure.nodeKinds` in the plan's YAML frontmatter \
                         (`structure:` / `  nodeKinds: [{}, {}]`), or use one of the \
                         declared kinds",
                        kind_raw,
                        structure.node_kinds,
                        structure.node_kinds.join(", "),
                        kind_canonical
                    ),
                    Some(line_number),
                ));
            }

            let depth = level.saturating_sub(2); // H3 -> depth 1
            if depth == 0 || depth > MAX_ALLOWED_LEVELS {
                return Err(ParseError::new(
                    format!("node heading depth out of range at level {level}"),
                    Some(line_number),
                ));
            }
            if depth > structure.max_levels {
                return Err(ParseError::new(
                    format!(
                        "node depth {depth} exceeds `structure.maxLevels` ({})",
                        structure.max_levels
                    ),
                    Some(line_number),
                ));
            }

            let id = parse_task_id(id_str).ok_or_else(|| {
                ParseError::new(
                    format!("malformed task id `{id_str}` in node heading"),
                    Some(line_number),
                )
            })?;

            if id.depth() as u8 != depth {
                return Err(ParseError::new(
                    format!(
                        "heading depth {depth} does not match id path depth {}; \
                         level-{level} nodes require {depth}-segment ids",
                        id.depth()
                    ),
                    Some(line_number),
                ));
            }

            if title.trim().is_empty() {
                return Err(ParseError::new(
                    format!(
                        "malformed node heading: expected '{} {} <id>: <title>'",
                        "#".repeat(level as usize),
                        title_case_kind(&kind_canonical)
                    ),
                    Some(line_number),
                ));
            }

            // Finalise any open nodes at >= this depth and attach them.
            unwind_to_level(&mut node_stack, &mut tasks, level)?;

            if depth > 1 {
                let Some(parent) = node_stack.last() else {
                    return Err(ParseError::new(
                        format!("level-{level} node `{id}` has no enclosing parent task"),
                        Some(line_number),
                    ));
                };
                if !id.extends(&parent.id) {
                    return Err(ParseError::new(
                        format!(
                            "child id `{id}` must extend parent id `{}` by exactly one segment",
                            parent.id
                        ),
                        Some(line_number),
                    ));
                }
            }

            node_stack.push(NodeBuilder {
                id,
                kind: kind_canonical,
                title,
                level,
                state: None,
                prior: Vec::new(),
                prior_kinds: Vec::new(),
                provides: Vec::new(),
                consumes: Vec::new(),
                excludes: Vec::new(),
                assignee: None,
                model: None,
                target: None,
                content: String::new(),
                children: Vec::new(),
                metadata_closed: false,
                blank_line_seen: false,
                heading_line: line_number,
            });
            continue;
        }

        if re_any_h3_to_h6.is_match(line) {
            return Err(ParseError::new(
                "Malformed node heading: expected '### <Kind> <id>: <title>' (for example `### Task 1: Title`)",
                Some(line_number),
            ));
        }

        // **State:** metadata
        if let Some(caps) = re_state.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            let raw_state = caps.get(1).unwrap().as_str().trim();
            top.state = Some(unescape_state(raw_state));
            continue;
        }

        if re_state_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**State:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Prior:** metadata
        if line.starts_with("**Prior:**") {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Prior:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if !top.provides.is_empty() || !top.consumes.is_empty() || !top.excludes.is_empty() {
                return Err(ParseError::new(
                    format!("**Prior:** must appear before export metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            let prior_value = line.strip_prefix("**Prior:**").unwrap_or_default();
            let mut ids = Vec::new();
            let mut kinds = Vec::new();
            for item in prior_value.split(',') {
                let item = item.trim();
                let Some(caps) = re_prior_ref.captures(item) else {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Prior:** reference '{item}': expected a task id \
                             (`**Prior:** 1`, `**Prior:** auth.2`), optionally prefixed with \
                             its node kind (`**Prior:** Task auth.2`); separate several with \
                             commas"
                        ),
                        Some(line_number),
                    ));
                };
                let Some(id) = caps.get(2).and_then(|m| parse_task_id(m.as_str())) else {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Prior:** reference '{item}': '{}' is not a valid task id",
                            caps.get(2).map(|m| m.as_str()).unwrap_or(item)
                        ),
                        Some(line_number),
                    ));
                };
                ids.push(id);
                // Kept as authored; the validator checks it against the
                // referenced node's kind. §FS-rhei-plan-language.3.1
                kinds.push(caps.get(1).map(|m| m.as_str().to_string()));
            }
            if ids.is_empty() {
                return Err(ParseError::new(
                    "Empty **Prior:** field: name at least one task id, or drop the line",
                    Some(line_number),
                ));
            }
            top.prior.extend(ids);
            top.prior_kinds.extend(kinds);
            continue;
        }

        if re_prior_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Prior:** Task <id>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Provides:** metadata — export names this task publishes.
        // §FS-rhei-plan-language.3.12
        if line.starts_with("**Provides:**") {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Provides:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if !top.excludes.is_empty() {
                return Err(ParseError::new(
                    format!("**Provides:** must appear before **Excludes:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            let mut names = Vec::new();
            for item in line.strip_prefix("**Provides:**").unwrap_or_default().split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                if !re_export_name.is_match(item) {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Provides:** name '{item}': an export name starts with \
                             a letter or digit and continues with letters, digits, '.', '_', or \
                             '-' (`**Provides:** api-contract`); separate several with commas"
                        ),
                        Some(line_number),
                    ));
                }
                // A repeated name would make the export ambiguous to name in a
                // consumer, and one of the two writers always loses.
                if names.iter().any(|seen| seen == item) || top.provides.iter().any(|s| s == item) {
                    return Err(ParseError::new(
                        format!(
                            "Task {} provides '{item}' more than once; drop the duplicate",
                            top.id
                        ),
                        Some(line_number),
                    ));
                }
                names.push(item.to_string());
            }
            if names.is_empty() {
                return Err(ParseError::new(
                    "Empty **Provides:** field: name at least one export, or drop the line",
                    Some(line_number),
                ));
            }
            top.provides.extend(names);
            continue;
        }

        if re_provides_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Provides:** <export-name>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Consumes:** metadata — exports this task reads from prior tasks.
        // §FS-rhei-plan-language.3.12
        if line.starts_with("**Consumes:**") {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Consumes:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if !top.excludes.is_empty() {
                return Err(ParseError::new(
                    format!("**Consumes:** must appear before **Excludes:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            let mut refs = Vec::new();
            for item in line.strip_prefix("**Consumes:**").unwrap_or_default().split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let Some(caps) = re_consumes_ref.captures(item) else {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Consumes:** reference '{item}': expected \
                             '<task-id>:<export-name>' (`**Consumes:** auth.1:api-contract`); \
                             separate several with commas"
                        ),
                        Some(line_number),
                    ));
                };
                let Some(task) = caps.get(1).and_then(|m| parse_task_id(m.as_str())) else {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Consumes:** reference '{item}': '{}' is not a valid task id",
                            caps.get(1).map(|m| m.as_str()).unwrap_or(item)
                        ),
                        Some(line_number),
                    ));
                };
                let name = caps.get(2).unwrap().as_str().to_string();
                if refs.iter().any(|prev: &ConsumedExport| prev.task == task && prev.name == name)
                    || top.consumes.iter().any(|prev| prev.task == task && prev.name == name)
                {
                    return Err(ParseError::new(
                        format!(
                            "Task {} consumes '{item}' more than once; drop the duplicate",
                            top.id
                        ),
                        Some(line_number),
                    ));
                }
                refs.push(ConsumedExport { task, name });
            }
            if refs.is_empty() {
                return Err(ParseError::new(
                    "Empty **Consumes:** field: name at least one export, or drop the line",
                    Some(line_number),
                ));
            }
            top.consumes.extend(refs);
            continue;
        }

        if re_consumes_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Consumes:** <task-id>:<export-name>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Excludes:** metadata — sources this task must not read.
        // §FS-rhei-plan-language.2 §FS-rhei-plan-language.3.13
        if line.starts_with("**Excludes:**") {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Excludes:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.assignee.is_some() || top.model.is_some() || top.target.is_some() {
                return Err(ParseError::new(
                    format!(
                        "**Excludes:** must appear before **Assignee:**, **Model:**, and **Target:** for Task {}",
                        top.id
                    ),
                    Some(line_number),
                ));
            }
            let mut entries = Vec::new();
            for raw_entry in line.strip_prefix("**Excludes:**").unwrap_or_default().split(',') {
                let entry = raw_entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let parsed = if let Some(path) = entry.strip_prefix("checkout=") {
                    parse_exclusion_path(path, "checkout", line_number)?
                } else if let Some(path) = entry.strip_prefix("artifact=") {
                    parse_exclusion_path(path, "artifact", line_number)?
                } else if let Some(caps) = re_consumes_ref.captures(entry) {
                    let task = caps
                        .get(1)
                        .and_then(|m| parse_task_id(m.as_str()))
                        .expect("exclusion regex captures a valid task id");
                    TaskExclusion::Export(ConsumedExport {
                        task,
                        name: caps.get(2).unwrap().as_str().to_string(),
                    })
                } else {
                    return Err(ParseError::new(
                        format!(
                            "Malformed **Excludes:** entry '{entry}': expected checkout=<path>, \
                             artifact=<path>, or <task-id>:<export-name>"
                        ),
                        Some(line_number),
                    ));
                };
                entries.push(parsed);
            }
            if entries.is_empty() {
                return Err(ParseError::new(
                    "Empty **Excludes:** field: name at least one path or export, or drop the line",
                    Some(line_number),
                ));
            }
            top.excludes.extend(entries);
            continue;
        }

        if re_excludes_like.is_match(line) {
            return Err(ParseError::new(
                "Malformed metadata field: expected '**Excludes:** checkout=<path>, artifact=<path>, or <task-id>:<export-name>'",
                Some(line_number),
            ));
        }

        // **Assignee:** metadata
        if let Some(caps) = re_assignee.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Assignee:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.assignee.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Assignee:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.assignee = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_assignee_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Assignee:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Model:** metadata
        if let Some(caps) = re_model.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Model:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.model.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Model:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.model = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_model_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Model:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        // **Target:** metadata
        if let Some(caps) = re_target.captures(line) {
            let Some(top) = node_stack.last_mut() else {
                return Err(ParseError::new(
                    "Metadata field appears outside a task",
                    Some(line_number),
                ));
            };
            if top.metadata_closed {
                return Err(ParseError::new(
                    "Metadata fields must appear immediately after the task heading before task content",
                    Some(line_number),
                ));
            }
            if top.state.is_none() {
                return Err(ParseError::new(
                    format!("**State:** must appear before **Target:** for Task {}", top.id),
                    Some(line_number),
                ));
            }
            if top.target.is_some() {
                return Err(ParseError::new(
                    format!("Duplicate **Target:** metadata for Task {}", top.id),
                    Some(line_number),
                ));
            }
            top.target = Some(caps.get(1).unwrap().as_str().trim().to_string());
            continue;
        }

        if re_target_like.is_match(line) {
            if node_stack.last().is_some() {
                return Err(ParseError::new(
                    "Malformed metadata field: expected '**Target:** <value>'",
                    Some(line_number),
                ));
            }
            return Err(ParseError::new(
                "Metadata field appears outside a task",
                Some(line_number),
            ));
        }

        if re_h2_heading.is_match(line) {
            return Err(ParseError::new(
                "Tasks section must be the final '##' chapter and appear as '## Tasks'",
                Some(line_number),
            ));
        }

        // An unrecognized `**Field:**` where metadata goes is a typo, not
        // prose: falling through to content drops a mistyped `**Priorr:**`
        // and leaves a green plan with no dependency. §FS-rhei-plan-language.2
        if let Some(top) = node_stack.last() {
            if !top.metadata_closed && !top.blank_line_seen {
                if let Some(caps) = re_unknown_metadata.captures(line) {
                    let field = caps.get(1).unwrap().as_str();
                    return Err(ParseError::new(
                        format!(
                            "Unknown metadata field '**{field}:**' for Task {}. Task metadata \
                             is one of **State:**, **Prior:**, **Provides:**, **Consumes:**, \
                             **Excludes:**, **Assignee:**, **Model:**, **Target:**. Leave a blank line before \
                             this line to keep it as task content.",
                            top.id
                        ),
                        Some(line_number),
                    ));
                }
            }
        }

        // Content line: append to the innermost open node.
        if let Some(top) = node_stack.last_mut() {
            top.metadata_closed = true;
            if !top.content.is_empty() {
                top.content.push('\n');
            }
            top.content.push_str(raw);
        }
    }

    if in_frontmatter {
        return Err(ParseError::new(
            "Unterminated YAML frontmatter: missing closing '---'",
            Some(frontmatter_start_line.saturating_sub(1).max(1)),
        ));
    }

    // Finalise remaining open nodes.
    unwind_to_level(&mut node_stack, &mut tasks, 0)?;

    let title = match rhei_title {
        Some(t) => t,
        None => {
            return Err(ParseError::new(
                "Missing '# Rhei: <title>' header",
                first_nonempty_line.or(tasks_section_line),
            ));
        }
    };

    if !in_tasks_section {
        return Err(ParseError::new(
            if pre_tasks_h2_seen {
                "Tasks section must be the final '##' chapter and appear as '## Tasks'"
            } else {
                "Missing '## Tasks' section"
            },
            None,
        ));
    }

    // An empty `## Tasks` section is a valid, empty rhei — the state a rhei
    // passes through between being created and receiving its first ticket.
    // The Directory Workspace format has always accepted one, and the two
    // formats describe the same rhei; `rhei validate` warns instead of
    // failing, so an accidentally emptied rhei is still named.

    // §FS-rhei-plan-language.1.1 §FS-rhei-new.2

    let states_declared = rhei_states.is_some();
    // Interior blanks are the author's; the ones trailing a section only
    // separate it from `## Tasks`. §FS-rhei-memory.4.2
    for section in &mut rhei_content {
        let trimmed = section.content.trim_end().to_string();
        section.content = trimmed;
    }
    Ok(Rhei {
        title,
        states: rhei_states.unwrap_or_else(|| "rhei".to_string()),
        states_declared,
        structure,
        metadata: rhei_metadata,
        content_sections: rhei_content,
        tasks,
    })
}

/// Parse and lexically validate one portable path exclusion. Files and
/// directories share a spelling; a trailing slash alone selects recursive
/// directory semantics. §FS-rhei-plan-language.3.13
fn parse_exclusion_path(path: &str, kind: &str, line_number: usize) -> Result<TaskExclusion> {
    let recursive = path.ends_with('/');
    let body = path.strip_suffix('/').unwrap_or(path);
    let invalid_component =
        body.split('/').find(|part| part.is_empty() || *part == "." || *part == "..");
    let prefixed = body.starts_with('/')
        || body.starts_with('\\')
        || body.as_bytes().get(1).is_some_and(|byte| *byte == b':');
    if body.is_empty() || prefixed || body.contains('\\') || invalid_component.is_some() {
        let reason = match invalid_component {
            Some("..") => "must not contain '..'",
            Some(".") => "must not contain '.' components",
            Some("") => "must not contain empty path components",
            _ if body.is_empty() => "must not be empty",
            _ if prefixed => "must be relative to its declared root",
            _ => "must use '/' separators",
        };
        return Err(ParseError::new(
            format!("Malformed **Excludes:** {kind} path '{path}': {reason}"),
            Some(line_number),
        ));
    }
    Ok(match kind {
        "checkout" => TaskExclusion::Checkout { path: path.to_string(), recursive },
        _ => TaskExclusion::Artifact { path: path.to_string(), recursive },
    })
}
