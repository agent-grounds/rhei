/// The plan files a reset rewrites, each paired with a sample qualified task
/// id from that file — the handle its owning rhei's machine resolves through.
/// §DA-per-rhei-state-machines
fn reset_target_files(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
) -> Vec<(PathBuf, String)> {
    if loaded.task_sources.is_empty() {
        // Only a bare plan file is itself the rewrite target; an empty
        // project or workspace has no plan files to rewrite — resetting it
        // is a no-op, not an error. §FS-rhei-panta.6
        return if input.is_file() {
            vec![(input.to_path_buf(), String::new())]
        } else {
            Vec::new()
        };
    }

    // §FS-rhei-panta.6.4: `--rhei` narrows which rheis are reset.
    let mut files = loaded
        .task_sources
        .iter()
        .filter(|(task_id, _)| task_in_rhei_scope(scope, task_id))
        .map(|(task_id, path)| (path.clone(), task_id.clone()))
        .collect::<Vec<_>>();
    files.sort();
    files.dedup_by(|a, b| a.0 == b.0);
    files
}

/// `authored` maps this file's *local* task ids to the state each was authored
/// in; a task absent from it never moved and keeps the line it has.
/// §FS-rhei-reset.2.2
fn reset_plan_file_states(
    path: &Path,
    authored: &BTreeMap<String, String>,
    locked: &LockedPlanFile,
) -> MietteResult<()> {
    let raw = locked.read_to_string("failed to read plan file")?;
    let new_raw = rewrite_states_to_authored(&raw, authored)?;
    let new_raw = strip_result_links(&new_raw);
    let new_raw = strip_assignee_lines(&new_raw);
    let new_raw = match rhei_core::parse(&new_raw) {
        Ok(rhei) => {
            if let Some(metadata) = clear_runtime_task_metadata(rhei.metadata.as_ref()) {
                rewrite_frontmatter(&new_raw, &metadata)?
            } else {
                new_raw
            }
        }
        Err(_) => new_raw,
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to create temp file: {err}"
        ))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to write temp file: {err}"
    ))?;
    persist_locked(tmp, path).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to persist temp file: {err}"
    ))?;
    Ok(())
}

fn clear_runtime_metadata_in_file(
    path: &Path,
    workspace_index: bool,
    locked: &LockedPlanFile,
) -> MietteResult<()> {
    let raw = locked.read_to_string("failed to read plan file")?;
    let metadata = if workspace_index {
        rhei_core::parser::parse_workspace_index(&raw)
            .map_err(|err| {
                miette!(
                    help = plan_authoring_help(),
                    "failed to parse workspace index for metadata reset: {}", err.message
                )
            })?
            .metadata
    } else {
        rhei_core::parse(&raw)
            .map_err(|err| miette!(
                help = plan_authoring_help(),
                "failed to parse plan for metadata reset: {}", err.message
            ))?
            .metadata
    };

    let new_raw = if let Some(metadata) = clear_runtime_task_metadata(metadata.as_ref()) {
        rewrite_frontmatter(&raw, &metadata)?
    } else {
        raw
    };

    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to create temp file: {err}"
        ))?;
    tmp.write_all(new_raw.as_bytes()).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to write temp file: {err}"
    ))?;
    persist_locked(tmp, path).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to persist temp file: {err}"
    ))?;
    Ok(())
}

/// Remove `> **Result:** …` lines (and a single leading blank line when
/// present) inserted by `rhei complete`. Used during `rhei reset` so the
/// plan returns to a clean authored state.
fn strip_result_links(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("> **Result:**") {
            // Drop a single trailing blank line accumulated before the result
            // link so we don't leave a pair of blank lines behind.
            if matches!(result.last(), Some(last) if last.trim().is_empty()) {
                result.pop();
            }
            continue;
        }
        result.push((*line).to_string());
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Remove all runtime-owned `**Assignee:** …` lines during reset.
fn strip_assignee_lines(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        if line.starts_with("**Assignee:**") {
            continue;
        }
        result.push((*line).to_string());
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Rewrite each task's `**State:**` line back to the state that task was
/// authored in. A task with no recorded authored state never moved, so its
/// line is already right and is left exactly as written — that is what keeps a
/// pre-authored chain intact across a reset.
// §FS-rhei-reset.2.2 §FS-rhei-supervision.7
fn rewrite_states_to_authored(
    raw: &str,
    authored: &BTreeMap<String, String>,
) -> MietteResult<String> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut result = Vec::with_capacity(lines.len());
    // The authored state of the heading being read, and `None` once its
    // `**State:**` line has been passed. The outer `Option` is "am I inside a
    // heading", the inner one "does this task have an authored state".
    let mut expecting_state: Option<Option<&str>> = None;
    let mut state_lines = 0usize;

    let task_heading_re = regex::Regex::new(
        r#"^#{3,6}\s+[A-Za-z][A-Za-z0-9_-]*\s+([A-Za-z0-9][A-Za-z0-9_.\-]*):\s+"#,
    )
    .expect("task heading regex compiles");

    for line in &lines {
        if let Some(captures) = task_heading_re.captures(line) {
            if expecting_state.is_some() {
                return Err(miette!(
                    help = plan_authoring_help(),
                    "could not find **State:** line before the next task header"
                ));
            }
            let task_id = captures.get(1).expect("task id capture").as_str();
            expecting_state = Some(authored.get(task_id).map(String::as_str));
            result.push((*line).to_string());
            continue;
        }

        if let Some(authored_state) = expecting_state {
            if !line.starts_with("**State:**") {
                result.push((*line).to_string());
                continue;
            }
            // A task with no recorded history keeps its line verbatim: reset
            // moves a task only where its own ledger says it has been.
            match authored_state {
                Some(state) => result
                    .push(format!("**State:** {}", format_state_metadata_value(state))),
                None => result.push((*line).to_string()),
            }
            expecting_state = None;
            state_lines += 1;
            continue;
        }

        result.push((*line).to_string());
    }

    if expecting_state.is_some() {
        return Err(miette!(
            help = plan_authoring_help(),
            "could not find **State:** line at the end of the plan"
        ));
    }
    // Guards the "wrong file" mistake, not the "nothing moved" case: a plan
    // with no `**State:**` line at all is not a plan this command can reset.
    if state_lines == 0 {
        return Err(miette!(
            help = "this plan declares no task **State:** lines to reset. Check you passed the right plan.",
            "found no task state metadata to reset"
        ));
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Find a terminal (non-cancelled) state reachable in one transition.
///
/// Prefers exact `from` matches over wildcards. Cancellation is not considered
/// a completion target for `rhei complete`.
fn find_completion_state(
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> Option<String> {
    // Exact from-state matches first.
    for rule in machine.transitions() {
        if rule.from.0 == current_state {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && !machine.is_cancellation(&rule.to.0) {
                return Some(rule.to.0.clone());
            }
        }
    }

    // Fall back to wildcard transitions.
    for rule in machine.transitions() {
        if rule.from.0 == "*" && machine.transition_matches_source(rule, current_state) {
            let is_terminal =
                machine.states.get(&rule.to.0).map(|def| def.terminal).unwrap_or(false);
            if is_terminal && !machine.is_cancellation(&rule.to.0) {
                return Some(rule.to.0.clone());
            }
        }
    }

    None
}

fn is_successful_completion_state(state: &str, machine: &rhei_validator::StateMachine) -> bool {
    let normalized = normalized_state_name(state, machine);
    // §FS-rhei-states.1.4
    !machine.is_cancellation(&normalized) && is_terminal_state(&normalized, machine)
}

/// Every non-terminal descendant of `task`, rendered as `Task <prefix><id>
/// (<state>)` — the same shape [`format_open_descendants`] prints, so a user
/// hitting `rhei next --task`, `rhei transition`, and `rhei complete` back to
/// back reads one format instead of three.
///
/// A single [`rhei_validator::StateMachine`] is the whole truth for a subtree:
/// [`rhei_validator::MachineSet::for_task`] keys on the first id segment, so a
/// parent and every one of its descendants resolve to the same machine. That
/// is why the shared transition path can run this guard without threading a
/// `MachineSet` into `execute_transition_with_origin`.
// §FS-rhei-panta.6: `id_prefix` re-attaches the rhei qualifier when the tree
// was parsed from a task file, whose headings carry rhei-local ids.
// §DA-per-rhei-state-machines
fn non_terminal_descendants(
    task: &rhei_core::ast::Task,
    machine: &rhei_validator::StateMachine,
    id_prefix: &str,
) -> Vec<String> {
    fn recurse(
        task: &rhei_core::ast::Task,
        machine: &rhei_validator::StateMachine,
        id_prefix: &str,
        out: &mut Vec<String>,
    ) {
        for child in &task.children {
            if !is_terminal_state(child.state.as_str(), machine) {
                out.push(format!(
                    "Task {}{} ({})",
                    id_prefix,
                    child.id,
                    normalized_state_name(child.state.as_str(), machine)
                ));
            }
            recurse(child, machine, id_prefix, out);
        }
    }
    let mut out = Vec::new();
    recurse(task, machine, id_prefix, &mut out);
    out
}

fn title_case_kind(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len());
    let mut chars = kind.chars();
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            out.push(c);
        }
    }
    for c in chars {
        out.push(c);
    }
    out
}
