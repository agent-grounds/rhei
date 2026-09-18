// Central movements, legacy history and root access. §FS-rhei-complete.3.1 §FS-rhei-recover.4

/// Load a task's history under its qualified id, falling back to the
/// rhei-local id: pre-qualification runtime records are keyed locally, and
/// an upgrade must not orphan an executed plan's history. §AR-rhei-panta.2
fn load_task_history(
    workspace_root: &Path,
    task_id: &str,
    legacy_id: Option<String>,
) -> Vec<StateHistoryEntry> {
    let history = load_task_history_for_id(workspace_root, task_id);
    if !history.is_empty() {
        return history;
    }
    legacy_id.map(|id| load_task_history_for_id(workspace_root, &id)).unwrap_or_default()
}

fn load_task_history_for_id(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let central_history = load_central_transition_history(workspace_root, task_id);
    let journal_history = load_task_journal_history(workspace_root, task_id);
    if !central_history.is_empty() {
        if journal_history.is_empty() {
            return central_history;
        }
        // §FS-rhei-viz.4: state-path flattening loses exceptional reasons and self hops.
        if central_history.iter().any(|entry| entry.forced_reason.is_some()) {
            return merge_forced_history(journal_history, central_history);
        }
        return merge_history_sources(vec![journal_history, central_history]);
    }
    let path = workspace_root.join("runtime").join("results").join(format!("{task_id}.md"));
    let result_history =
        fs::read_to_string(path).map(|raw| parse_result_history(&raw)).unwrap_or_default();
    merge_history_sources(vec![journal_history, result_history])
}

/// Retain central movements verbatim while removing an overlapping legacy suffix. §FS-rhei-viz.4
fn merge_forced_history(
    mut journal: Vec<StateHistoryEntry>,
    central: Vec<StateHistoryEntry>,
) -> Vec<StateHistoryEntry> {
    let overlap = (1..=journal.len().min(central.len()))
        .rev()
        .find(|count| {
            journal[journal.len() - count..]
                .iter()
                .zip(&central[..*count])
                .all(|(left, right)| left.from == right.from && left.to == right.to)
        })
        .unwrap_or(0);
    journal.truncate(journal.len() - overlap);
    journal.extend(central);
    journal
}

fn load_central_transition_history(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let path = workspace_root.join("runtime").join("state-transitions.log");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_central_transition_history(&raw, task_id)
}

fn parse_central_transition_history(raw: &str, task_id: &str) -> Vec<StateHistoryEntry> {
    // §FS-rhei-viz.4: preserve the reason without counting the metadata as a move.
    rhei_core::transition_history::parse(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|movement| movement.task_id == task_id)
        .map(|movement| StateHistoryEntry {
            from: movement.from,
            to: movement.to,
            forced_reason: movement.audit.map(|audit| audit.reason),
        })
        .collect()
}

fn parse_result_history(raw: &str) -> Vec<StateHistoryEntry> {
    raw.lines()
        .filter_map(|line| {
            let heading = line.trim().strip_prefix("## ")?;
            let (from, to) = heading.split_once('\u{2192}').or_else(|| heading.split_once("->"))?;
            let from = from.trim();
            let to = to.trim();
            if from.is_empty() || to.is_empty() {
                return None;
            }
            Some(StateHistoryEntry {
                from: from.to_string(),
                to: to.to_string(),
                forced_reason: None,
            })
        })
        .collect()
}

fn load_task_journal_history(workspace_root: &Path, task_id: &str) -> Vec<StateHistoryEntry> {
    let path = workspace_root.join("runtime").join("transitions.log");
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let states = parse_journal_state_path(&raw, task_id);
    states
        .windows(2)
        .filter_map(|pair| match pair {
            [from, to] if from != to => {
                Some(StateHistoryEntry { from: from.clone(), to: to.clone(), forced_reason: None })
            }
            _ => None,
        })
        .collect()
}

fn merge_history_sources(sources: Vec<Vec<StateHistoryEntry>>) -> Vec<StateHistoryEntry> {
    let mut states = Vec::new();
    for source in sources {
        let source_states = history_to_states(&source);
        append_state_path(&mut states, &source_states);
    }
    states_to_history(states)
}

fn history_to_states(history: &[StateHistoryEntry]) -> Vec<String> {
    let mut states = Vec::new();
    for entry in history {
        push_history_state(&mut states, &entry.from);
        push_history_state(&mut states, &entry.to);
    }
    states
}

fn append_state_path(states: &mut Vec<String>, next: &[String]) {
    if next.is_empty() {
        return;
    }
    let max_overlap = states.len().min(next.len());
    let overlap = (1..=max_overlap)
        .rev()
        .find(|&len| states[states.len() - len..] == next[..len])
        .unwrap_or(0);
    for state in &next[overlap..] {
        push_history_state(states, state);
    }
}

fn states_to_history(states: Vec<String>) -> Vec<StateHistoryEntry> {
    states
        .windows(2)
        .filter_map(|pair| match pair {
            [from, to] if from != to => {
                Some(StateHistoryEntry { from: from.clone(), to: to.clone(), forced_reason: None })
            }
            _ => None,
        })
        .collect()
}

fn parse_journal_state_path(raw: &str, task_id: &str) -> Vec<String> {
    let mut states = Vec::new();
    for line in raw.lines() {
        let parts = line.split("  ").collect::<Vec<_>>();
        if parts.len() < 3 || parts[1] != task_id {
            continue;
        }
        let movement = parts[2];
        if let Some(state) =
            movement.strip_prefix("start@").or_else(|| movement.strip_prefix("end@"))
        {
            push_history_state(&mut states, state);
        } else if let Some((from, to)) = movement.split_once('\u{2192}') {
            push_history_state(&mut states, from.trim());
            push_history_state(&mut states, to.trim());
        } else if let Some((from, to)) = movement.split_once("->") {
            push_history_state(&mut states, from.trim());
            push_history_state(&mut states, to.trim());
        }
    }
    states
}

fn push_history_state(states: &mut Vec<String>, state: &str) {
    let state = state.trim();
    if state.is_empty() || states.last().is_some_and(|last| last == state) {
        return;
    }
    states.push(state.to_string());
}

/// Direct viz APIs refuse pending roots and corrupt history before returning data. §FS-rhei-recover.4
fn history_guards(
    root: &Path,
    members: &HashMap<String, PathBuf>,
) -> std::io::Result<Vec<rhei_core::root_access::RootAccessGuard>> {
    let roots = std::iter::once(root.to_path_buf())
        .chain(members.values().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let guards = rhei_core::root_access::shared_roots(roots.iter().cloned())?;
    for root in roots {
        match fs::read_to_string(root.join("runtime/state-transitions.log")) {
            Ok(raw) => {
                rhei_core::transition_history::parse(&raw)?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }
    }
    Ok(guards)
}
