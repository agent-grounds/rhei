// Operator recovery in place, without travel or account capacity. §FS-rhei-retry

/// Refuse worker, poll and terminal recovery before any runtime write. §FS-rhei-retry.2
fn retry_state_preflight(worker: bool, state: &rhei_validator::StateDef) -> MietteResult<()> {
    if worker {
        return Err(diagnostic!("rhei retry is refused inside a worker (RHEI_TASK_ID is set)"));
    }
    if state.terminal {
        return Err(diagnostic!("cannot retry a terminal state"));
    }
    if state.poll.is_some() {
        return Err(diagnostic!("cannot retry a poll state; polling has its own exhaustion rule"));
    }
    Ok(())
}

/// Validate the exact current visit, including grants already consumed or still
/// available; neither a stale record nor a refill stockpile is recovery. §FS-rhei-retry.2
fn retry_record_preflight(
    record: Option<&SpawnRecord>,
    task: &str,
    state: &str,
    moves: u64,
    configured: u64,
) -> MietteResult<()> {
    let record = record
        .filter(|record| record.task == task && record.state == state && record.moves == moves)
        .ok_or_else(|| {
            diagnostic!("Task {task} has no spawn record for its current visit in '{state}'")
        })?;
    let allowance = record.allowance(configured);
    if record.charged < allowance {
        return Err(diagnostic!(
            "Task {task} has {} attempts left on this visit; retry grants cannot be stockpiled",
            allowance - record.charged
        ));
    }
    Ok(())
}

/// The real grant transaction shares its lock and atomic replacement with charge.
/// A read-only preflight keeps ordinary refusals entirely write-free. §FS-rhei-retry.2
fn grant_retry_record(
    path: &Path,
    task: &str,
    state: &str,
    moves: u64,
    grant: AttemptGrant,
) -> MietteResult<SpawnRecord> {
    retry_record_preflight(read_spawn_record(path).as_ref(), task, state, moves, grant.attempts)?;
    update_spawn_record(path, |stored| {
        retry_record_preflight(stored.as_ref(), task, state, moves, grant.attempts)?;
        let mut stored = stored.expect("current record checked");
        stored.grants.push(grant);
        Ok(Some(stored))
    })?
    .ok_or_else(|| diagnostic!("spawn record disappeared during retry"))
}

/// Resolve the shared ticket/plan target and refill its exhausted active invocations.
/// No transition, visit, ledger or account write. §FS-rhei-retry.1 §FS-rhei-retry.2
fn retry_command(
    input: &Path,
    scope: &[String],
    state_machine_path: Option<&Path>,
    task_id: &str,
) -> MietteResult<()> {
    retry_command_with_worker(
        input,
        scope,
        state_machine_path,
        task_id,
        std::env::var_os("RHEI_TASK_ID").is_some(),
    )
}

/// Worker authority is resolved once at dispatch; the handler never permits a
/// worker to grant itself capacity. §FS-rhei-retry.2
fn retry_command_with_worker(
    input: &Path,
    scope: &[String],
    state_machine_path: Option<&Path>,
    task_id: &str,
    worker: bool,
) -> MietteResult<()> {
    if worker {
        return Err(diagnostic!("rhei retry is refused inside a worker (RHEI_TASK_ID is set)"));
    }
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    let scope = resolve_rhei_scope(&loaded, scope)?;
    let task_id = resolve_cli_task_id(&loaded, task_id, &scope)?;
    let route = loaded.task_route(&task_id, input);
    // Serialize with state/visit changes as well as with the record writer.
    // §FS-rhei-retry.2: a grant belongs to the state and visit that still stand.
    let _metadata_lock = LockedPlanFile::open(&route.metadata_file)?;
    let _task_lock = if route.task_file != route.metadata_file {
        Some(LockedPlanFile::open(&route.task_file)?)
    } else {
        None
    };
    let loaded = load_plan(input)?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input, &loaded)?;
    let machine = machines.for_task_str(&task_id);
    let task = find_task_by_id(&loaded.rhei.tasks, &parse_task_id(&task_id))
        .ok_or_else(|| diagnostic!("Task {task_id} not found"))?;
    let state = normalized_state_name(&task.state, machine);
    let definition =
        machine.states.get(&state).ok_or_else(|| diagnostic!("unknown state '{state}'"))?;
    retry_state_preflight(false, definition)?;
    let root = execution_workspace_root(input);
    let settings = load_merged_settings(&root)?;
    let AttemptBudget::Visit(configured) = resolve_attempt_budget(Some(definition), &settings)
    else {
        unreachable!("poll refused")
    };
    // §FS-rhei-retry.1: a live-admitted member can have local records even when
    // this fresh command loads it as an initial project member.
    let mut runtimes = vec![root.join("runtime"), route.execution_root.join("runtime")];
    runtimes.sort();
    runtimes.dedup();
    let records = retry_active_records(
        &loaded,
        machine,
        task,
        &state,
        &settings,
        &route.execution_root,
        &runtimes,
    )?;
    let exhausted = records
        .iter()
        .filter(|(_, record, _)| record.charged >= record.allowance(configured))
        .collect::<Vec<_>>();
    if exhausted.is_empty() {
        let record = records.first().map(|(_, record, _)| record);
        retry_record_preflight(
            record,
            &task_id,
            &state,
            record.map_or(0, |record| record.moves),
            configured,
        )?;
        unreachable!("no exhausted record")
    }
    let account = std::process::Command::new("whoami")
        .output()
        .map_err(|err| diagnostic!("cannot identify invoking OS account: {err}"))?;
    let os_user = String::from_utf8(account.stdout)
        .map_err(|err| diagnostic!("cannot decode OS account: {err}"))?;
    if !account.status.success() || os_user.trim().is_empty() {
        return Err(diagnostic!("cannot identify invoking OS account"));
    }
    for (path, previous, identity) in exhausted {
        let record = grant_retry_record(
            path,
            &task_id,
            &state,
            previous.moves,
            AttemptGrant {
                at: chrono::Utc::now().to_rfc3339(),
                os_user: os_user.trim().into(),
                attempts: configured,
            },
        )?;
        // §FS-rhei-retry.3: name only the invocations actually granted.
        let label = identity.as_ref().map(|id| format!(" ({id})")).unwrap_or_default();
        let mut message = format!("Task {task_id} '{state}'{label}: granted {configured} more attempts to this visit ({} in total); the next `rhei run` spawns it as attempt {}.", record.allowance(configured), record.attempt + 1);
        if execute_on_of(machine, &state).is_some() {
            let count = supervision_checkpoints(loaded.rhei.metadata.as_ref(), &task.id).len();
            message.push_str(&format!(
                " Subtree still held, {count} checkpoint{} pending.",
                if count == 1 { "" } else { "s" }
            ));
        }
        println!("{message}");
    }
    Ok(())
}
