/// The fence character and its run length, when a line is a code fence.
///
/// CommonMark: a fence is a run of at least three backticks or tildes; the
/// opening one may carry an info string, the closing one may not, and the
/// closing run must be at least as long as the opening one.
///
/// Crate-visible because the prompt composer reads result files by the same
/// rule: a heading inside a fence is a quotation there too.
// §FS-rhei-plan-language.3.6
pub fn code_fence_run(line: &str) -> Option<(char, usize, bool)> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let run = trimmed.chars().take_while(|character| *character == marker).count();
    if run < 3 {
        return None;
    }
    let bare = trimmed[run..].trim().is_empty();
    Some((marker, run, bare))
}

/// One line with its inline code spans removed.
///
/// A span opens on a run of backticks and closes on the next run of the same
/// length; a run that never closes is literal text and stays. Blanking the
/// span rather than the line keeps a real link on the same line checkable.
// §FS-rhei-plan-language.3.6
fn strip_inline_code_spans(line: &str) -> String {
    let characters: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '`' {
            out.push(characters[index]);
            index += 1;
            continue;
        }
        let open = characters[index..].iter().take_while(|character| **character == '`').count();
        let mut cursor = index + open;
        let mut close = None;
        while cursor < characters.len() {
            if characters[cursor] != '`' {
                cursor += 1;
                continue;
            }
            let run =
                characters[cursor..].iter().take_while(|character| **character == '`').count();
            if run == open {
                close = Some(cursor);
                break;
            }
            cursor += run;
        }
        match close {
            Some(end) => index = end + open,
            None => {
                out.extend(std::iter::repeat_n('`', open));
                index += open;
            }
        }
    }
    out
}

/// `text` with every code region blanked and every other line left alone.
///
/// Line structure is preserved so a link written across two lines of prose is
/// still one match. An unclosed fence swallows the rest of the body, which is
/// what a markdown renderer does with it too.
// §FS-rhei-plan-language.3.6
fn markdown_prose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<(char, usize)> = None;
    for line in text.lines() {
        match fence {
            Some((marker, open)) => {
                if let Some((character, run, bare)) = code_fence_run(line) {
                    if character == marker && run >= open && bare {
                        fence = None;
                    }
                }
            }
            None => match code_fence_run(line) {
                Some((marker, run, _)) => fence = Some((marker, run)),
                None => out.push_str(&strip_inline_code_spans(line)),
            },
        }
        out.push('\n');
    }
    out
}

/// Extract markdown links from a text block, returning `(display_text, target)` pairs.
///
/// A link inside a fenced code block or an inline code span is an illustration,
/// not a reference, and is not returned: a plan that documents its own format
/// carries example links, in a task body as in a content section.
// §FS-rhei-plan-language.3.6
fn extract_markdown_links(text: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").expect("valid regex");
    let prose = markdown_prose(text);
    re.captures_iter(&prose).map(|cap| (cap[1].to_string(), cap[2].to_string())).collect()
}

/// Collect all markdown links from every content field in the plan.
///
/// Returns `(location_label, display_text, target)` triples.
fn collect_all_links(rhei: &Rhei) -> Vec<(String, String, String)> {
    let mut links = Vec::new();

    for section in &rhei.content_sections {
        for (display, target) in extract_markdown_links(&section.content) {
            links.push((format!("section '{}'", section.title), display, target));
        }
    }

    for_each_node(rhei, |task| {
        for (display, target) in extract_markdown_links(&task.content) {
            let label = format!("{} {}", title_case_kind(&task.kind), task.id);
            links.push((label, display, target));
        }
    });

    links
}

/// Returns true if the link target looks like an external URL or a fragment-only anchor.
fn is_non_file_link(target: &str) -> bool {
    target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
}

/// Validate that relative markdown links in all content fields point to
/// existing files, resolved against `base_path`.
fn validate_markdown_links(rhei: &Rhei, base_path: &Path, report: &mut ValidationReport) {
    let links = collect_all_links(rhei);

    for (location, display, target) in &links {
        validate_one_markdown_link(location, display, target, base_path, report);
    }
}

fn validate_markdown_links_with_task_bases(
    rhei: &Rhei,
    default_base: &Path,
    task_bases: &HashMap<String, PathBuf>,
    section_bases: &[PathBuf],
    report: &mut ValidationReport,
) {
    for (index, section) in rhei.content_sections.iter().enumerate() {
        for (display, target) in extract_markdown_links(&section.content) {
            let location = format!("section '{}'", section.title);
            let base = section_bases.get(index).map(PathBuf::as_path).unwrap_or(default_base);
            validate_one_markdown_link(&location, &display, &target, base, report);
        }
    }

    for_each_node(rhei, |task| {
        let label = format!("{} {}", title_case_kind(&task.kind), task.id);
        let task_id = task.id.to_string();
        let base = task_bases.get(&task_id).map(PathBuf::as_path).unwrap_or(default_base);
        for (display, target) in extract_markdown_links(&task.content) {
            validate_one_markdown_link(&label, &display, &target, base, report);
        }
    });
}

fn validate_one_markdown_link(
    location: &str,
    display: &str,
    target: &str,
    base_path: &Path,
    report: &mut ValidationReport,
) {
    if is_non_file_link(target) {
        return;
    }

    // Strip fragment (e.g. "file.md#section" → "file.md")
    let file_part = target.split('#').next().unwrap_or(target);
    if file_part.is_empty() {
        return; // pure fragment link, already handled above
    }

    let resolved = base_path.join(file_part);
    if !resolved.exists() {
        report.errors.push(format!(
            "{} contains a link [{}]({}) but '{}' does not exist",
            location, display, target, file_part
        ));
    }
}

/// The ticket id as it reads inside its own rhei file: the project-qualified
/// id with the rhei-prefix segments removed. `None` when the task carries no
/// prefix (already rhei-local). §AR-rhei-panta.3
fn rhei_local_task_id(task: &Task) -> Option<String> {
    let prefix = task.profile_depth_offset as usize;
    if prefix == 0 || task.id.segments.len() <= prefix {
        return None;
    }
    Some(
        task.id.segments[prefix..]
            .iter()
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>()
            .join("."),
    )
}

fn validate_result_blocks(rhei: &Rhei, machines: &MachineSet, report: &mut ValidationReport) {
    let re = Regex::new(r"^> \*\*Result:\*\* \[([^\]]*)\]\(([^)]+)\)\s*$")
        .expect("valid result block regex");

    for_each_node(rhei, |task| {
        let mut in_code_block = false;
        let mut valid_blocks = Vec::new();
        let label = format!("{} {}", title_case_kind(&task.kind), task.id);

        for line in task.content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block || !trimmed.starts_with("> **Result:**") {
                continue;
            }

            let Some(caps) = re.captures(trimmed) else {
                report.errors.push(format!(
                    "{} has malformed result block; expected '> **Result:** [{}](runtime/results/{}.md)'",
                    label, task.id, task.id
                ));
                continue;
            };
            valid_blocks.push((caps[1].to_string(), caps[2].to_string()));
        }

        if valid_blocks.len() > 1 {
            report.errors.push(format!(
                "{} has {} result blocks; at most one is allowed",
                label,
                valid_blocks.len()
            ));
        }

        if valid_blocks.is_empty() {
            return;
        }

        let machine = machines.for_task(&task.id);
        let parsed = parse_task_state(&task.state, machine);
        let is_terminal =
            machine.states.get(&parsed.state).map(|state| state.terminal).unwrap_or(false);
        if !is_terminal {
            report.errors.push(format!(
                "{} is in non-terminal state '{}' but contains a result block",
                label, task.state
            ));
        }

        let expected_text = task.id.to_string();
        let expected_target = format!("runtime/results/{}.md", task.id);
        // §FS-rhei-panta.6.3: text and target validate as a pair — both
        // qualified or both legacy rhei-local. A mixed link names two
        // different artifacts and is an error.
        let legacy_text = rhei_local_task_id(task);
        let legacy_target = legacy_text.as_ref().map(|id| format!("runtime/results/{id}.md"));
        for (display, target) in valid_blocks {
            let qualified = display == expected_text && target == expected_target;
            let legacy = Some(display.as_str()) == legacy_text.as_deref()
                && Some(target.as_str()) == legacy_target.as_deref();
            if qualified || legacy {
                continue;
            }
            // Renaming a ticket id is a two-part edit — the link text *and* the
            // artifact it points at. Naming only the `mv` sent authors round a
            // second time: they moved the file, the stale link then failed as a
            // dangling target, and the new error read as a different problem.
            let rename_hint = format!(
                ". If this ticket was renamed, do both halves: rewrite the block as \
                 `> **Result:** [{expected_text}](runtime/results/{expected_text}.md)`, and \
                 move the artifact to match with \
                 `mv {target} runtime/results/{expected_text}.md`"
            );
            match legacy_text.as_deref() {
                Some(local) => report.errors.push(format!(
                    "{} result block must link '[{}](runtime/results/{}.md)' \
                     (or the legacy '[{}](runtime/results/{}.md)'), got '[{}]({})'{}",
                    label, expected_text, expected_text, local, local, display, target, rename_hint
                )),
                None => report.errors.push(format!(
                    "{} result block must link '[{}](runtime/results/{}.md)', got '[{}]({})'{}",
                    label, expected_text, expected_text, display, target, rename_hint
                )),
            }
        }
    });
}

/// Detect cycles using Kahn's algorithm; report a generic cycle set on failure.
fn validate_circular_dependencies(
    _rhei: &Rhei,
    index: &HashMap<TaskId, &Task>,
    report: &mut ValidationReport,
) {
    // Build adjacency as dep -> dependent
    let mut nodes: HashSet<TaskId> = index.keys().cloned().collect();
    let mut adj: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    let mut indegree: HashMap<TaskId, usize> = HashMap::new();

    for n in nodes.clone() {
        adj.entry(n.clone()).or_default();
        indegree.entry(n).or_insert(0);
    }

    for task in index.values() {
        // task depends on deps; edges: dep -> task.id
        for dep in &task.prior {
            // Include unseen dependency as a node to make cycle detection robust even if integrity check was skipped.
            nodes.insert(dep.clone());
            adj.entry(dep.clone()).or_default().push(task.id.clone());
            *indegree.entry(task.id.clone()).or_insert(0) += 1;
            indegree.entry(dep.clone()).or_insert(0);
        }
    }

    // Kahn's algorithm
    let mut q: VecDeque<TaskId> =
        indegree.iter().filter_map(|(n, &d)| if d == 0 { Some(n.clone()) } else { None }).collect();
    let mut processed = 0usize;

    while let Some(n) = q.pop_front() {
        processed += 1;
        if let Some(neigh) = adj.get(&n) {
            for m in neigh {
                if let Some(d) = indegree.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        q.push_back(m.clone());
                    }
                }
            }
        }
    }

    if processed != indegree.len() {
        // Collect nodes still with indegree > 0
        let cyc_nodes: Vec<String> = indegree
            .iter()
            .filter_map(|(n, &d)| if d > 0 { Some(n.to_string()) } else { None })
            .collect();
        if !cyc_nodes.is_empty() {
            report
                .errors
                .push(format!("Circular dependency detected among tasks: {:?}", cyc_nodes));
        } else {
            report
                .errors
                .push("Circular dependency detected (unable to isolate nodes)".to_string());
        }
    }
}

// ---------------------------
// Tests
// ---------------------------
