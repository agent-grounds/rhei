// Manual source validation is separate from spawn-time preload: operator and
// automatic Prior selection share contract rules, but not their entry point.
// §AR-source-file-size.3 §FS-rhei-snapshot-operations.2

/// Consume only the selected invocation's override, once it reaches preload.
/// A preload error aborts the run; a cold fallback still uses this attempt.
/// §FS-rhei-snapshot-operations.2
fn take_snapshot_override_for_invocation(
    override_selection: Option<&SnapshotOverrideRunSelection>,
    task: &rhei_core::ast::Task,
    target_slug: &str,
    has_override_ref: bool,
) -> bool {
    if !has_override_ref {
        return false;
    }
    override_selection.is_none_or(|selection| {
        selection.task_id == task.id.to_string()
            && selection.target_slug == target_slug
            && !selection.consumed.replace(true)
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_contract(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
    record: &SnapshotRecord,
    resolved: &ResolvedAgent,
) -> MietteResult<()> {
    if record.snapshot_name != inherit.name {
        return Err(miette!(
            help = snapshot_inherit_help(),
            "--from-snapshot selected snapshot name '{}', but snapshot.inherit requires '{}'",
            record.snapshot_name,
            inherit.name
        ));
    }
    let task_id = task.id.to_string();
    match inherit.from_axis.as_deref().unwrap_or("self") {
        "self" => {
            if record.task_id != task_id {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected task '{}', but snapshot.inherit.from: self requires task '{}'",
                    record.task_id,
                    task_id
                ));
            }
            if record.emitting_state == current_state && record.visit >= visit_count {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected {} from the current or future visit; snapshot.inherit.from: self only permits prior visits",
                    record.display_ref()
                ));
            }
        }
        "ancestor" => {
            let ancestors = ancestor_task_ids(&task_id);
            if !ancestors.iter().any(|ancestor| ancestor == &record.task_id) {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected task '{}', but snapshot.inherit.from: ancestor requires an ancestor of task '{}'",
                    record.task_id,
                    task_id
                ));
            }
        }
        "prior" => {
            if !prior_sources.iter().any(|source| source == &record.task_id) {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected task '{}', but it is not a declared Prior source eligible for task '{}'",
                    record.task_id,
                    task_id
                ));
            }
        }
        other => {
            return Err(miette!(
                help = state_machine_help(),
                "unsupported snapshot.inherit.from '{}' while validating --from-snapshot",
                other
            ));
        }
    }

    if let Some(select) = inherit.select.as_ref() {
        if let Some(state) = select.state.as_deref() {
            if record.emitting_state != state {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected emitting state '{}', but snapshot.inherit.select.state requires '{}'",
                    record.emitting_state,
                    state
                ));
            }
        }
        if let Some(target) = select.target.as_deref() {
            let required_target = if target == "same" { target_slug } else { target };
            if record.target_slug != required_target {
                return Err(miette!(
                    help = snapshot_inherit_help(),
                    "--from-snapshot selected target '{}', but snapshot.inherit.select.target requires '{}'",
                    record.target_slug,
                    required_target
                ));
            }
        }
        if let Some(visit) = select.visit.as_ref() {
            validate_snapshot_override_visit(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                prior_sources,
                record,
                visit,
            )?;
        } else {
            validate_snapshot_override_default_visit(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                prior_sources,
                record,
            )?;
        }
        if let Some(generation) = select.generation.as_ref() {
            validate_snapshot_override_generation(
                cache_root,
                task,
                current_state,
                inherit,
                target_slug,
                visit_count,
                prior_sources,
                record,
                generation,
            )?;
        } else if !record.is_current {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected {}, but snapshot.inherit.select.generation defaults to current",
                record.display_ref()
            ));
        }
    } else {
        validate_snapshot_override_default_visit(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
            prior_sources,
            record,
        )?;
        if !record.is_current {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected {}, but snapshot.inherit.select.generation defaults to current",
                record.display_ref()
            ));
        }
    }

    if inherit.compat.as_deref().unwrap_or("native") == "native"
        && !snapshot_record_native_compatible(record, resolved)
    {
        return Err(miette!(
            help = snapshot_inherit_help(),
            "--from-snapshot selected snapshot {} is not native-compatible with agent '{}'",
            record.display_ref(),
            resolved.agent.id()
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_visit(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
    record: &SnapshotRecord,
    visit: &YamlValue,
) -> MietteResult<()> {
    if let Some(required_visit) = yaml_selector_u64(visit) {
        if record.visit != required_visit {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected visit {}, but snapshot.inherit.select.visit requires {}",
                record.visit,
                required_visit
            ));
        }
    } else if yaml_selector_string(visit) == Some("latest") {
        let mut candidates = snapshot_override_contract_candidates(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
            prior_sources,
        )?;
        if inherit.from_axis.as_deref() == Some("prior") {
            candidates.retain(|candidate| candidate.task_id == record.task_id);
        }
        let latest_visit = candidates.iter().map(|candidate| candidate.visit).max();
        if latest_visit != Some(record.visit) {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected visit {}, but snapshot.inherit.select.visit requires latest visit {}",
                record.visit,
                latest_visit.unwrap_or(record.visit)
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_default_visit(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
    record: &SnapshotRecord,
) -> MietteResult<()> {
    let mut candidates = snapshot_override_contract_candidates(
        cache_root,
        task,
        current_state,
        inherit,
        target_slug,
        visit_count,
        prior_sources,
    )?;
    if inherit.from_axis.as_deref() == Some("prior") {
        candidates.retain(|candidate| candidate.task_id == record.task_id);
    }
    let latest_visit = candidates.iter().map(|candidate| candidate.visit).max();
    if latest_visit != Some(record.visit) {
        return Err(miette!(
            help = snapshot_inherit_help(),
            "--from-snapshot selected visit {}, but snapshot.inherit.select.visit defaults to latest visit {}",
            record.visit,
            latest_visit.unwrap_or(record.visit)
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_snapshot_override_generation(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
    record: &SnapshotRecord,
    generation: &YamlValue,
) -> MietteResult<()> {
    if let Some(required_generation) = yaml_selector_u64(generation) {
        if record.generation != required_generation {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected generation {}, but snapshot.inherit.select.generation requires {}",
                record.generation,
                required_generation
            ));
        }
    } else if yaml_selector_string(generation) == Some("latest") {
        let mut candidates = snapshot_override_contract_candidates(
            cache_root,
            task,
            current_state,
            inherit,
            target_slug,
            visit_count,
            prior_sources,
        )?;
        if inherit.from_axis.as_deref() == Some("prior") {
            candidates.retain(|candidate| candidate.task_id == record.task_id);
        }
        candidates.retain(|candidate| candidate.visit == record.visit);
        let latest_generation = candidates.iter().map(|candidate| candidate.generation).max();
        if latest_generation != Some(record.generation) {
            return Err(miette!(
                help = snapshot_inherit_help(),
                "--from-snapshot selected generation {}, but snapshot.inherit.select.generation requires latest generation {}",
                record.generation,
                latest_generation.unwrap_or(record.generation)
            ));
        }
    } else if yaml_selector_string(generation) == Some("current") && !record.is_current {
        return Err(miette!(
            help = snapshot_inherit_help(),
            "--from-snapshot selected {}, but snapshot.inherit.select.generation requires current",
            record.display_ref()
        ));
    }
    Ok(())
}

fn snapshot_override_contract_candidates(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
) -> MietteResult<Vec<SnapshotRecord>> {
    let mut scoped = read_snapshot_records(cache_root)?
        .into_iter()
        .filter(|candidate| candidate.snapshot_name == inherit.name)
        .filter(|candidate| candidate.produced_by == "orchestrator")
        .filter(|candidate| match inherit.from_axis.as_deref().unwrap_or("self") {
            "self" => {
                candidate.task_id == task.id.to_string()
                    && !(candidate.emitting_state == current_state && candidate.visit >= visit_count)
            }
            "ancestor" => ancestor_task_ids(&task.id.to_string())
                .iter()
                .any(|ancestor| ancestor == &candidate.task_id),
            "prior" => prior_sources.iter().any(|source| source == &candidate.task_id),
