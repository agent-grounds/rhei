    // The frontmatter half of a placement: the target's index gains the
    // template's node kinds, the depth the placement needs, and the placed
    // tickets' own `metadata.tasks` entries — and loses nothing.

    // §FS-rhei-library.12 §FS-rhei-library.12.1

    /// What a placement adds to the target's index.
    struct IndexAdditions<'a> {
        kinds: &'a [String],
        /// The depth the placed ids actually reach.
        levels: u8,
        /// `metadata.tasks` entries, already re-keyed by the placed ids.
        tasks: &'a BTreeMap<String, YamlValue>,
        /// The `**States:**` line the interim rule of [§FS-rhei-library.10.1]
        /// writes when the root file exists and the index is silent.
        declaration: Option<&'a str>,
    }

    /// The target's index with the additions inserted, keeping every line it
    /// had. §FS-rhei-library.12
    fn union_index(raw: &str, add: &IndexAdditions<'_>) -> MietteResult<String> {
        let mut lines: Vec<String> = raw.lines().map(str::to_owned).collect();
        if let Some(name) = add.declaration {
            let after = lines
                .iter()
                .position(|line| line.starts_with("# Rhei:") || line.starts_with("# Panta:"))
                .map_or(0, |at| at + 1);
            lines.insert(after, format!("**States:** {name}"));
        }
        let span = frontmatter_span(&lines).unwrap_or_else(|| open_frontmatter(&mut lines));
        union_structure(&mut lines, &span, add);
        let span = frontmatter_span(&lines).expect("the frontmatter was just established");
        union_task_metadata(&mut lines, &span, add.tasks)?;
        let mut out = lines.join("\n");
        if raw.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }

    /// The `(start, end)` line indices of the frontmatter body, exclusive of
    /// its two `---` fences. §FS-rhei-plan-language.1.1
    fn frontmatter_span(lines: &[String]) -> Option<(usize, usize)> {
        let open = lines.iter().position(|line| line.trim_end() == "---")?;
        if lines[..open].iter().any(|line| line.starts_with("## ")) {
            return None;
        }
        let close = lines[open + 1..].iter().position(|line| line.trim_end() == "---")?;
        Some((open + 1, open + 1 + close))
    }

    /// Open a frontmatter block on a plan that has none, directly after the
    /// header, which is where the plan language fixes it.
    /// §FS-rhei-plan-language.1.1
    fn open_frontmatter(lines: &mut Vec<String>) -> (usize, usize) {
        let header_end = lines
            .iter()
            .position(|line| line.starts_with("## ") || line.starts_with("### "))
            .unwrap_or(lines.len());
        let at = lines[..header_end]
            .iter()
            .rposition(|line| !line.trim().is_empty())
            .map_or(0, |last| last + 1);
        let block = vec![
            String::new(),
            "---".to_owned(),
            "structure:".to_owned(),
            "---".to_owned(),
        ];
        lines.splice(at..at, block);
        // The body is the one `structure:` line between the two fences.
        (at + 2, at + 3)
    }

    /// `structure`: `maxLevels` grows to the depth the placement needs and
    /// `nodeKinds` gains the template's kinds. §FS-rhei-library.12.1
    fn union_structure(lines: &mut Vec<String>, span: &(usize, usize), add: &IndexAdditions<'_>) {
        let (start, end) = *span;
        let structure = lines[start..end].iter().position(|line| line.trim_end() == "structure:");
        let Some(structure) = structure.map(|at| start + at) else {
            let mut block = vec!["structure:".to_owned()];
            block.push(format!("  maxLevels: {}", add.levels.max(rhei_core::ast::DEFAULT_MAX_LEVELS)));
            block.push("  nodeKinds:".to_owned());
            block.push("  - task".to_owned());
            block.extend(add.kinds.iter().filter(|kind| *kind != "task").map(|kind| format!("  - {kind}")));
            lines.splice(start..start, block);
            return;
        };
        let body_end = |lines: &[String], end: usize| {
            lines[structure + 1..end]
                .iter()
                .position(|line| !line.starts_with(' ') && !line.trim().is_empty())
                .map_or(end, |at| structure + 1 + at)
        };
        let before = lines.len();
        grow_max_levels(lines, structure, body_end(lines, end), add.levels);
        let end = end + (lines.len() - before);
        add_node_kinds(lines, structure, body_end(lines, end), add.kinds);
    }

    /// `maxLevels` is grown rather than refused: the depth a placement needs is
    /// the frontmatter union, not an ordinary create's limit.
    /// §FS-rhei-library.12.1 §FS-rhei-new.3.3
    fn grow_max_levels(lines: &mut Vec<String>, start: usize, end: usize, needed: u8) {
        let Some(at) = lines[start..end].iter().position(|line| line.trim().starts_with("maxLevels:"))
        else {
            if needed > rhei_core::ast::DEFAULT_MAX_LEVELS {
                lines.insert(start + 1, format!("  maxLevels: {needed}"));
            }
            return;
        };
        let at = start + at;
        let current: u8 = lines[at]
            .split_once(':')
            .and_then(|(_, value)| value.trim().parse().ok())
            .unwrap_or(rhei_core::ast::DEFAULT_MAX_LEVELS);
        if current >= needed {
            return;
        }
        let indent = &lines[at][..lines[at].len() - lines[at].trim_start().len()];
        lines[at] = format!("{indent}maxLevels: {needed}");
    }

    /// A template's node kinds join the target's, in the form the target wrote
    /// them in. §FS-rhei-library.12
    fn add_node_kinds(lines: &mut Vec<String>, start: usize, end: usize, kinds: &[String]) {
        let Some(at) = lines[start..end].iter().position(|line| line.trim().starts_with("nodeKinds:"))
        else {
            if kinds.is_empty() {
                return;
            }
            let mut block = vec!["  nodeKinds:".to_owned(), "  - task".to_owned()];
            block.extend(kinds.iter().filter(|kind| *kind != "task").map(|k| format!("  - {k}")));
            lines.splice(end..end, block);
            return;
        };
        let at = start + at;
        let inline = lines[at].split_once(':').map(|(_, rest)| rest.trim().to_owned());
        if let Some(inline) = inline.filter(|rest| !rest.is_empty()) {
            let declared: BTreeSet<String> = inline
                .trim_matches(['[', ']'])
                .split(',')
                .map(|kind| kind.trim().to_ascii_lowercase())
                .collect();
            let missing: Vec<&String> =
                kinds.iter().filter(|kind| !declared.contains(kind.as_str())).collect();
            if missing.is_empty() {
                return;
            }
            let joined = missing.iter().map(|kind| kind.as_str()).collect::<Vec<_>>().join(", ");
            let head = lines[at].trim_end().trim_end_matches(']').to_owned();
            lines[at] = format!("{head}, {joined}]");
            return;
        }
        let last = lines[at + 1..end]
            .iter()
            .position(|line| !line.trim_start().starts_with("- "))
            .map_or(end, |offset| at + 1 + offset);
        let declared: BTreeSet<String> = lines[at + 1..last]
            .iter()
            .filter_map(|line| line.trim().strip_prefix("- ").map(str::to_ascii_lowercase))
            .collect();
        let indent = lines
            .get(at + 1)
            .filter(|line| line.trim_start().starts_with("- "))
            .map_or_else(|| "  ".to_owned(), |line| line[..line.len() - line.trim_start().len()].to_owned());
        let added: Vec<String> = kinds
            .iter()
            .filter(|kind| !declared.contains(kind.as_str()))
            .map(|kind| format!("{indent}- {kind}"))
            .collect();
        lines.splice(last..last, added);
    }

    /// `metadata.tasks` entries travel with their tickets, already re-keyed by
    /// the placed ids. A key both sides hold is the id collision already
    /// refused. §FS-rhei-library.12
    fn union_task_metadata(
        lines: &mut Vec<String>,
        span: &(usize, usize),
        tasks: &BTreeMap<String, YamlValue>,
    ) -> MietteResult<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let (start, end) = *span;
        let mut rendered = String::new();
        for (id, value) in tasks {
            rendered.push_str(&format!("    {id}:\n"));
            let body = serde_yaml::to_string(value).map_err(|err| {
                miette!(
                    help = internal_error_help(),
                    "failed to render the placed ticket's metadata: {err}"
                )
            })?;
            for line in body.lines().filter(|line| !line.trim().is_empty()) {
                rendered.push_str(&format!("      {line}\n"));
            }
        }
        let added: Vec<String> = rendered.lines().map(str::to_owned).collect();
        let metadata = lines[start..end].iter().position(|line| line.trim_end() == "metadata:");
        let Some(metadata) = metadata.map(|at| start + at) else {
            let mut block = vec!["metadata:".to_owned(), "  tasks:".to_owned()];
            block.extend(added);
            lines.splice(end..end, block);
            return Ok(());
        };
        let body_end = lines[metadata + 1..end]
            .iter()
            .position(|line| !line.starts_with(' ') && !line.trim().is_empty())
            .map_or(end, |at| metadata + 1 + at);
        let tasks_at =
            lines[metadata + 1..body_end].iter().position(|line| line.trim_end() == "tasks:");
        let Some(tasks_at) = tasks_at.map(|at| metadata + 1 + at) else {
            let mut block = vec!["  tasks:".to_owned()];
            block.extend(added);
            lines.splice(body_end..body_end, block);
            return Ok(());
        };
        let tasks_end = lines[tasks_at + 1..body_end]
            .iter()
            .position(|line| !line.starts_with("    ") && !line.trim().is_empty())
            .map_or(body_end, |at| tasks_at + 1 + at);
        lines.splice(tasks_end..tasks_end, added);
        Ok(())
    }
