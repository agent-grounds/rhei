/// Classify the shared cache under each ticket's machine before choosing detail
/// or a member-focused summary. §FS-rhei-validate.4.7
fn snapshot_orphan_validation_warnings(
    workspace_root: &Path,
    loaded: &LoadedPlan,
    machines: &ResolvedMachineSet,
    settings: &RheiSettings,
    member: Option<&str>,
    state_machine: Option<&Path>,
) -> MietteResult<Vec<String>> {
    let cache_root = snapshot_cache_dir(settings, workspace_root);
    if !cache_root.exists() {
        return Ok(Vec::new());
    }
    let records = read_snapshot_records(&cache_root)?;
    let orphans = records.iter().filter(|record| {
        // A snapshot belongs to one ticket; judge it under that ticket's
        // machine. §DA-per-rhei-state-machines
        let machine = machines.machine_for_task_str(&record.task_id);
        snapshot_record_is_orphaned_for_loaded(record, loaded, machine, settings)
    });
    let member = member.filter(|id| {
        loaded.is_panta_project() && loaded.rhei_ids.iter().any(|rhei| rhei.as_str() == *id)
    });
    let inspect = snapshot_orphan_inspection_command(workspace_root, state_machine);
    Ok(format_snapshot_orphan_warnings(orphans, &loaded.rhei_ids, member, &inspect))
}

/// Inspect all counted origins in the same project, retaining any machine
/// override on snapshot's parent command. §FS-rhei-validate.4.7
fn snapshot_orphan_inspection_command(workspace_root: &Path, state_machine: Option<&Path>) -> String {
    format!(
        "rhei snapshot{} list --orphaned --produced-by all --plan={}",
        state_machine.map(state_machine_argument).unwrap_or_default(),
        shell_word(workspace_root),
    )
}

/// Keep own and unattributable records visible; only known other owners enter
/// the sorted counts, including tickets no longer in the graph. §FS-rhei-validate.4.7
fn format_snapshot_orphan_warnings<'a>(
    orphans: impl IntoIterator<Item = &'a SnapshotRecord>,
    rhei_ids: &[String],
    member: Option<&str>,
    inspect: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut others = BTreeMap::<&str, usize>::new();
    for record in orphans {
        let owner = record.task_id.split_once('.').and_then(|(owner, local)| {
            (!local.is_empty() && rhei_ids.iter().any(|id| id == owner)).then_some(owner)
        });
        match (member, owner) {
            (Some(member), Some(owner)) if owner != member => {
                *others.entry(owner).or_default() += 1;
            }
            _ => warnings.push(format!(
                "snapshot {} is orphaned relative to the current plan/state machine",
                record.display_ref()
            )),
        }
    }
    if !others.is_empty() {
        let total: usize = others.values().sum();
        let counts = others
            .iter()
            .map(|(id, count)| format!("{id}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        warnings.push(format!(
            "{total} orphaned snapshots belong to other rheis of this project ({counts}); `{inspect}` lists them"
        ));
    }
    warnings
}

/// Validation and inspection use one orphan classification, independently of
/// how warnings are presented. §FS-rhei-validate.4.7
fn snapshot_record_is_orphaned_for_loaded(
    record: &SnapshotRecord,
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
) -> bool {
    let task_exists =
        flatten_tasks(&loaded.rhei).into_iter().any(|task| task.id.to_string() == record.task_id);
    if !task_exists {
        return true;
    }
    if !machine.states.contains_key(&record.emitting_state) {
        return true;
    }
    let Ok(slugs) = effective_target_slugs_for_state(machine, &record.emitting_state, settings)
    else {
        return true;
    };
    slugs.is_empty() || !slugs.contains(&record.target_slug)
}

#[cfg(test)]
include!("tests_snapshot_validation_warnings.rs");
