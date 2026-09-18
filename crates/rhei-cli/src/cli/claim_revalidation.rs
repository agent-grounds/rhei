// Repeating `rhei next`'s complete claimability decision under the plan sidecar,
// and the in-place assignee write that consumes that decision.

// §AR-source-file-size.3 §FS-rhei-next.3.1

/// Project data needed to repeat the selected task's complete claimability
/// decision after its plan sidecars are locked.
struct ClaimEligibilityContext<'a> {
    input: &'a Path,
    machines: &'a ExecutionMachines,
    workspace_root: &'a Path,
    selection: ClaimSelection,
}

/// Re-load the current project and ask the same ready-set implementation that
/// selected the task whether it is still claimable. The separate input call
/// retains the specific missing-artifact diagnostic when that is what changed.
// §FS-rhei-next.3.1 §FS-rhei-next.3.3
fn ensure_claimable_under_lock(
    context: &ClaimEligibilityContext<'_>,
    qualified_id: &str,
) -> MietteResult<()> {
    let loaded = load_plan(context.input)?;
    let target_id = parse_task_id(qualified_id);
    let task = find_task_by_id(&loaded.rhei.tasks, &target_id).ok_or_else(|| {
        miette!(
            help = task_moved_help(),
            "conflict: Task {} no longer exists after re-reading the plan under lock",
            qualified_id
        )
    })?;
    let roots = ReadySetRoots {
        workspace_root: context.workspace_root,
        task_roots: &loaded.task_roots,
    };
    let still_claimable = find_claimable_tasks_for_selection(
        &loaded.rhei,
        &context.machines.set,
        &roots,
        context.selection,
    )
    .into_iter()
    .any(|candidate| candidate.id == task.id);

    let machine = context.machines.for_task(&task.id);
    let state = normalized_state_name(task.state.as_str(), machine);
    if let Some(state_def) = machine.states.get(&state) {
        let settings = load_merged_settings(context.workspace_root)?;
        let artifact_root = loaded.task_root(qualified_id, context.workspace_root);
        ensure_state_inputs_exist_for_transition(
            &artifact_root,
            Some(task),
            qualified_id,
            &state,
            state_def,
            Some(render_visit_count(
                loaded.rhei.metadata.as_ref(),
                &task.id,
                &state,
                task.state.as_str(),
                machine,
            )),
            machine,
            &settings,
            &format!("Task {} cannot be claimed in state {}.", qualified_id, state),
        )?;
    }

    if still_claimable {
        return Ok(());
    }
    Err(miette!(
        help = "another writer changed this task or its prerequisites; inspect current ready work with: rhei list <plan>",
        "conflict: Task {} is no longer claimable after re-reading the plan under lock",
        qualified_id
    ))
}

struct LockedTaskAssigneeClaimContext<'a> {
    metadata_file: &'a Path,
    eligibility: &'a ClaimEligibilityContext<'a>,
}

trait TaskAssigneeRevalidation {
    fn metadata_file<'a>(&'a self, task_file: &'a Path) -> &'a Path;

    fn before_lock(&self) {}

    fn structure(&self) -> Option<&rhei_core::ast::Structure> {
        None
    }

    fn validate(
        &self,
        task: &rhei_core::ast::Task,
        qualified_id: &str,
        current_state: &str,
        machine: &rhei_validator::StateMachine,
    ) -> MietteResult<()>;
}

impl TaskAssigneeRevalidation for LockedTaskAssigneeClaimContext<'_> {
    fn metadata_file<'a>(&'a self, _task_file: &'a Path) -> &'a Path {
        self.metadata_file
    }

    fn before_lock(&self) {
        #[cfg(test)]
        run_claim_before_lock_hook();
    }

    fn validate(
        &self,
        _task: &rhei_core::ast::Task,
        qualified_id: &str,
        _current_state: &str,
        _machine: &rhei_validator::StateMachine,
    ) -> MietteResult<()> {
        ensure_claimable_under_lock(self.eligibility, qualified_id)
    }
}

#[cfg(test)]
struct TaskAssigneeClaimContext<'a> {
    workspace_root: &'a Path,
    metadata: Option<&'a Metadata>,
    structure: Option<&'a rhei_core::ast::Structure>,
    state_def: &'a rhei_validator::StateDef,
    settings: &'a RheiSettings,
}

#[cfg(test)]
impl TaskAssigneeRevalidation for TaskAssigneeClaimContext<'_> {
    fn metadata_file<'a>(&'a self, task_file: &'a Path) -> &'a Path {
        task_file
    }

    fn structure(&self) -> Option<&rhei_core::ast::Structure> {
        self.structure
    }

    fn validate(
        &self,
        task: &rhei_core::ast::Task,
        qualified_id: &str,
        current_state: &str,
        machine: &rhei_validator::StateMachine,
    ) -> MietteResult<()> {
        ensure_state_inputs_exist_for_transition(
            self.workspace_root,
            Some(task),
            qualified_id,
            current_state,
            self.state_def,
            Some(render_visit_count(
                self.metadata,
                &parse_task_id(qualified_id),
                current_state,
                task.state.as_str(),
                machine,
            )),
            machine,
            self.settings,
            &format!("Task {} cannot be claimed in state {}.", qualified_id, current_state),
        )
    }
}

/// Atomically add the assignee after repeating the selected task's applicable
/// revalidation policy under the metadata and task-file sidecars.
// §FS-rhei-next.3.1
fn write_task_assignee(
    task_file: &Path,
    task_id: &str,
    qualified_id: &str,
    expected_state: &str,
    machine: &rhei_validator::StateMachine,
    claim: impl TaskAssigneeRevalidation,
    assignee: &str,
) -> MietteResult<()> {
    claim.before_lock();

    // Match the transition path's lock order so metadata-backed eligibility
    // (polling, visits, and supervision) is stable during the re-read too.
    let metadata_file = claim.metadata_file(task_file);
    let metadata_lock = LockedPlanFile::open(metadata_file)?;
    let task_lock = if task_file == metadata_file {
        None
    } else {
        Some(LockedPlanFile::open(task_file)?)
    };
    let raw = match task_lock.as_ref() {
        Some(lock) => lock.read_to_string("failed to read plan file")?,
        None => metadata_lock.read_to_string("failed to read plan file")?,
    };
    let structure = if task_file == metadata_file {
        claim.structure().cloned()
    } else {
        let metadata_raw = metadata_lock.read_to_string("failed to read plan file")?;
        Some(parse_metadata_manifest(metadata_file, &metadata_raw)?.structure)
    };
    let target = parse_task_id(task_id);
    let task =
        parse_claim_task_from_raw(&raw, task_file, structure.as_ref(), &target, task_id)?;
    let current_state = normalized_state_name(task.state.as_str(), machine);
    if current_state != expected_state {
        return Err(miette!(
            help = task_moved_help(),
            "conflict: Task {} is in state '{}', expected '{}'",
            qualified_id,
            task.state,
            expected_state
        ));
    }
    if let Some(existing) = task.assignee.as_deref() {
        return Err(miette!(
            help = format!(
                "release it with: rhei release {qualified_id} — or work on a different task."
            ),
            "Task {} is already assigned to {}",
            qualified_id,
            existing
        ));
    }
    claim.validate(&task, qualified_id, &current_state, machine)?;

    let rewritten = insert_task_assignee(&raw, task_id, assignee)?;
    let parent = task_file.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(help = temp_write_help(), "failed to create temp file: {err}"))?;
    tmp.write_all(rewritten.as_bytes())
        .map_err(|err| miette!(help = temp_write_help(), "failed to write temp file: {err}"))?;
    let task_lock = task_lock.as_ref().unwrap_or(&metadata_lock);
    persist_locked(tmp, task_file)
        .map_err(|err| miette!(help = temp_write_help(), "failed to persist temp file: {err}"))?;

    task_lock.release();
    metadata_lock.release();
    Ok(())
}

/// Find the task being claimed in the raw markdown just read under the lock.
/// The owning workspace's declared node kinds govern task-file parsing.
// §FS-rhei-next.3.1
fn parse_claim_task_from_raw(
    raw: &str,
    task_file: &Path,
    workspace_structure: Option<&rhei_core::ast::Structure>,
    target: &TaskId,
    task_id: &str,
) -> MietteResult<rhei_core::ast::Task> {
    if let Ok(rhei) = rhei_core::parse(raw) {
        if let Some(task) = find_task_by_id(&rhei.tasks, target) {
            return Ok(task.clone());
        }
    }

    let workspace_tasks = match workspace_structure {
        Some(structure) => rhei_core::parser::parse_workspace_tasks_with_structure(raw, structure),
        None => rhei_core::parser::parse_workspace_tasks(raw),
    };
    if let Ok(tasks) = workspace_tasks {
        if let Some(task) = find_task_by_id(&tasks, target) {
            return Ok(task.clone());
        }
    }

    Err(miette!(
        help = task_id_help(),
        "task '{}' not found in {}",
        task_id,
        task_file.display()
    ))
}
