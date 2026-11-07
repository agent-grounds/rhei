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
/// in; a task absent from it never moved and keeps the line it has. A file whose
/// text does not change is not written. Returns whether it was.
/// §FS-rhei-reset.2.2 §FS-rhei-reset.2
fn reset_plan_file_states(
    path: &Path,
    authored: &BTreeMap<String, String>,
    locked: &LockedPlanFile,
) -> MietteResult<bool> {
    let raw = locked.read_to_string("failed to read plan file")?;
    let new_raw = reset_file_text(&raw, authored, plan_file_kind(path))?;
    if new_raw == raw {
        return Ok(false);
    }
    write_file_atomic_locked(path, &new_raw, Some(locked))?;
    Ok(true)
}

/// The text a reset writes over one plan file's `raw`: each task's `**State:**` line
/// set to where `authored` sends it, every claim and result link gone. Metadata
/// is reset separately in its owning document. The plan a reset
/// is checked against is built from it too. §FS-rhei-reset.2.2 §FS-rhei-reset.2.3
fn reset_file_text(
    raw: &str,
    authored: &BTreeMap<String, String>,
    kind: PlanFileKind,
) -> MietteResult<String> {
    let new_raw = rewrite_states_to_authored(raw, authored, kind)?;
    let new_raw = strip_result_links(&new_raw, kind);
    Ok(strip_assignee_lines(&new_raw, kind))
}

/// Group selected live tickets, descendants included, by their writable metadata
/// document and its own task keys. Shared manifests are written once, without
/// touching out-of-scope entries or authored task-file blocks. §FS-rhei-reset.2
fn reset_metadata_targets(
    loaded: &LoadedPlan,
    input: &Path,
    scope: &RheiScope,
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut tasks = Vec::new();
    collect_plan_tasks(&loaded.rhei.tasks, &mut tasks);
    let mut targets: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for task in tasks {
        let id = task.id.to_string();
        if task_in_rhei_scope(scope, &id) {
            let route = loaded.task_route(&id, input);
            targets.entry(route.metadata_file).or_default().insert(route.metadata_id);
        }
    }
    targets
}

/// The metadata reset writes, also used by the validation projection. Read each
/// document through the existing form dispatch and delete only selected tickets'
/// register-cleared keys, pruning containers emptied by them. §FS-rhei-reset.2
fn reset_metadata_text(path: &Path, raw: &str, task_ids: &BTreeSet<String>) -> MietteResult<String> {
    let metadata = rhei_core::metadata::parse_metadata_file(path, raw).map_err(|err| {
        miette!(
            help = plan_authoring_help(),
            "failed to parse metadata document for reset: {}", err.error.message
        )
    })?;
    match clear_runtime_task_metadata_for(metadata.as_ref(), |id| task_ids.contains(id)) {
        Some(cleared) if Some(&cleared) != metadata.as_ref() => rewrite_frontmatter(raw, &cleared),
        _ => Ok(raw.to_string()),
    }
}

/// Persist the scoped cleanup used by validation and the writability check while
/// the complete reset writer-lock stack remains held. Unchanged files are not
/// written. Returns whether it was. §FS-rhei-reset.2
fn clear_runtime_metadata_in_file(
    path: &Path,
    task_ids: &BTreeSet<String>,
    locked: &LockedPlanFile,
) -> MietteResult<bool> {
    let raw = locked.read_to_string("failed to read plan file")?;
    let new_raw = reset_metadata_text(path, &raw, task_ids)?;
    if new_raw == raw {
        return Ok(false);
    }
    write_file_atomic_locked(path, &new_raw, Some(locked))?;
    Ok(true)
}

/// Remove `> **Result:** …` lines (and a single leading blank line when
/// present) inserted by `rhei complete`. Used during `rhei reset` so the
/// plan returns to a clean authored state.
fn strip_result_links(raw: &str, kind: PlanFileKind) -> String {
    strip_result_links_under(raw, kind, |_| true)
}

/// [`strip_result_links`], for the lines under a task heading whose file-local id
/// `under` accepts; `None` is the text before the first heading. A fence's own lines
/// are never dropped: the line above one read as structure is outside any fence, since
/// a fence's last line is the run that closes it. §FS-rhei-reset.2.3 §FS-rhei-reset.2.4
fn strip_result_links_under(
    raw: &str,
    kind: PlanFileKind,
    under: impl Fn(Option<&str>) -> bool,
) -> String {
    let lines = plan_file_lines(raw, kind);
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut heading = None;

    for (line, structure) in lines {
        if structure {
            if let Some(task_id) = task_heading_id(line) {
                heading = Some(task_id);
            } else if is_result_link(line) && under(heading) {
                // Drop a single trailing blank line accumulated before the result
                // link so we don't leave a pair of blank lines behind.
                if matches!(result.last(), Some(last) if last.trim().is_empty()) {
                    result.pop();
                }
                continue;
            }
        }
        result.push(line.to_string());
    }

    let mut output = result.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Remove all runtime-owned `**Assignee:** …` lines during reset. A quoted one is
/// authored text. §FS-rhei-reset.2.4
fn strip_assignee_lines(raw: &str, kind: PlanFileKind) -> String {
    let lines = plan_file_lines(raw, kind);
    let mut result: Vec<String> = Vec::with_capacity(lines.len());

    for (line, structure) in lines {
        if structure && line.starts_with("**Assignee:**") {
            continue;
        }
        result.push(line.to_string());
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
// §FS-rhei-reset.2.2 §FS-rhei-reset.2.4 §FS-rhei-supervision.7
fn rewrite_states_to_authored(
    raw: &str,
    authored: &BTreeMap<String, String>,
    kind: PlanFileKind,
) -> MietteResult<String> {
    let lines = plan_file_lines(raw, kind);
    let mut result = Vec::with_capacity(lines.len());
    // The authored state of the heading being read, and `None` once its
    // `**State:**` line has been passed. The outer `Option` is "am I inside a
    // heading", the inner one "does this task have an authored state".
    let mut expecting_state: Option<Option<&str>> = None;
    let mut state_lines = 0usize;

    for (line, structure) in lines {
        if !structure {
            result.push(line.to_string());
            continue;
        }

        if let Some(task_id) = task_heading_id(line) {
            if expecting_state.is_some() {
                return Err(miette!(
                    help = plan_authoring_help(),
                    "could not find **State:** line before the next task header"
                ));
            }
            expecting_state = Some(authored.get(task_id).map(String::as_str));
            result.push(line.to_string());
            continue;
        }

        if let Some(authored_state) = expecting_state {
            if !line.starts_with("**State:**") {
                result.push(line.to_string());
                continue;
            }
            // A task with no recorded history keeps its line verbatim: reset
            // moves a task only where its own ledger says it has been.
            match authored_state {
                Some(state) => result
                    .push(format!("**State:** {}", format_state_metadata_value(state))),
                None => result.push(line.to_string()),
            }
            expecting_state = None;
            state_lines += 1;
            continue;
        }

        result.push(line.to_string());
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

/// The file-local id of the task `line` is the heading of, if it is one: the
/// headings a reset rewrites under. §FS-rhei-reset.2.2
fn task_heading_id(line: &str) -> Option<&str> {
    static HEADING: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let heading = HEADING.get_or_init(|| {
        regex::Regex::new(r#"^#{3,6}\s+[A-Za-z][A-Za-z0-9_-]*\s+([A-Za-z0-9][A-Za-z0-9_.\-]*):\s+"#)
            .expect("task heading regex compiles")
    });
    heading.captures(line).and_then(|captures| captures.get(1)).map(|id| id.as_str())
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
