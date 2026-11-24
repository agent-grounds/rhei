// Resolve recovery against the invocations the next run can actually spawn.
// §FS-rhei-retry.2 §FS-rhei-agents.8.4

/// One current-visit record of an invocation: where it is, what it says, and
/// whose it is. §FS-rhei-retry.2
type RetryRecord = (PathBuf, SpawnRecord, Option<String>);

/// The current visit's records as the next run treats them: `active` ones it
/// would spawn again, and whether any other is `reusable`, its successful work
/// reused, which a grant cannot refill. §FS-rhei-retry.2 §FS-rhei-errors.1.5.2
struct RetryRecords {
    active: Vec<RetryRecord>,
    reusable: bool,
}

/// Each active invocation keeps its own current-visit record in the runtime the
/// next run consumes. Its own-name/legacy precedence also determines the grant's
/// allowance and next-attempt output. Reusable work and removed targets get no
/// grant; reusable work is still read by the same names and precedence, so a
/// refusal never calls a record on disk missing.
/// §FS-rhei-retry.2 §FS-rhei-retry.3 §FS-rhei-agent-visit-pairing.1
fn retry_current_records(
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state: &str,
    settings: &RheiSettings,
    artifact_root: &Path,
    runtime: &Path,
) -> MietteResult<RetryRecords> {
    let definition = &machine.states[state];
    let invocations = if definition.program.is_some() {
        Vec::new()
    } else {
        resolve_agent_invocations_for_task(
            machine,
            state,
            settings,
            &default_run_options(),
            Some(task),
        )?
    };
    let task_id = task.id.to_string();
    let metadata = loaded.rhei.metadata.as_ref();
    let visit_count = render_visit_count(metadata, &task.id, state, &task.state, machine);
    let mut active: BTreeMap<Option<String>, (PathBuf, SpawnRecord)> = BTreeMap::new();
    let mut reusable = false;
    let completion = InvocationCompletion {
        artifact_root,
        runtime_dir: runtime,
        task,
        state_name: state,
        current_state_raw: &task.state,
        machine,
        metadata,
        state_def: definition,
        finishes_ticket: selected_forward_transition(&loaded.rhei, machine, task)
            .is_some_and(|to| is_terminal_state(&to, machine)),
        visit_count,
        moves: ticket_move_count(artifact_root, runtime, &task_id),
        invocations: &invocations,
        pairing: std::cell::OnceCell::new(),
    };
    // Keep the ledger's path and typed read/parse guidance. §FS-rhei-errors.6
    let stay = Stay::of(machine, state, &task.state);
    let number = resolve_log_number_report(machine, state, visit_count, artifact_root, runtime, &task_id, stay)?;
    // Each identity with whether the next run would spawn it again.
    let identities: Vec<(Option<String>, bool)> = if invocations.is_empty() {
        vec![(None, true)]
    } else {
        invocations
            .iter()
            .map(|resolved| {
                let pending =
                    definition.outputs.is_empty() || completion.invocation_is_pending(resolved);
                (resolved_agent_log_identity(resolved), pending)
            })
            .collect()
    };
    for (identity, pending) in identities {
        let names = own_spawn_record_names(runtime, &task_id, state, identity.as_deref(), number);
        // Match the run's primary-name and legacy-fallback precedence.
        let current = names
            .into_iter()
            .enumerate()
            .find_map(|(index, path)| {
                let record = read_spawn_record(&path)?;
                if index > 0
                    && (record.task != task_id
                        || record.state != state
                        || record.moves != completion.moves)
                {
                    return None;
                }
                Some((path, record))
            })
            .filter(|(_, record)| {
                record.task == task_id && record.state == state && record.moves == completion.moves
            });
        let Some((path, record)) = current else { continue };
        if pending {
            active.insert(identity, (path, record));
        } else {
            reusable |= record.proves_successful_work(&task_id, state, completion.moves);
        }
    }
    let active =
        active.into_iter().map(|(identity, (path, record))| (path, record, identity)).collect();
    Ok(RetryRecords { active, reusable })
}

/// The refusal of a retry whose current visit's work is reusable: the next run
/// spawns nothing for it, so there is nothing to refill. It names the edge that
/// work selected when its destination refuses it, and never calls the record on
/// disk missing. §FS-rhei-retry.2 §FS-rhei-errors.1.5.2
fn nothing_to_refill_refusal(
    task_id: &str,
    state: &str,
    refusal: Option<DestinationRefusal>,
) -> miette::Report {
    let help = match refusal {
        Some(refusal) => {
            let mut release = refusal.release();
            release[..1].make_ascii_uppercase();
            format!("{}. {release} and rerun, or cancel the ticket.", refusal.edge())
        }
        None => "the next run reuses that work rather than spawning this state again, so a \
                 grant would give it nothing to use."
            .to_string(),
    };
    // §FS-rhei-errors.6
    miette!(
        help = help,
        "Task {task_id} '{state}': nothing to refill: this visit's attempt finished and its work is reusable"
    )
}
