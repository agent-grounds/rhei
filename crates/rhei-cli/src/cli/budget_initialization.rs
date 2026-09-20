// A durable intent makes metadata installation restartable without minting
// another financial identity. §FS-rhei-budgets.8

#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetInitialization {
    schema: String,
    project_uuid: String,
    root: PathBuf,
    allowance: rhei_core::budget::Allowance,
    audit: rhei_core::budget::Audit,
    #[serde(default)]
    history_required: bool,
    #[serde(default)]
    history_cutoff: String,
    #[serde(default)]
    history: Option<BudgetInitializationHistory>,
    documents: BTreeMap<PathBuf, BudgetIdentityDocument>,
}

#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetInitializationHistory {
    evidence: Vec<u8>,
    signature: Vec<u8>,
}

#[derive(serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetIdentityDocument {
    before: String,
    after: String,
}

fn budget_prepare_initialization(
    project: &BudgetProject,
    loaded: &LoadedPlan,
    allowance: rhei_core::budget::Allowance,
    audit: rhei_core::budget::Audit,
    history_required: bool,
) -> MietteResult<BudgetInitialization> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut paths = BTreeMap::<PathBuf, Vec<String>>::new();
    paths.entry(project.metadata_file.clone()).or_default();
    for task in budget_task_list(&loaded.rhei.tasks) {
        let route = loaded.task_route(&task.id.to_string(), &project.input);
        paths.entry(route.metadata_file).or_default().push(route.metadata_id);
    }
    let locks = paths
        .keys()
        .map(|path| LockedPlanFile::open(path).map(|lock| (path, lock)))
        .collect::<MietteResult<BTreeMap<_, _>>>()?;
    let mut documents = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for (path, tasks) in &paths {
        let before = locks[path].read_to_string("cannot read identity document")?;
        let mut metadata = budget_document_metadata(&before, path)?;
        if *path == project.metadata_file {
            if metadata.contains_key(yaml_key("budgetProjectId")) {
                return Err(miette!(help = "another process initialized this project first; read it with `rhei budget show`", "project identity changed during initialization"));
            }
            metadata.insert(yaml_key("budgetProjectId"), yaml_key(&id));
        }
        for local in tasks {
            let section = ensure_mapping(&mut metadata, yaml_key("metadata"));
            let entries = ensure_mapping(section, yaml_key("tasks"));
            let entry = ensure_mapping(entries, task_id_yaml_key(&parse_task_id(local)));
            let ticket = if let Some(value) = entry.get(yaml_key("budgetTicketId")) {
                let value =
                    value.as_str().ok_or_else(|| miette!(help = "restore the ticket's frontmatter from version control; a budget identity is never re-minted", "budgetTicketId is not a UUID"))?;
                validate_budget_uuid(value)?;
                value.to_string()
            } else {
                let ticket = uuid::Uuid::new_v4().to_string();
                entry.insert(yaml_key("budgetTicketId"), yaml_key(&ticket));
                ticket
            };
            if !identities.insert(ticket) {
                return Err(miette!(help = "two plan files claim one budgetTicketId; give the copy its own identity or remove it", "ticket identity has conflicting live documents"));
            }
        }
        let after = rewrite_frontmatter(&before, &metadata)?;
        documents.insert(path.clone(), BudgetIdentityDocument { before, after });
    }
    Ok(BudgetInitialization {
        schema: "rhei.budget.initialization.v1".into(),
        project_uuid: id,
        root: project.root.clone(),
        allowance,
        history_required,
        history_cutoff: audit.written_at.clone(),
        history: None,
        audit,
        documents,
    })
}

fn budget_save_initialization(path: &Path, intent: &BudgetInitialization) -> MietteResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| miette!(help = "this is an engine invariant, not an operator error; report it with the command you ran", "initialization path has no parent"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot stage the initialization intent: {e}"))?;
    serde_json::to_writer(&mut tmp, intent)
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot write the initialization intent: {e}"))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot sync the initialization intent: {e}"))?;
    tmp.persist_noclobber(path).map_err(|e| miette!(help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again", "cannot save initialization intent: {e}"))?;
    budget_sync_directory(parent)
}

fn budget_resume_initialization(
    project: &BudgetProject,
    path: &Path,
    allowance: &rhei_core::budget::Allowance,
    history: Option<&Path>,
) -> MietteResult<()> {
    let mut intent: BudgetInitialization = serde_json::from_slice(
        &fs::read(path)
            .map_err(|e| file_io_report(path, "cannot read initialization intent", e))?,
    )
    .map_err(|e| miette!(help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again", "untrustworthy initialization intent: {e}"))?;
    if intent.schema != "rhei.budget.initialization.v1"
        || intent.root != project.root
        || intent.allowance != *allowance
        || !intent.documents.contains_key(&project.metadata_file)
        || intent.documents.keys().any(|p| !p.starts_with(&project.root))
    {
        return Err(miette!(
            help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again",
            "unfinished initialization must resume its original project, allowance and history"
        ));
    }
    validate_budget_uuid(&intent.project_uuid)?;
    if let Some(history) = history {
        budget_attach_history(&mut intent, history)?;
        budget_replace_initialization(path, &intent)?;
    }
    if intent.history_required && intent.history.is_none() {
        return Err(miette!(
            help = "supply the signed provider export with --history <PATH>; unknown historical exposure is never assumed to be zero",
            "historical exposure requires complete provider evidence for panta:{} through {}; rerun this exact init with --history <PATH>",
            intent.project_uuid,
            intent.history_cutoff
        ));
    }
    budget_install_initialization(project, path, intent)
}

fn budget_attach_history(
    intent: &mut BudgetInitialization,
    path: &Path,
) -> MietteResult<()> {
    let evidence = fs::read(path)
        .map_err(|e| file_io_report(path, "cannot read authoritative history", e))?;
    let signature = budget_evidence_signature(path)?;
    rhei_core::budget::Registry::verify_history(
        &evidence,
        &signature,
        &format!("panta:{}", intent.project_uuid),
        &intent.allowance.spend.currency,
        &intent.history_cutoff,
    )
    .map_err(budget_error)?;
    let supplied = BudgetInitializationHistory {
        evidence,
        signature: signature.to_vec(),
    };
    if let Some(existing) = &intent.history {
        if existing.evidence != supplied.evidence || existing.signature != supplied.signature {
            return Err(miette!(
                help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again",
                "unfinished initialization is bound to different authoritative history"
            ));
        }
    } else {
        intent.history = Some(supplied);
    }
    Ok(())
}

fn budget_replace_initialization(
    path: &Path,
    intent: &BudgetInitialization,
) -> MietteResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| miette!(help = "this is an engine invariant, not an operator error; report it with the command you ran", "initialization path has no parent"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot stage the initialization intent: {e}"))?;
    serde_json::to_writer(&mut tmp, intent)
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot write the initialization intent: {e}"))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot sync the initialization intent: {e}"))?;
    tmp.persist(path)
        .map_err(|e| miette!(help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again", "cannot update initialization intent: {e}"))?;
    budget_sync_directory(parent)
}

fn budget_install_initialization(
    project: &BudgetProject,
    path: &Path,
    intent: BudgetInitialization,
) -> MietteResult<()> {
    let verified_history = match &intent.history {
        Some(history) => {
            let signature: [u8; 64] = history.signature.as_slice().try_into().map_err(|_| {
                miette!(help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again", "untrustworthy initialization intent: invalid history signature length")
            })?;
            Some(
                rhei_core::budget::Registry::verify_history(
                    &history.evidence,
                    &signature,
                    &format!("panta:{}", intent.project_uuid),
                    &intent.allowance.spend.currency,
                    &intent.history_cutoff,
                )
                .map_err(budget_error)?,
            )
        }
        None if intent.history_required => {
            return Err(miette!(help = "supply the signed provider export with --history <PATH>; unknown historical exposure is never assumed to be zero", "initialization requires authoritative lifetime history"));
        }
        None => None,
    };
    let locks = intent
        .documents
        .keys()
        .map(|path| LockedPlanFile::open(path).map(|lock| (path, lock)))
        .collect::<MietteResult<BTreeMap<_, _>>>()?;
    let mut staged = Vec::new();
    // Validate *all* documents and stage every write before installing any.
    // No allowance is active while this intent is incomplete. §FS-rhei-budgets.8
    for (document, bytes) in &intent.documents {
        let current = locks[document].read_to_string("cannot resume identity document")?;
        if current != bytes.before && current != bytes.after {
            return Err(miette!(help = "finish or remove `.agent-grounds/rhei/budgets/initialization.json`, then run `rhei budget init` again", "identity document changed during initialization: {}; restore the recorded document before retrying", document.display()));
        }
        budget_document_metadata(&bytes.after, document)?;
        let mut tmp = tempfile::NamedTempFile::new_in(document.parent().unwrap_or(Path::new(".")))
            .map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot stage identity: {e}"))?;
        tmp.write_all(bytes.after.as_bytes()).map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot stage identity: {e}"))?;
        tmp.as_file().sync_all().map_err(|e| miette!(help = "free space and check write permission under the project, then run `rhei budget init` again", "cannot sync identity: {e}"))?;
        staged.push((document, tmp));
    }
    for (document, tmp) in staged {
        persist_locked(tmp, document)
            .map_err(|e| file_io_report(document, "cannot persist identity", e.error))?;
        budget_sync_directory(document.parent().unwrap_or(Path::new(".")))?;
    }
    // Release metadata locks before acquiring the authority/project locks;
    // journal absence refuses admissions throughout installation. §AR-neural-admission.3
    drop(locks);
    let journal = if let Some(history) = verified_history {
        rhei_core::budget::Journal::initialize_imported(
            &project.root,
            &intent.project_uuid,
            intent.allowance,
            history,
            &intent.audit,
        )
    } else {
        rhei_core::budget::Journal::initialize_empty(
            &project.root,
            &intent.project_uuid,
            intent.allowance,
            &intent.audit,
        )
    }
    .map_err(budget_error)?;
    drop(journal);
    fs::remove_file(path).map_err(|e| file_io_report(path, "cannot finish initialization", e))?;
    budget_sync_directory(path.parent().expect("intent has a parent"))?;
    budget_ensure_ticket_identities(project)?;
    println!(
        "Initialized lifetime allowance panta:{}; inspect it with `rhei budget show`.",
        intent.project_uuid
    );
    Ok(())
}

fn budget_sync_directory(path: &Path) -> MietteResult<()> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| file_io_report(path, "cannot sync budget directory", e))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
