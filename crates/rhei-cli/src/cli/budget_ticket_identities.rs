// Assign appended tasks under the same lifetime ledger, never a new allowance.
// §FS-rhei-budgets.2.3 §AR-neural-admission.3

/// Retain source ownership through the caller's mutation. §AR-neural-admission.3
struct BudgetIdentityTransaction {
    _locks: BTreeMap<PathBuf, LockedPlanFile>,
    journal: rhei_core::budget::Journal,
}

fn budget_ensure_ticket_identities(
    project: &BudgetProject,
) -> MietteResult<BudgetIdentityTransaction> {
    let id = project.required_identity()?;
    let mut journal =
        rhei_core::budget::Journal::open(&project.root, &id, true).map_err(budget_error)?;
    let loaded = load_plan(&project.input)?;
    let tasks = budget_task_list(&loaded.rhei.tasks);
    let mut metadata_paths = BTreeSet::new();
    metadata_paths.insert(project.metadata_file.clone());
    let mut source_paths = BTreeSet::new();
    for task in &tasks {
        let route = loaded.task_route(&task.id.to_string(), &project.input);
        metadata_paths.insert(route.metadata_file);
        source_paths.insert(route.task_file);
    }
    for path in journal.identity_sources().map_err(budget_error)? {
        if path
            .try_exists()
            .map_err(|e| file_io_report(&path, "cannot inspect identity source", e))?
        {
            source_paths.insert(path);
        }
    }
    // Alias paths must share a single sidecar acquisition. §AR-neural-admission.3
    let canonical = |path: PathBuf| {
        rhei_core::platform::canonical_path(&path)
            .map_err(|e| file_io_report(&path, "cannot resolve identity document", e))
    };
    let metadata_paths =
        metadata_paths.into_iter().map(canonical).collect::<MietteResult<BTreeSet<_>>>()?;
    let source_paths =
        source_paths.into_iter().map(canonical).collect::<MietteResult<BTreeSet<_>>>()?;
    let locks = metadata_paths
        .iter()
        .chain(source_paths.difference(&metadata_paths))
        .map(|path| LockedPlanFile::open(path).map(|lock| (path.clone(), lock)))
        .collect::<MietteResult<BTreeMap<_, _>>>()?;
    journal.validate_identity_sources().map_err(budget_error)?;
    let mut documents = BTreeMap::new();
    for path in &metadata_paths {
        let raw = locks[path].read_to_string("cannot read ticket identities")?;
        documents.insert(path.clone(), budget_document_metadata(&raw, path)?);
    }
    let owner = canonical(project.metadata_file.clone())?;
    if documents[&owner].get(yaml_key("budgetProjectId")).and_then(YamlValue::as_str)
        != Some(id.as_str())
    {
        return Err(miette!(help = "another process is initializing this project; let it finish and run the command again", "project identity changed while assigning ticket identities"));
    }
    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();
    for task in tasks {
        let display = task.id.to_string();
        let route = loaded.task_route(&display, &project.input);
        let path = canonical(route.metadata_file)?;
        let metadata = documents.get_mut(&path).expect("locked metadata was read");
        let section = ensure_mapping(metadata, yaml_key("metadata"));
        let entries = ensure_mapping(section, yaml_key("tasks"));
        let entry = ensure_mapping(entries, task_id_yaml_key(&parse_task_id(&route.metadata_id)));
        let binding = journal.identities().iter().find(|(_, row)| row["display_id"] == display);
        let assigned =
            binding.map(|(ticket, _)| ticket.rsplit(':').next().expect("ticket UUID").to_string());
        let pending = !entry.contains_key(yaml_key("budgetTicketId"));
        let uuid = match entry.get(yaml_key("budgetTicketId")) {
            Some(value) => {
                let value =
                    value.as_str().ok_or_else(|| miette!(help = "restore the ticket's frontmatter from version control; a budget identity is never re-minted", "budgetTicketId is not a UUID"))?;
                validate_budget_uuid(value)?;
                if assigned.as_deref().is_some_and(|old| old != value) {
                    return Err(miette!(help = "restore the ticket's recorded budgetTicketId; lifetime travel is keyed by it", "ticket {display} changed its persistent budget identity"));
                }
                value.to_string()
            }
            None => {
                if binding.is_some_and(|(_, row)| row["status"] != "pending") {
                    return Err(miette!(help = "restore the ticket's budgetTicketId from version control; a fresh one would reset its travel", "ticket {display} lost its persistent budget identity; restore its metadata"));
                }
                assigned.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
            }
        };
        if !seen.insert(uuid.clone()) {
            return Err(miette!(help = "two plan files claim one budgetTicketId; give the copy its own identity or remove it", "ticket identity has conflicting live documents"));
        }
        entry.insert(yaml_key("budgetTicketId"), yaml_key(&uuid));
        bindings.push((
            format!("ticket:{id}:{uuid}"),
            display,
            canonical(route.task_file)?,
            pending,
        ));
    }
    let audit = budget_audit("assign persistent ticket identities before admission")?;
    // Record the chosen UUID before metadata; a failed write reuses it on the
    // next attempt. Binding an identity debits no unit. §FS-rhei-budgets.2.3
    for (ticket, display, source, pending) in &bindings {
        journal.bind_ticket(ticket, display, source, *pending, &audit).map_err(budget_error)?;
    }
    for (path, metadata) in documents {
        let raw = locks[&path].read_to_string("cannot read ticket identities")?;
        if budget_document_metadata(&raw, &path)? != metadata {
            write_budget_metadata(&path, &locks[&path], &metadata)?;
        }
    }
    for (ticket, _, _, _) in bindings {
        journal.finish_ticket_binding(&ticket, &audit).map_err(budget_error)?;
    }
    Ok(BudgetIdentityTransaction { _locks: locks, journal })
}
