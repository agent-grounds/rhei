// Resolve recovery against the invocations the next run can actually spawn.
// §FS-rhei-retry.2 §FS-rhei-agents.8.4

/// Each active invocation keeps its own current-visit record. Discovery across
/// project/member runtimes selects per identity, never one newest sibling for
/// the whole ticket. Finished reusable work and removed targets receive no grant.
/// §FS-rhei-retry.2 §FS-rhei-agent-visit-pairing.1
fn retry_active_records(
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    state: &str,
    settings: &RheiSettings,
    artifact_root: &Path,
    runtimes: &[PathBuf],
) -> MietteResult<Vec<(PathBuf, SpawnRecord, Option<String>)>> {
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
    for runtime in runtimes {
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
        let number =
            resolve_log_number(machine, state, visit_count, artifact_root, runtime, &task_id)
                .map_err(|message| diagnostic!("{message}"))?;
        let identities = if invocations.is_empty() {
            vec![None]
        } else {
            invocations
                .iter()
                .filter(|resolved| {
                    definition.outputs.is_empty() || completion.invocation_is_pending(resolved)
                })
                .map(resolved_agent_log_identity)
                .collect()
        };
        for identity in identities {
            let names =
                own_spawn_record_names(runtime, &task_id, state, identity.as_deref(), number);
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
                    record.task == task_id
                        && record.state == state
                        && record.moves == completion.moves
                });
            let Some((path, record)) = current else { continue };
            let replace = active.get(&identity).is_none_or(|(_, previous)| {
                record
                    .ended
                    .cmp(&previous.ended)
                    .then(record.attempt.cmp(&previous.attempt))
                    .is_gt()
            });
            if replace {
                active.insert(identity, (path, record));
            }
        }
    }
    Ok(active.into_iter().map(|(identity, (path, record))| (path, record, identity)).collect())
}
