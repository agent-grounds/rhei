// Explicit compatibility migration for export producers missing from Prior.

// §AR-source-file-size.3 §FS-rhei-migrate

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportPriorAddition {
    file: PathBuf,
    consumer: String,
    consumer_kind: String,
    producer: String,
    producer_reference: String,
}

struct StagedMigrationFile {
    path: PathBuf,
    additions: Vec<ExportPriorAddition>,
    temp: tempfile::NamedTempFile,
}

/// Repair every otherwise-valid missing export-producer edge in one explicit,
/// locked operation. §FS-rhei-migrate.1 §FS-rhei-migrate.4
fn migrate_export_priors_command(input: &Path, dry_run: bool) -> MietteResult<()> {
    let initial = load_plan(input)?;
    let workspace_root = execution_workspace_root(input);
    let _run_locks = acquire_migration_run_locks(&initial, &workspace_root)?;

    // Decide from bytes reread after every relevant sidecar is held; widen and
    // reacquire the canonical lock set if that reread finds another file. §FS-rhei-migrate.4
    let mut paths = migration_file_paths(input, &initial)?;
    loop {
        let locks = lock_migration_files(&paths)?;
        let loaded = load_plan(input)?;
        let additions = export_prior_additions(input, &loaded);
        let current_paths = canonical_migration_paths(
            additions.iter().map(|addition| addition.file.as_path()),
        )?;
        if current_paths.keys().all(|path| paths.contains_key(path)) {
            return migrate_locked(input, loaded, additions, locks, dry_run);
        }
        drop(locks);
        paths.extend(current_paths);
    }
}

/// Migration never queues behind execution: every involved root is either
/// held immediately or the whole operation is refused. §FS-rhei-migrate.4
fn acquire_migration_run_locks(
    loaded: &LoadedPlan,
    workspace_root: &Path,
) -> MietteResult<Vec<HeldRunLock>> {
    let mut locks = Vec::new();
    for root in run_lock_roots(loaded, workspace_root) {
        match try_acquire_run_lock(&root)? {
            Some(lock) => locks.push(lock),
            None => return Err(run_lock_conflict(&root)),
        }
    }
    Ok(locks)
}

fn migration_file_paths(
    input: &Path,
    loaded: &LoadedPlan,
) -> MietteResult<BTreeMap<PathBuf, PathBuf>> {
    canonical_migration_paths(
        export_prior_additions(input, loaded)
            .iter()
            .map(|addition| addition.file.as_path()),
    )
}

fn canonical_migration_paths<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> MietteResult<BTreeMap<PathBuf, PathBuf>> {
    let mut canonical = BTreeMap::new();
    for path in paths {
        let key = rhei_core::platform::canonical_path(path)
            .map_err(|err| file_io_report(path, "failed to resolve migration target", err))?;
        canonical.insert(key, path.to_path_buf());
    }
    Ok(canonical)
}

fn lock_migration_files(
    paths: &BTreeMap<PathBuf, PathBuf>,
) -> MietteResult<BTreeMap<PathBuf, LockedPlanFile>> {
    let mut locks = BTreeMap::new();
    for (canonical, path) in paths {
        locks.insert(canonical.clone(), LockedPlanFile::open(path)?);
    }
    Ok(locks)
}

/// Classify once in core, then deduplicate several exports from one producer
/// into one consumer-producer edge. §FS-rhei-migrate.1.1
fn export_prior_additions(input: &Path, loaded: &LoadedPlan) -> Vec<ExportPriorAddition> {
    let mut seen = HashSet::new();
    rhei_validator::classify_export_relationships(&loaded.rhei)
        .into_iter()
        .filter(|relationship| {
            matches!(
                relationship.kind,
                rhei_validator::ExportRelationshipKind::MissingDirectPrior
                    | rhei_validator::ExportRelationshipKind::ProposedCycle
            )
        })
        .filter(|relationship| {
            seen.insert((relationship.consumer.clone(), relationship.producer.clone()))
        })
        .map(|relationship| {
            let consumer = relationship.consumer.to_string();
            let producer = relationship.producer.to_string();
            let producer_local = migration_reference_id(&consumer, &producer).to_string();
            let producer_kind = relationship.producer_kind.as_deref().unwrap_or("task");
            ExportPriorAddition {
                file: loaded.task_file(&consumer, input),
                consumer,
                consumer_kind: title_case_node_kind(&relationship.consumer_kind),
                producer,
                producer_reference: format!(
                    "{} {}",
                    title_case_node_kind(producer_kind),
                    producer_local
                ),
            }
        })
        .collect()
}

fn migration_reference_id<'a>(consumer: &str, producer: &'a str) -> &'a str {
    let consumer_rhei = consumer.split_once('.').map(|(rhei, _)| rhei);
    let producer_parts = producer.split_once('.');
    match (consumer_rhei, producer_parts) {
        (Some(consumer_rhei), Some((producer_rhei, local)))
            if consumer_rhei == producer_rhei =>
        {
            local
        }
        _ => producer,
    }
}

fn title_case_node_kind(kind: &str) -> String {
    let mut chars = kind.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Task".to_string(),
    }
}

fn migrate_locked(
    input: &Path,
    mut loaded: LoadedPlan,
    additions: Vec<ExportPriorAddition>,
    locks: BTreeMap<PathBuf, LockedPlanFile>,
    dry_run: bool,
) -> MietteResult<()> {
    // Validate the exact complete graph that the staged authored rewrite will
    // produce. Only the classified missing edges are provisionally supplied;
    // every independent error retains precedence. §FS-rhei-migrate.1.2
    apply_additions_to_graph(&mut loaded.rhei.tasks, &additions);
    let pass = validation_pass_for_loaded(input, None, loaded, None)?;
    if !pass.errors.is_empty() {
        return Err(validation_report(
            input,
            &pass.state_machine_sources,
            &pass.errors,
            &pass.help,
        ));
    }

    if additions.is_empty() {
        println!("No export Prior migration needed");
        return Ok(());
    }

    let grouped = group_migration_additions(additions)?;
    if dry_run {
        for additions in grouped.values() {
            for addition in additions {
                print_migration_addition("Would add", addition);
            }
        }
        print_migration_summary(&grouped);
        return Ok(());
    }

    // Every temp file is created and fully written before the first authored
    // pathname is replaced. §FS-rhei-migrate.4.1
    let mut staged = Vec::new();
    for (canonical, additions) in &grouped {
        let locked = locks.get(canonical).expect("affected file lock is held");
        let raw = locked.read_to_string("failed to read migration target")?;
        let rewritten = rewrite_export_priors(&raw, additions)?;
        let parent = additions[0].file.parent().unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|err| {
            file_io_report(parent, "failed to stage export Prior migration", err)
        })?;
        temp.write_all(rewritten.as_bytes()).map_err(|err| {
            file_io_report(&additions[0].file, "failed to stage export Prior migration", err)
        })?;
        staged.push(StagedMigrationFile {
            path: additions[0].file.clone(),
            additions: additions.clone(),
            temp,
        });
    }

    let all_paths: Vec<PathBuf> = staged.iter().map(|file| file.path.clone()).collect();
    let mut completed = Vec::new();
    for (index, file) in staged.into_iter().enumerate() {
        if let Err(err) = persist_migration_file(file.temp, &file.path, index) {
            let remaining = &all_paths[index..];
            return Err(miette!(
                help = "re-run the same migration; completed files are idempotent and the remaining files will be replanned",
                "export Prior migration stopped after a per-file replacement failure: {err}\ncompleted files: {}\nremaining files: {}",
                display_migration_paths(&completed),
                display_migration_paths(remaining)
            ));
        }
        for addition in &file.additions {
            print_migration_addition("Added", addition);
        }
        completed.push(file.path);
    }
    print_migration_summary(&grouped);
    Ok(())
}

fn apply_additions_to_graph(
    tasks: &mut [rhei_core::ast::Task],
    additions: &[ExportPriorAddition],
) {
    for task in tasks {
        for addition in additions.iter().filter(|addition| addition.consumer == task.id.to_string()) {
            let producer = parse_task_id(&addition.producer);
            if !task.prior.contains(&producer) {
                task.prior.push(producer);
                task.prior_kinds.push(
                    addition.producer_reference.split_once(' ').map(|(kind, _)| kind.to_string()),
                );
            }
        }
        apply_additions_to_graph(&mut task.children, additions);
    }
}

fn group_migration_additions(
    additions: Vec<ExportPriorAddition>,
) -> MietteResult<BTreeMap<PathBuf, Vec<ExportPriorAddition>>> {
    let mut grouped: BTreeMap<PathBuf, Vec<ExportPriorAddition>> = BTreeMap::new();
    for addition in additions {
        let canonical = rhei_core::platform::canonical_path(&addition.file).map_err(|err| {
            file_io_report(&addition.file, "failed to resolve migration target", err)
        })?;
        grouped.entry(canonical).or_default().push(addition);
    }
    Ok(grouped)
}

/// Rewrite only the target metadata lines, preserving their original newline
/// bytes and every unrelated byte. §FS-rhei-migrate.2 §FS-rhei-migrate.2.1
fn rewrite_export_priors(
    raw: &str,
    additions: &[ExportPriorAddition],
) -> MietteResult<String> {
    let mut output = raw.to_string();
    // Reverse source order keeps byte offsets stable while applying several
    // consumers in one file.
    let mut edits = Vec::new();
    for addition in additions.iter().rev() {
        if edits.iter().any(|(consumer, _, _)| consumer == &addition.consumer) {
            continue;
        }
        let refs: Vec<&str> = additions
            .iter()
            .filter(|candidate| candidate.consumer == addition.consumer)
            .map(|candidate| candidate.producer_reference.as_str())
            .collect();
        edits.push((addition.consumer.clone(), addition.file.clone(), refs));
    }
    for (consumer, file, refs) in edits {
        let local = rhei_local_id_str(&consumer);
        output = rewrite_one_consumer(&output, local, &refs).map_err(|reason| {
            miette!(
                help = "the plan changed while migration was preparing it; re-run the command",
                "could not rewrite Task {} in {}: {}",
                consumer,
                file.display(),
                reason
            )
        })?;
    }
    Ok(output)
}

fn rewrite_one_consumer(raw: &str, local_id: &str, references: &[&str]) -> Result<String, String> {
    let lines = line_spans(raw);
    let mut in_code = false;
    let mut target_heading = None;
    let mut target_end = raw.len();
    for (start, end) in &lines {
        let line = trim_line_ending(&raw[*start..*end]);
        if let Some((_, id)) = node_heading_outside_code(line, &mut in_code) {
            if target_heading.is_some() {
                target_end = *start;
                break;
            }
            if id == local_id {
                target_heading = Some(*end);
            }
        }
    }
    let Some(start) = target_heading else {
        return Err("the owning task heading is no longer present".to_string());
    };

    let mut state_end = None;
    let mut prior_span = None;
    for (line_start, line_end) in lines
        .iter()
        .copied()
        .filter(|(line_start, _)| *line_start >= start && *line_start < target_end)
    {
        let line = trim_line_ending(&raw[line_start..line_end]);
        if line.starts_with("**State:**") {
            state_end = Some(line_end);
        } else if line.starts_with("**Prior:**") {
            prior_span = Some((line_start, line_end));
            break;
        }
    }

    let joined = references.join(", ");
    if let Some((line_start, line_end)) = prior_span {
        let whole = &raw[line_start..line_end];
        let ending = &whole[trim_line_ending(whole).len()..];
        let body = trim_line_ending(whole);
        return Ok(format!(
            "{}{}, {}{}{}",
            &raw[..line_start],
            body,
            joined,
            ending,
            &raw[line_end..]
        ));
    }
    let Some(state_end) = state_end else {
        return Err("the task has no **State:** line".to_string());
    };
    let state_line = lines
        .iter()
        .find(|(_, end)| *end == state_end)
        .map(|(start, end)| &raw[*start..*end])
        .unwrap_or("");
    let newline = if state_line.ends_with("\r\n") { "\r\n" } else { "\n" };
    Ok(format!(
        "{}**Prior:** {}{}{}",
        &raw[..state_end],
        joined,
        newline,
        &raw[state_end..]
    ))
}

fn line_spans(raw: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in raw.bytes().enumerate() {
        if byte == b'\n' {
            spans.push((start, index + 1));
            start = index + 1;
        }
    }
    if start < raw.len() {
        spans.push((start, raw.len()));
    }
    spans
}

fn trim_line_ending(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line)
}

fn print_migration_addition(verb: &str, addition: &ExportPriorAddition) {
    println!(
        "{} {} to {} {} **Prior:** in {}",
        verb,
        addition.producer_reference,
        addition.consumer_kind,
        addition.consumer,
        addition.file.display()
    );
}

fn print_migration_summary(grouped: &BTreeMap<PathBuf, Vec<ExportPriorAddition>>) {
    let edges: usize = grouped.values().map(Vec::len).sum();
    println!("Added {edges} direct Prior edge(s) in {} file(s)", grouped.len());
}

fn display_migration_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        "(none)".to_string()
    } else {
        paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
thread_local! {
    static MIGRATION_REPLACE_FAILURE: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn fail_migration_replacement_at(index: Option<usize>) {
    MIGRATION_REPLACE_FAILURE.with(|failure| failure.set(index));
}

fn persist_migration_file(
    temp: tempfile::NamedTempFile,
    path: &Path,
    index: usize,
) -> Result<(), String> {
    #[cfg(test)]
    if MIGRATION_REPLACE_FAILURE.with(|failure| failure.get()) == Some(index) {
        return Err(format!("injected replacement failure for {}", path.display()));
    }
    #[cfg(not(test))]
    let _ = index;
    persist_locked(temp, path).map_err(|err| err.to_string())
}
