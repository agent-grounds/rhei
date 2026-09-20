// Financial identities live in authored metadata, outside reset's runtime
// cleanup. §FS-rhei-budgets.2.3

struct BudgetProject {
    input: PathBuf,
    root: PathBuf,
    metadata_file: PathBuf,
}

impl BudgetProject {
    fn resolve(target: &Path) -> MietteResult<Self> {
        let target = resolve_plan_target(Some(target.to_path_buf()))?;
        let mut input = std::path::absolute(target.path())
            .map_err(|e| miette!(help = "run the command from a directory that still exists", "cannot resolve the budget project path: {e}"))?;
        // A member target still owns the outer Panta allowance. §FS-rhei-budgets.8
        if workspace::panta_project_dir(&input).is_none() {
            let candidate = if input.is_dir() {
                input.parent()
            } else if input.file_name().and_then(|s| s.to_str()) == Some(workspace::RHEI_INDEX_FILE)
            {
                input.parent().and_then(Path::parent)
            } else {
                input.parent()
            };
            if let Some(parent) = candidate.filter(|p| workspace::is_panta_project(p)) {
                input = parent.to_path_buf();
            }
        }
        let root = execution_workspace_root(&input);
        let metadata_file = if workspace::panta_project_dir(&input).is_some() {
            root.join(workspace::PANTA_INDEX_FILE)
        } else if workspace::is_workspace(&root) {
            root.join(workspace::RHEI_INDEX_FILE)
        } else {
            input.clone()
        };
        Ok(Self { input, root, metadata_file })
    }

    fn identity(&self) -> MietteResult<Option<String>> {
        let raw = read_input_file(&self.metadata_file)?;
        let metadata = budget_document_metadata(&raw, &self.metadata_file)?;
        match metadata.get(yaml_key("budgetProjectId")) {
            None => Ok(None),
            Some(value) => {
                let id = value
                    .as_str()
                    .ok_or_else(|| miette!(help = "restore the project frontmatter from version control; an allowance is never re-minted", "budgetProjectId must be a lowercase RFC 4122 UUID"))?;
                validate_budget_uuid(id)?;
                Ok(Some(id.into()))
            }
        }
    }

    fn required_identity(&self) -> MietteResult<String> {
        self.identity()?.ok_or_else(|| miette!(help = "initialize a finite allowance with `rhei budget init <TARGET> --invocations N --spend-micro N --currency USD --reason TEXT`", "missing project allowance: budgetProjectId is absent"))
    }
}

/// Parse only the owning document; merged metadata is unsuitable for writes.
/// §FS-rhei-budgets.2.3
fn budget_document_metadata(raw: &str, path: &Path) -> MietteResult<Metadata> {
    if raw.lines().any(|line| line.starts_with("# Panta:")) {
        Ok(rhei_core::parser::parse_panta_manifest(raw)
            .map_err(|e| parse_report(path, raw, &e))?
            .metadata
            .unwrap_or_default())
    } else if path.file_name().and_then(|s| s.to_str()) == Some(workspace::RHEI_INDEX_FILE) {
        Ok(rhei_core::parser::parse_workspace_index(raw)
            .map_err(|e| parse_report(path, raw, &e))?
            .metadata
            .unwrap_or_default())
    } else {
        Ok(rhei_core::parse(raw)
            .map_err(|e| parse_report(path, raw, &e))?
            .metadata
            .unwrap_or_default())
    }
}

fn validate_budget_uuid(value: &str) -> MietteResult<()> {
    let id = uuid::Uuid::parse_str(value).map_err(|e| miette!(help = "restore the recorded identity from version control; a fresh one would open a second allowance", "invalid budget identity: {e}"))?;
    if id.to_string() != value || id.get_variant() != uuid::Variant::RFC4122 {
        return Err(miette!(help = "restore the recorded identity from version control; a fresh one would open a second allowance", "budget identity must be a lowercase RFC 4122 UUID"));
    }
    Ok(())
}

/// Display IDs locate metadata; the returned UUID is the financial identity.
/// §FS-rhei-budgets.2.3
fn budget_ticket_identity(
    loaded: &LoadedPlan,
    task: &TaskId,
    project_uuid: &str,
) -> MietteResult<String> {
    let uuid = task_metadata_map(loaded.rhei.metadata.as_ref(), task)
        .and_then(|m| m.get(yaml_key("budgetTicketId")))
        .and_then(YamlValue::as_str)
        .ok_or_else(|| miette!(help = "run `rhei budget init` or any bounded run once to assign ticket identities", "ticket {task} has no persistent budgetTicketId"))?;
    validate_budget_uuid(uuid)?;
    Ok(format!("ticket:{project_uuid}:{uuid}"))
}

/// Preserve unrelated frontmatter and use the existing stable writer lock.
/// The caller already owns the Panta budget lock. §AR-neural-admission.3
fn write_budget_metadata(
    path: &Path,
    locked: &LockedPlanFile,
    metadata: &Metadata,
) -> MietteResult<()> {
    let raw = locked.read_to_string("cannot read budget metadata")?;
    let rewritten = rewrite_frontmatter(&raw, metadata)?;
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| file_io_report(path, "cannot stage budget identity", e))?;
    tmp.write_all(rewritten.as_bytes())
        .map_err(|e| file_io_report(path, "cannot write budget identity", e))?;
    tmp.as_file().sync_all().map_err(|e| file_io_report(path, "cannot sync budget identity", e))?;
    persist_locked(tmp, path)
        .map_err(|e| file_io_report(path, "cannot persist budget identity", e.error))?;
    #[cfg(unix)]
    fs::File::open(path.parent().unwrap_or(Path::new(".")))
        .and_then(|f| f.sync_all())
        .map_err(|e| file_io_report(path, "cannot sync identity directory", e))?;
    Ok(())
}

fn budget_task_list(tasks: &[rhei_core::ast::Task]) -> Vec<&rhei_core::ast::Task> {
    tasks
        .iter()
        .flat_map(|task| std::iter::once(task).chain(budget_task_list(&task.children)))
        .collect()
}

/// Initializing a fresh document is explicit; deleting its ledger is never
/// initialization. Historical uncertainty refuses before any identity write.
/// §FS-rhei-budgets.8 §FS-rhei-budgets.12
fn budget_initialize(
    target: &Path,
    allowance: rhei_core::budget::Allowance,
    reason: &str,
    history: Option<&Path>,
) -> MietteResult<()> {
    allowance.validate().map_err(budget_error)?;
    let audit = budget_audit(reason)?;
    let project = BudgetProject::resolve(target)?;
    let budget_dir = project.root.join(".agent-grounds/rhei/budgets");
    fs::create_dir_all(&budget_dir)
        .map_err(|e| file_io_report(&budget_dir, "cannot create budget directory", e))?;
    // Bootstrap identity is serialized before there is a UUID-specific lock.
    // The sidecar is stable and never removed. §AR-neural-admission.3
    let bootstrap = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(budget_dir.join("identity.lock"))
        .map_err(|e| miette!(help = "another rhei process holds the budget lock; let it finish and run the command again", "cannot lock project budget identity: {e}"))?;
    bootstrap.lock_exclusive().map_err(|e| miette!(help = "another rhei process holds the budget lock; let it finish and run the command again", "cannot lock project budget identity: {e}"))?;
    let intent_path = budget_dir.join("initialization.json");
    if intent_path.exists() {
        return budget_resume_initialization(&project, &intent_path, &allowance, history);
    }
    let existing = project.identity()?;
    let loaded = load_plan(&project.input)?;
    if let Some(id) = &existing {
        // Opening first preserves missing/corrupt-ledger refusal even when
        // reset removed the accounting history. §FS-rhei-budgets.7
        let journal =
            rhei_core::budget::Journal::open(&project.root, id, false).map_err(budget_error)?;
        if journal.snapshot().map_err(budget_error)?.allowance != allowance || history.is_some() {
            return Err(miette!(help = "change capacity with `rhei budget adjust --reason <why>`; initialization never adds to a live allowance", "budget already initialized; use an audited adjustment instead"));
        }
        println!("Allowance already initialized as panta:{id}; no capacity added.");
        return Ok(());
    }
    // A removed project identity must not mint capacity from an apparently
    // fresh document beside an existing financial history. §FS-rhei-budgets.2.3
    for entry in fs::read_dir(&budget_dir)
        .map_err(|e| file_io_report(&budget_dir, "cannot inspect budget identities", e))?
    {
        let entry = entry
            .map_err(|e| file_io_report(&budget_dir, "cannot inspect budget identities", e))?;
        if entry.file_type().map_err(|e| miette!(help = "recover the ledger with `rhei budget recover` before running bounded work again", "cannot inspect budget identity: {e}"))?.is_dir() {
            return Err(miette!(help = "restore the project frontmatter from version control; existing history may never be replaced by a fresh balance", "budgetProjectId is missing beside existing budget history; restore the identity and ledger instead of initializing a fresh balance"));
        }
    }
    let history_required = budget_history_required(&loaded, &project.root, history)?;
    let mut intent = budget_prepare_initialization(
        &project,
        &loaded,
        allowance,
        audit,
        history_required,
    )?;
    budget_save_initialization(&intent_path, &intent)?;
    if let Some(history) = history {
        budget_attach_history(&mut intent, history)?;
        budget_replace_initialization(&intent_path, &intent)?;
    }
    if intent.history_required && intent.history.is_none() {
        return Err(miette!(
            help = "supply the signed provider export with --history <PATH>; unknown historical exposure is never assumed to be zero",
            "historical exposure requires complete provider evidence for panta:{} through {}; rerun this exact init with --history <PATH>",
            intent.project_uuid,
            intent.history_cutoff
        ));
    }
    budget_install_initialization(&project, &intent_path, intent)?;

    Ok(())
}

/// A missing usage record is not a zero-cost import. A complete history
/// importer needs independently authoritative evidence, not legacy totals.
/// §FS-rhei-budgets.8
fn budget_history_required(
    loaded: &LoadedPlan,
    root: &Path,
    history: Option<&Path>,
) -> MietteResult<bool> {
    if history.is_some() {
        return Ok(true);
    }
    let roots = std::iter::once(root.to_path_buf())
        .chain(loaded.rhei_roots.values().cloned())
        .collect::<BTreeSet<_>>();
    for root in roots {
        for relative in [
            "runtime/accounting/invocations",
            "runtime/spawns",
            "runtime/logs",
            "runtime/state-transitions.log",
            "runtime/state-transitions.jsonl",
        ] {
            let path = root.join(relative);
            match fs::metadata(&path) {
                Ok(m) => {
                    let has_history = if m.is_dir() {
                        fs::read_dir(&path)
                            .map_err(|e| {
                                file_io_report(&path, "cannot inspect historical exposure", e)
                            })?
                            .next()
                            .is_some()
                    } else {
                        m.len() != 0
                    };
                    if has_history {
                        return Ok(true);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(file_io_report(&path, "cannot inspect historical exposure", e))
                }
            }
        }
    }
    Ok(false)
}
