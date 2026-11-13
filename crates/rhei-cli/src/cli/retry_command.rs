// Operator recovery in place, without travel or account capacity. §FS-rhei-retry

/// Refuse worker and terminal recovery before any runtime write; a poll state is
/// refused once its records show no visit its kills halted. §FS-rhei-retry.2
fn retry_state_preflight(worker: bool, state: &rhei_validator::StateDef) -> MietteResult<()> {
    if worker {
        // §FS-rhei-errors.1.2
        return Err(miette!(help = WORKER_RETRY_HANDOFF_HELP, "rhei retry is refused inside a worker (RHEI_TASK_ID is set)"));
    }
    if state.terminal {
        // §FS-rhei-errors.1.2
        return Err(miette!(help = "an authorized human operator must reopen the finished task through rhei transition --force at an interactive terminal with fresh typed confirmation and a reason before retry can apply. An agent may not perform that ceremony.", "cannot retry a terminal state"));
    }
    Ok(())
}

/// The refusal of a poll state none of whose visits its kills halted.
/// §FS-rhei-retry.2 §FS-rhei-errors.6
fn poll_retry_refusal() -> miette::Report {
    miette!(help = format!("use the poll state's declared outgoing transition when polling is exhausted; {} shows the available edges. retry cannot refill poll attempts.", help_command_or_bare("states")), "cannot retry a poll state; polling has its own exhaustion rule")
}

/// Grant a poll visit its kills halted one configured budget: each granted
/// attempt allows one more kill. Re-checked under the record lock, as a charge
/// and a grant are. §FS-rhei-retry.2 §FS-rhei-agents.3.2.3
fn grant_kill_retry_record(
    path: &Path,
    task: &str,
    state: &str,
    moves: u64,
    grant: AttemptGrant,
) -> MietteResult<SpawnRecord> {
    update_spawn_record(path, |stored| {
        let current = stored.filter(|record| {
            record.task == task && record.state == state && record.moves == moves
        });
        let Some(mut record) = current.filter(SpawnRecord::kills_spent) else {
            return Err(poll_retry_refusal());
        };
        record.grants.push(grant);
        Ok(Some(record))
    })?
    // §FS-rhei-errors.6
    .ok_or_else(|| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "spawn record disappeared during retry"))
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
            // §FS-rhei-errors.6
            miette!(help = format!("run the task first with {}; retry can refill only an existing exhausted invocation of the current visit.", help_command("run")), "Task {task} has no spawn record for its current visit in '{state}'")
        })?;
    let allowance = record.allowance(configured);
    if record.charged < allowance {
        // §FS-rhei-errors.6
        return Err(miette!(
            help = format!("resume the run to use the remaining attempts: {}; request retry only after this visit is exhausted.", help_command("run")),
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
    // §FS-rhei-errors.6
    .ok_or_else(|| miette!(help = INTERNAL_DIAGNOSTIC_HELP, "spawn record disappeared during retry"))
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
        // §REQ-test-isolation.7: read through the one reader of rhei's own variables.
        inherited_rhei_var_os("RHEI_TASK_ID").is_some(),
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
        // §FS-rhei-errors.1.2
        return Err(miette!(help = WORKER_RETRY_HANDOFF_HELP, "rhei retry is refused inside a worker (RHEI_TASK_ID is set)"));
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
        // §FS-rhei-errors.6
        .ok_or_else(|| miette!(help = format!("find the current ticket id with {}; the task may have changed while acquiring its lock.", help_command("list")), "Task {task_id} not found"))?;
    let state = normalized_state_name(&task.state, machine);
    let definition =
        // §FS-rhei-errors.6
        machine.states.get(&state).ok_or_else(|| miette!(help = format!("use {} to list valid states, then correct the task's **State:** in its plan to a declared state.", help_command_or_bare("states")), "unknown state '{state}'"))?;
    retry_state_preflight(false, definition)?;
    let root = execution_workspace_root(input);
    let settings = load_merged_settings(&root)?;
    // A poll visit is refilled only once its kills halted it. §FS-rhei-retry.2
    let poll = definition.poll.is_some();
    let configured = configured_attempts(Some(definition), &settings);
    // §FS-rhei-retry.2: match the next fresh run, whose agents are initial members;
    // programs always use their owning root. Dormant duplicates cannot grant capacity.
    let runtime = if definition.program.is_some() {
        route.execution_root.join("runtime")
    } else {
        root.join("runtime")
    };
    let records = retry_active_records(
        &loaded,
        machine,
        task,
        &state,
        &settings,
        &route.execution_root,
        &runtime,
    )?;
    let exhausted = records
        .iter()
        .filter(|(_, record, _)| match poll {
            true => record.kills_spent(),
            false => record.charged >= record.allowance(configured),
        })
        .collect::<Vec<_>>();
    if exhausted.is_empty() && poll {
        return Err(poll_retry_refusal());
    }
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
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = OS_ACCOUNT_HELP, "cannot identify invoking OS account: {err}"))?;
    let os_user = String::from_utf8(account.stdout)
        // §FS-rhei-errors.6
        .map_err(|err| miette!(help = OS_ACCOUNT_HELP, "cannot decode OS account: {err}"))?;
    if !account.status.success() || os_user.trim().is_empty() {
        // §FS-rhei-errors.6
        return Err(miette!(help = OS_ACCOUNT_HELP, "cannot identify invoking OS account"));
    }
    for (path, previous, identity) in exhausted {
        let grant = AttemptGrant {
            at: chrono::Utc::now().to_rfc3339(),
            os_user: os_user.trim().into(),
            attempts: configured,
        };
        let record = match poll {
            true => grant_kill_retry_record(path, &task_id, &state, previous.moves, grant)?,
            false => grant_retry_record(path, &task_id, &state, previous.moves, grant)?,
        };
        // §FS-rhei-retry.3: name only the invocations actually granted.
        let label = identity.as_ref().map(|id| format!(" ({id})")).unwrap_or_default();
        let total = match poll {
            // A poll visit's grants allow kills, not attempts. §FS-rhei-agents.3.2.3
            true => format!("{} kills together with its run in total", record.allowance(FREE_KILLS_PER_VISIT)),
            false => format!("{} in total", record.allowance(configured)),
        };
        let mut message = format!("Task {task_id} '{state}'{label}: granted {configured} more attempts to this visit ({total}); the next `rhei run` spawns it as attempt {}.", record.attempt + 1);
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
