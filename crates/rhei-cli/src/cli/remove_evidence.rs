// `rhei remove`'s read-only half: whether a ticket may go, and what of it is
// disposable residue rather than history.
//
// Its own part because every check here reads and none writes, so the dry run,
// the first pass and the re-check under the locks run exactly the same code.

// §FS-rhei-remove.2 §FS-rhei-remove.3

/// What one read of the project found about a ticket. §FS-rhei-remove.2
struct RemovalAssessment {
    /// Dependents, children and node-kind refusals, one line each.
    blockers: Vec<String>,
    /// History: `(what, where)`, one line each. §FS-rhei-remove.3.4
    history: Vec<(String, String)>,
    /// Empty, ticket-owned paths removal cleans. §FS-rhei-remove.4.2
    residue: Vec<PathBuf>,
}

impl RemovalAssessment {
    fn refused(&self) -> bool {
        !self.blockers.is_empty() || !self.history.is_empty()
    }

    /// Every blocker in one diagnostic, the first on the error line.
    /// §FS-rhei-remove.3 §FS-rhei-errors.6
    fn refusal(&self, id: &str) -> Report {
        let mut lines: Vec<String> = self.blockers.clone();
        if !self.history.is_empty() {
            lines.push("it has history".to_string());
        }
        let mut message = format!("{id} cannot be removed: {}", lines[0]);
        for line in &lines[1..] {
            message.push_str(&format!("\n  {line}"));
        }
        for (what, place) in &self.history {
            message.push_str(&format!("\n  {what}: {place}"));
        }
        let help = if self.history.is_empty() {
            "change or remove what names it first, then remove it.".to_string()
        } else {
            format!(
                "instead: move it to its machine's cancellation state with \
                 `rhei transition {id} <state>`"
            )
        };
        miette!(help = help, "{message}")
    }
}

/// Read every source of evidence about `id` at its owning and the project
/// execution root, without writing anything. §FS-rhei-remove.2
fn assess_removal(
    loaded: &LoadedPlan,
    input: &Path,
    id: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<RemovalAssessment> {
    let task = find_task_by_id_str(&loaded.rhei.tasks, id)
        .ok_or_else(|| miette!(help = task_id_help(), "task '{id}' not found"))?;
    let mut assessment =
        RemovalAssessment { blockers: Vec::new(), history: Vec::new(), residue: Vec::new() };
    // §FS-rhei-remove.3.2
    if !task.children.is_empty() {
        let children: Vec<String> = task.children.iter().map(|c| c.id.to_string()).collect();
        assessment.blockers.push(format!(
            "it has children ({}); remove them first",
            children.join(", ")
        ));
    }
    collect_dependents(&loaded.rhei.tasks, id, &mut assessment.blockers);
    // A terminal state is a recorded outcome whatever produced it. §FS-rhei-remove.2
    if is_terminal_state(&task.state, machine) {
        assessment.history.push(("state".into(), format!("{} is terminal", task.state)));
    }
    if let Some(assignee) = &task.assignee {
        assessment.history.push(("claim".into(), format!("**Assignee:** {assignee}")));
    }
    if task.content.contains("> **Result:**") {
        assessment.history.push(("result".into(), "a > **Result:** link in its body".into()));
    }
    metadata_evidence(loaded, id, &mut assessment.history);

    let route = loaded.task_route(id, input);
    let project_root = execution_workspace_root(input);
    let mut roots = vec![route.execution_root.clone()];
    if canonical_or_self(&route.execution_root) != canonical_or_self(&project_root) {
        roots.push(project_root.clone());
    }
    let others = other_ticket_ids(loaded, id);
    for root in &roots {
        ledger_evidence(root, id, &mut assessment.history);
        runtime_evidence(root, id, machine, &others, &mut assessment)?;
    }
    note_and_event_evidence(&project_root, id, &mut assessment.history);
    budget_evidence(loaded, &project_root, id, &mut assessment.history);
    assessment.residue.sort();
    assessment.residue.dedup();
    Ok(assessment)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    rhei_core::platform::canonical_path(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Every ticket that names `id` in a field the plan language resolves.
/// §FS-rhei-remove.3.1
fn collect_dependents(tasks: &[rhei_core::ast::Task], id: &str, out: &mut Vec<String>) {
    for task in tasks {
        let mut fields = Vec::new();
        if task.prior.iter().any(|prior| prior.to_string() == id) {
            fields.push("**Prior:**");
        }
        if task.consumes.iter().any(|consumed| consumed.task.to_string() == id) {
            fields.push("**Consumes:**");
        }
        let excluded = task.excludes.iter().any(|exclusion| {
            matches!(exclusion, rhei_core::ast::TaskExclusion::Export(e) if e.task.to_string() == id)
        });
        if excluded {
            fields.push("**Excludes:**");
        }
        for field in fields {
            out.push(format!("{} names it in {field}", task.id));
        }
        collect_dependents(&task.children, id, out);
    }
}

/// Counted visits, supervision and parked waits are acts; a budget identity
/// alone is not. §FS-rhei-remove.2
fn metadata_evidence(loaded: &LoadedPlan, id: &str, out: &mut Vec<(String, String)>) {
    use rhei_core::metadata::{
        POLL_NEXT_ATTEMPT_AT_KEY, PROVIDER_LIMITS_KEY, STATE_VISITS_KEY, SUPERVISION_KEY,
    };
    let Some(entry) = loaded
        .rhei
        .metadata
        .as_ref()
        .and_then(|root| root.get("metadata"))
        .and_then(YamlValue::as_mapping)
        .and_then(|section| section.get("tasks"))
        .and_then(YamlValue::as_mapping)
        .and_then(|tasks| tasks.get(id))
        .and_then(YamlValue::as_mapping)
    else {
        return;
    };
    for key in [STATE_VISITS_KEY, SUPERVISION_KEY, PROVIDER_LIMITS_KEY, POLL_NEXT_ATTEMPT_AT_KEY] {
        if entry.get(key).is_some() {
            out.push(("metadata".into(), format!("metadata.tasks.{id}.{key}")));
        }
    }
}

/// Every other ticket id the project knows, live or retired: what makes a
/// loose prefix match ambiguous. §FS-rhei-remove.4.2
fn other_ticket_ids(loaded: &LoadedPlan, id: &str) -> Vec<String> {
    let mut ids = Vec::new();
    fn walk(tasks: &[rhei_core::ast::Task], ids: &mut Vec<String>) {
        for task in tasks {
            ids.push(task.id.to_string());
            walk(&task.children, ids);
        }
    }
    walk(&loaded.rhei.tasks, &mut ids);
    if let Ok(retired) = rhei_core::retired::retired_tickets(loaded.rhei.metadata.as_ref()) {
        ids.extend(retired.into_keys());
    }
    ids.retain(|other| other != id);
    ids
}

/// Any ledger row, ordinary or forced, for the ticket. §FS-rhei-remove.2
fn ledger_evidence(root: &Path, id: &str, out: &mut Vec<(String, String)>) {
    let ledger = root.join("runtime").join("state-transitions.log");
    let raw = match fs::read_to_string(&ledger) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            out.push(("unreadable".into(), format!("{}: {err}", display_path(&ledger))));
            return;
        }
    };
    match rhei_core::transition_history::parse(&raw) {
        Ok(movements) => {
            for movement in movements.iter().filter(|movement| movement.task_id == id) {
                out.push((
                    "transition".into(),
                    format!(
                        "{} records {id} {} -> {}",
                        relative_to(root, &ledger),
                        movement.from,
                        movement.to
                    ),
                ));
            }
        }
        Err(err) => out.push(("unreadable".into(), format!("{}: {err}", display_path(&ledger)))),
    }
}

/// Task notes and invocation events naming the ticket. §FS-rhei-remove.2
fn note_and_event_evidence(project_root: &Path, id: &str, out: &mut Vec<(String, String)>) {
    let notes = project_root.join("runtime").join("notes.md");
    match fs::read_to_string(&notes) {
        Ok(raw) => {
            let named = parse_note_store(&raw).iter().any(|record| match record {
                NoteRecord::Left { task, .. } => task == id,
                NoteRecord::Restates { task, target } | NoteRecord::Strikes { task, target } => {
                    task == id || target == id
                }
            });
            if named {
                out.push(("note".into(), relative_to(project_root, &notes)));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => out.push(("unreadable".into(), format!("{}: {err}", display_path(&notes)))),
    }
    let events = project_root.join("runtime").join("events.jsonl");
    match fs::read_to_string(&events) {
        Ok(raw) => {
            let quoted = serde_json::to_string(id).unwrap_or_default();
            if raw.lines().any(|line| line.contains(&quoted)) {
                out.push(("event".into(), relative_to(project_root, &events)));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => out.push(("unreadable".into(), format!("{}: {err}", display_path(&events)))),
    }
}

/// A reservation, start or travel receipt for the ticket; an identity binding
/// alone is not evidence. §FS-rhei-remove.2 §FS-rhei-budgets.5.2
fn budget_evidence(
    loaded: &LoadedPlan,
    project_root: &Path,
    id: &str,
    out: &mut Vec<(String, String)>,
) {
    let account_root = budget_project_root(project_root);
    let account = match rhei_core::budget::Account::locate(&account_root) {
        Ok(Some(account)) => account,
        Ok(None) => return,
        Err(err) => {
            out.push(("unreadable".into(), format!("budget account: {err}")));
            return;
        }
    };
    let journal = match account.open(false) {
        Ok(journal) => journal,
        Err(err) => {
            out.push(("unreadable".into(), format!("budget account: {err}")));
            return;
        }
    };
    let identity = budget_ticket_id(loaded, id).map(|uuid| account.ticket_identity(&uuid));
    for receipt in journal.receipts().iter().filter(|receipt| receipt.kind != "identity") {
        if json_names_ticket(&receipt.payload, id, identity.as_deref()) {
            out.push((
                "budget".into(),
                format!("{} receipt #{} in the project account", receipt.kind, receipt.sequence),
            ));
        }
    }
}

/// The ticket's `budgetTicketId`, which removal keeps in its retirement.
/// §FS-rhei-remove.5.1
fn budget_ticket_id(loaded: &LoadedPlan, id: &str) -> Option<String> {
    loaded
        .rhei
        .metadata
        .as_ref()?
        .get("metadata")?
        .get("tasks")?
        .get(id)?
        .get(rhei_core::metadata::BUDGET_TICKET_ID_KEY)?
        .as_str()
        .map(str::to_string)
}

fn json_names_ticket(value: &serde_json::Value, id: &str, identity: Option<&str>) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "display_id" && value.as_str() == Some(id))
                || (key == "ticket_identity" && identity.is_some() && value.as_str() == identity)
                || json_names_ticket(value, id, identity)
        }),
        serde_json::Value::Array(items) => {
            items.iter().any(|item| json_names_ticket(item, id, identity))
        }
        serde_json::Value::String(text) => identity == Some(text.as_str()),
        _ => false,
    }
}

fn relative_to(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.to_string_lossy().replace('\\', "/")
}
