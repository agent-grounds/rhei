#[derive(Default)]
struct ParsedSnapshotRef {
    task_id: Option<String>,
    snapshot_name: Option<String>,
    emitting_state: Option<String>,
    visit: Option<u64>,
    target_slug: Option<String>,
    generation: Option<u64>,
}

fn resolve_snapshot_ref(
    ctx: &SnapshotCommandContext,
    reference: &str,
    target_override: Option<&str>,
    generation_override: Option<u64>,
) -> MietteResult<SnapshotRecord> {
    if let Some(path_record) = resolve_snapshot_path_ref(ctx, reference)? {
        return Ok(path_record);
    }

    let parsed = parse_snapshot_ref(reference, ctx)?;
    let records = read_snapshot_records(&ctx.cache_root)?;
    let mut matches: Vec<SnapshotRecord> = records
        .into_iter()
        .filter(|record| {
            parsed.task_id.as_deref().is_none_or(|value| record.task_id == value)
                && parsed.snapshot_name.as_deref().is_none_or(|value| record.snapshot_name == value)
                && parsed
                    .emitting_state
                    .as_deref()
                    .is_none_or(|value| record.emitting_state == value)
                && parsed.visit.is_none_or(|value| record.visit == value)
                && parsed.target_slug.as_deref().is_none_or(|value| record.target_slug == value)
                && target_override.is_none_or(|value| record.target_slug == value)
        })
        .collect();

    let generation = generation_override.or(parsed.generation);
    if let Some(generation) = generation {
        matches.retain(|record| record.generation == generation);
    } else {
        matches = select_current_records(reference, matches)?;
    }

    match matches.len() {
        0 => Err(miette!(
            help = snapshot_reference_help(),
            "snapshot reference '{reference}' did not match any cached generation"
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(ambiguous_snapshot_report(reference, &matches)),
    }
}

fn resolve_snapshot_path_ref(
    ctx: &SnapshotCommandContext,
    reference: &str,
) -> MietteResult<Option<SnapshotRecord>> {
    let path = Path::new(reference);
    if !path.is_absolute() && !reference.contains(std::path::MAIN_SEPARATOR) {
        return Ok(None);
    }
    let candidate =
        if path.is_absolute() { path.to_path_buf() } else { ctx.workspace_root.join(path) };
    if !candidate.join("manifest.json").is_file() {
        return Ok(None);
    }
    // Both sides of the containment test below have to be spelled the same
    // way; a verbatim prefix on one of them alone puts every generation
    // outside the cache on Windows. §REQ-cross-platform.5
    let cache_root = rhei_core::platform::canonical_path(&ctx.cache_root).map_err(|err| {
        file_io_report(&ctx.cache_root, "failed to resolve snapshot cache path", err)
    })?;
    let canonical = rhei_core::platform::canonical_path(&candidate).map_err(|err| {
        file_io_report(&candidate, "failed to resolve snapshot generation path", err)
    })?;
    if !canonical.starts_with(&cache_root) {
        return Err(miette!(
            help = "the snapshot cache root comes from settings.json. Check it with: rhei diag",
            "snapshot path '{}' is outside the configured cache '{}'",
            candidate.display(),
            ctx.cache_root.display()
        ));
    }
    let raw = fs::read_to_string(canonical.join("manifest.json")).map_err(|err| {
        file_io_report(&canonical.join("manifest.json"), "failed to read snapshot manifest", err)
    })?;
    let manifest = serde_json::from_str(&raw).map_err(|err| {
        miette!(
            help = snapshot_help(),
            "failed to parse snapshot manifest '{}': {err}", canonical.display()
        )
    })?;
    snapshot_record_from_manifest(&ctx.cache_root, &canonical.join("manifest.json"), manifest)
}

fn parse_snapshot_ref(
    reference: &str,
    ctx: &SnapshotCommandContext,
) -> MietteResult<ParsedSnapshotRef> {
    let (body, generation) = match reference.rsplit_once("/g") {
        Some((body, n)) => {
            let generation = n.parse::<u64>().map_err(|_| {
                miette!(
                    help = snapshot_reference_help(),
                    "snapshot reference '{reference}' has invalid generation '/g{n}'"
                )
            })?;
            (body, Some(generation))
        }
        None => (reference, None),
    };
    let parts: Vec<&str> = body.split(':').collect();
    if parts.len() < 2 || parts.len() > 4 || parts.iter().any(|part| part.is_empty()) {
        return Err(miette!(
            help = snapshot_reference_help(),
            "snapshot reference '{reference}' must use <task>:<name>[:<state>][@<visit>][:<target>][/g<N>]"
        ));
    }

    let mut parsed =
        ParsedSnapshotRef { task_id: Some(parts[0].to_string()), generation, ..Default::default() };
    let task_id = parts[0];

    match parts.len() {
        2 => {
            let (second, visit) = split_visit(parts[1], reference)?;
            parsed.visit = visit;
            if snapshot_name_exists(ctx, task_id, second)? {
                parsed.snapshot_name = Some(second.to_string());
            } else {
                parsed.snapshot_name = Some("_state".to_string());
                parsed.emitting_state = Some(second.to_string());
            }
        }
        3 => {
            if parts[1] == "_state" || snapshot_name_exists(ctx, task_id, parts[1])? {
                let (state, visit) = split_visit(parts[2], reference)?;
                parsed.snapshot_name = Some(parts[1].to_string());
                parsed.emitting_state = Some(state.to_string());
                parsed.visit = visit;
            } else {
                let (state, visit) = split_visit(parts[1], reference)?;
                parsed.snapshot_name = Some("_state".to_string());
                parsed.emitting_state = Some(state.to_string());
                parsed.visit = visit;
                parsed.target_slug = Some(parts[2].to_string());
            }
        }
        4 => {
            let (state, visit) = split_visit(parts[2], reference)?;
            parsed.snapshot_name = Some(parts[1].to_string());
            parsed.emitting_state = Some(state.to_string());
            parsed.visit = visit;
            parsed.target_slug = Some(parts[3].to_string());
        }
        _ => unreachable!(),
    }

    Ok(parsed)
}

fn split_visit<'a>(segment: &'a str, reference: &str) -> MietteResult<(&'a str, Option<u64>)> {
    match segment.rsplit_once('@') {
        Some((prefix, visit)) => {
            if prefix.is_empty() {
                return Err(miette!(
                    help = snapshot_reference_help(),
                    "snapshot reference '{reference}' has an empty state before '@'"
                ));
            }
            let visit = visit.parse::<u64>().map_err(|_| {
                miette!(
                    help = snapshot_reference_help(),
                    "snapshot reference '{reference}' has invalid visit '@{visit}'"
                )
            })?;
            Ok((prefix, Some(visit)))
        }
        None => Ok((segment, None)),
    }
}

fn snapshot_name_exists(
    ctx: &SnapshotCommandContext,
    task_id: &str,
    name: &str,
) -> MietteResult<bool> {
    Ok(read_snapshot_records(&ctx.cache_root)?
        .into_iter()
        .any(|record| record.task_id == task_id && record.snapshot_name == name))
}

fn select_current_records(
    reference: &str,
    records: Vec<SnapshotRecord>,
) -> MietteResult<Vec<SnapshotRecord>> {
    let mut grouped: BTreeMap<SnapshotIdentity, Vec<SnapshotRecord>> = BTreeMap::new();
    for record in records {
        grouped.entry(record.identity()).or_default().push(record);
    }
    let mut selected = Vec::new();
    for (identity, group) in grouped {
        let Some(current) = group.iter().find(|record| record.is_current).cloned() else {
            return Err(miette!(
                help = snapshot_reference_help(),
                "snapshot reference '{reference}' matched cached generations for {}, but none is marked current; retry with /g<N> or repair the current pointer",
                snapshot_identity_ref(&identity)
            ));
        };
        selected.push(current);
    }
    Ok(selected)
}

fn snapshot_identity_ref(identity: &SnapshotIdentity) -> String {
    format!(
        "{}:{}:{}@{}:{}",
        identity.task_id,
        identity.snapshot_name,
        identity.emitting_state,
        identity.visit,
        identity.target_slug
    )
}

fn ambiguous_snapshot_report(reference: &str, matches: &[SnapshotRecord]) -> Report {
    let mut sorted = matches.to_vec();
    sorted.sort_by_key(|record| record.display_ref());
    let candidates = sorted
        .iter()
        .map(|record| format!("  {}", record.display_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    miette!(
        help = snapshot_reference_help(),
        "snapshot reference '{reference}' is ambiguous; matched {} candidates:\n{}\nretry with explicit --task, --name, --state, --target, or --generation selectors",
        sorted.len(),
        candidates
    )
}

#[allow(clippy::too_many_arguments)]
fn snapshot_gc_command(
    ctx: &SnapshotCommandContext,
    task: Option<&str>,
    name: Option<&str>,
    older_than: Option<&str>,
    keep_generations: Option<usize>,
    include_operator: bool,
    orphaned: bool,
    dry_run: bool,
    force: bool,
) -> MietteResult<()> {
    if keep_generations == Some(0) {
        return Err(miette!(
            help = "--keep-generations keeps at least one generation per lineage; pass 1 or more.",
            "--keep-generations must be at least 1"
        ));
    }
    if older_than.is_none() && keep_generations.is_none() && !orphaned {
        return Err(miette!(
            help = "choose what to delete: --older-than <dur>, --keep-generations <n>, or --orphaned.",
            "snapshot gc requires a deletion policy: pass --older-than, --keep-generations, or --orphaned"
        ));
    }
    if !force && run_lock_is_held(&ctx.workspace_root)? {
        return Err(miette!(
            help = "a run is in progress and may still write snapshots. Wait for it to finish, or pass --force.",
            "refusing to garbage-collect snapshots while .rhei/run.lock is held; stop the run first or pass --force"
        ));
    }

    let older_than_secs = older_than.map(parse_snapshot_duration_secs).transpose()?;
    let now = std::time::SystemTime::now();
    let all_records = read_snapshot_records(&ctx.cache_root)?;
    let base_eligible: Vec<SnapshotRecord> = all_records
        .into_iter()
        .filter(|record| {
            if task.is_some_and(|value| record.task_id != value) {
                return false;
            }
            if name.is_some_and(|value| record.snapshot_name != value) {
                return false;
            }
            if !include_operator && record.produced_by != "orchestrator" {
                return false;
            }
            if orphaned && !is_snapshot_orphaned(record, ctx) {
                return false;
            }
            true
        })
        .collect();

    let mut eligible = if let Some(keep) = keep_generations {
        generations_beyond_keep(base_eligible, keep)
    } else {
        base_eligible
    };

    if let Some(threshold) = older_than_secs {
        eligible
            .retain(|record| snapshot_age_secs(record, now).is_some_and(|age| age >= threshold));
    }

    if !force {
        let protected: Vec<String> = eligible
            .iter()
            .filter(|record| snapshot_generation_protected_by_active_inherit(record, ctx))
            .map(SnapshotRecord::display_ref)
            .collect();
        if !protected.is_empty() {
            return Err(miette!(
                help = snapshot_help(),
                "refusing to garbage-collect snapshots selected by active snapshot.inherit rules: {}. Stop the run or pass --force to acknowledge the risk.",
                protected.join(", ")
            ));
        }
    }

    eligible.sort_by_key(|record| record.display_ref());
    let deleted_identities: BTreeSet<SnapshotIdentity> =
        eligible.iter().map(SnapshotRecord::identity).collect();
    for record in &eligible {
        if dry_run {
            println!("would delete {}", record.path.display());
        } else {
            fs::remove_dir_all(&record.path).map_err(|err| {
                file_io_report(&record.path, "failed to delete snapshot generation", err)
            })?;
            println!("deleted {}", record.path.display());
        }
    }
    if !dry_run {
        refresh_current_links(&ctx.cache_root, deleted_identities)?;
    }
    Ok(())
}

fn snapshot_generation_protected_by_active_inherit(
    record: &SnapshotRecord,
    ctx: &SnapshotCommandContext,
) -> bool {
    for task in flatten_tasks(&ctx.loaded.rhei) {
        let machine = ctx.machines.for_task(&task.id);
        let current_state = normalized_state_name(task.state.as_str(), machine);
        if is_terminal_state(&current_state, machine) {
            continue;
        }
        let Some(inherit) = effective_snapshot_inherit(machine, task, &current_state) else {
            continue;
        };
        let active_task_id = task.id.to_string();
        match inherit.from_axis.as_deref().unwrap_or("self") {
            "self" if record.task_id != active_task_id => continue,
            "ancestor"
                if !ancestor_task_ids(&active_task_id)
                    .iter()
                    .any(|ancestor| ancestor == &record.task_id) =>
            {
                continue;
            }
            "prior" => {
                let eligible = task.prior.iter().any(|prior| {
                    if prior.to_string() != record.task_id {
                        return false;
                    }
                    let Some(source) = find_task_by_id(&ctx.loaded.rhei.tasks, prior) else {
                        return false;
                    };
                    let source_machine = ctx.machines.for_task(&source.id);
                    let source_state = normalized_state_name(&source.state, source_machine);
                    is_terminal_state(&source_state, source_machine)
                        && !rhei_validator::is_cancelled_state_name(&source_state)
                });
                if !eligible {
                    continue;
                }
            }
            "self" | "ancestor" => {}
            _ => continue,
        }
        if inherit.name != record.snapshot_name {
            continue;
        }
        if let Some(select) = inherit.select.as_ref() {
            if select.state.as_deref().is_some_and(|state| state != record.emitting_state) {
                continue;
            }
            if let Some(target) = select.target.as_deref() {
                if target != "same" && target != record.target_slug {
                    continue;
                }
            }
            if let Some(visit) = select.visit.as_ref().and_then(yaml_selector_u64) {
                if visit != record.visit {
                    continue;
                }
            }
            match select.generation.as_ref() {
                Some(value) if yaml_selector_string(value) == Some("latest") => {
                    if record.produced_by == "orchestrator"
                        && record.generation
                            == latest_orchestrator_generation(&record.identity(), &ctx.cache_root)
                                .unwrap_or(record.generation)
                    {
                        return true;
                    }
                }
                Some(value) if yaml_selector_u64(value).is_some() => {
                    if yaml_selector_u64(value) == Some(record.generation) {
                        return true;
                    }
                }
                Some(value) if yaml_selector_string(value) == Some("current") => {
                    if record.is_current {
                        return true;
                    }
                }
                Some(_) | None => {
                    if record.is_current {
                        return true;
                    }
                }
            }
        } else if record.is_current {
            return true;
        }
    }
    false
}

fn yaml_selector_u64(value: &YamlValue) -> Option<u64> {
    match value {
        YamlValue::Number(number) => number.as_u64(),
        YamlValue::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn yaml_selector_string(value: &YamlValue) -> Option<&str> {
    match value {
        YamlValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn latest_orchestrator_generation(identity: &SnapshotIdentity, cache_root: &Path) -> Option<u64> {
    read_snapshot_records_for_identity(cache_root, identity)
        .ok()?
        .into_iter()
        .filter(|record| record.produced_by == "orchestrator")
        .map(|record| record.generation)
        .max()
}

fn generations_beyond_keep(records: Vec<SnapshotRecord>, keep: usize) -> Vec<SnapshotRecord> {
    let mut grouped: BTreeMap<SnapshotIdentity, Vec<SnapshotRecord>> = BTreeMap::new();
    for record in records {
        grouped.entry(record.identity()).or_default().push(record);
    }
    let mut delete = Vec::new();
    for mut group in grouped.into_values() {
        group.sort_by(|a, b| b.generation.cmp(&a.generation));
        delete.extend(group.into_iter().skip(keep));
    }
    delete
}

fn refresh_current_links(
    cache_root: &Path,
    identities: BTreeSet<SnapshotIdentity>,
) -> MietteResult<()> {
    for identity in identities {
        let identity_dir = cache_root
            .join(&identity.task_id)
            .join(&identity.snapshot_name)
            .join(&identity.emitting_state)
            .join(identity.visit.to_string())
            .join(&identity.target_slug);
        let mut records = read_snapshot_records_for_identity(cache_root, &identity)?;
        records.retain(|record| record.produced_by == "orchestrator");
        records.sort_by(|a, b| b.generation.cmp(&a.generation));
        let current = identity_dir.join("current");
        if let Some(newest) = records.first() {
            let target = newest.path.file_name().and_then(OsStr::to_str).ok_or_else(|| {
                miette!(
                    help = "this cached snapshot is corrupt. Remove it with: rhei snapshot gc --orphaned",
                    "invalid snapshot generation path '{}'", newest.path.display()
                )
            })?;
            // The gc rewrites one identity at a time and never races itself.
            replace_current_pointer(&identity_dir, target, "rhei-gc")?;
        } else if current.exists() || current.is_symlink() {
            fs::remove_file(&current).map_err(|err| {
                file_io_report(&current, "failed to remove stale current pointer", err)
            })?;
        }
    }
    Ok(())
}

fn read_snapshot_records_for_identity(
    cache_root: &Path,
    identity: &SnapshotIdentity,
) -> MietteResult<Vec<SnapshotRecord>> {
    Ok(read_snapshot_records(cache_root)?
        .into_iter()
        .filter(|record| record.identity() == *identity)
        .collect())
}
