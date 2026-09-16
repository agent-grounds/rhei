#[allow(clippy::too_many_arguments)]
fn ensure_state_inputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
    context: &str,
) -> MietteResult<()> {
    for artifact in &state_def.inputs {
        if artifact.optional {
            continue;
        }
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        );
        if artifact_relative_path_escapes_root(&relative) {
            return Err(miette!(
                help = artifact_path_help(),
                "{context}\nInput artifact '{}' expands to '{}' which escapes the workspace root",
                artifact.name,
                relative
            ));
        }
        if !path.exists() {
            // Pre-qualification artifacts are keyed by the rhei-local id;
            // when one exists, name it so the fix is one rename away. §FS-rhei-panta.6
            let local_id = rhei_local_id_str(task_id);
            let legacy_hint = if local_id != task_id && artifact.path.contains("{task_id}") {
                let (legacy_relative, legacy_path) = resolve_artifact_path(
                    workspace_root,
                    artifact,
                    local_id,
                    state_name,
                    visit_count,
                    target,
                    model,
                    model_provider,
                    model_name,
                    agent,
                    agent_mode,
                );
                if legacy_path.exists() {
                    format!(
                        "\nA pre-qualification artifact exists at '{legacy_relative}'. \
                         Ticket ids are now project-qualified; rename it to '{relative}' \
                         to keep this run's history."
                    )
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            return Err(miette!(
                help = format!(
                    "the state cannot start until that file exists. Produce it in the previous \
                     state, or mark the input `optional: true` in the state machine. Expected \
                     at: {}",
                    path.display()
                ),
                "{context}\nMissing required input artifact: {} ({}){}",
                artifact.name,
                relative,
                legacy_hint
            ));
        }
    }

    Ok(())
}

/// The required inputs of `state_def` that are not on disk, each rendered
/// `name (<absolute path>)` and carrying the same pre-qualification rename hint
/// [`ensure_state_inputs_exist`] raises.
///
/// The same files that check refuses on, as data rather than as an error: a
/// halt report has to name them, not raise them.
///
/// The path is made absolute, and spelled in the platform's own separators,
/// because the whole question a held ticket raises is *which root* the file was
/// looked for under, and `rhei run <project>` would otherwise print it relative
/// to wherever the operator stood. The missing file cannot be canonicalized —
/// that is what missing means — but the root it was looked for under exists, so
/// the root is canonicalized and the resolved relative path rejoined to it.
// §FS-rhei-states.3 §FS-rhei-run-report.3.1 §AR-rhei-panta.5 §REQ-cross-platform.5
#[allow(clippy::too_many_arguments)]
fn missing_state_inputs(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
) -> Vec<String> {
    let root = absolutize(workspace_root);
    let local_id = rhei_local_id_str(task_id);
    state_def
        .inputs
        .iter()
        .filter(|artifact| !artifact.optional)
        .filter_map(|artifact| {
            let resolve = |id: &str| {
                resolve_artifact_path(
                    workspace_root,
                    artifact,
                    id,
                    state_name,
                    visit_count,
                    target,
                    model,
                    model_provider,
                    model_name,
                    agent,
                    agent_mode,
                )
            };
            let (relative, path) = resolve(task_id);
            if path.exists() {
                return None;
            }
            // Pre-qualification artifacts are keyed by the rhei-local id; when
            // one exists the fix is a rename, and the halt line is the only
            // place the operator hears it. §FS-rhei-panta.6
            let renameable = (local_id != task_id && artifact.path.contains("{task_id}"))
                .then(|| resolve(local_id))
                .filter(|(_, legacy_path)| legacy_path.exists())
                .map(|(legacy_relative, _)| {
                    format!("; rename the pre-qualification artifact at '{legacy_relative}'")
                })
                .unwrap_or_default();
            // Component by component: an artifact template spells its path
            // with `/`, and joining that whole leaves a Windows path mixed,
            // `C:\proj\auth\runtime/brief.md`. §REQ-cross-platform.5
            let mut looked_for = root.clone();
            looked_for.extend(Path::new(&relative).components());
            Some(format!("{} ({}{})", artifact.name, looked_for.display(), renameable))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn ensure_state_outputs_exist(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    state_def: &rhei_validator::StateDef,
    visit_count: Option<u64>,
    target: Option<&ExecutionTarget>,
    model: Option<&str>,
    model_provider: Option<&str>,
    model_name: Option<&str>,
    agent: Option<&str>,
    agent_mode: Option<&str>,
    entering_final: bool,
) -> MietteResult<()> {
    for artifact in &state_def.outputs {
        let (relative, path) = resolve_artifact_path(
            workspace_root,
            artifact,
            task_id,
            state_name,
            visit_count,
            target,
            model,
            model_provider,
            model_name,
            agent,
            agent_mode,
        );
        if artifact_relative_path_escapes_root(&relative) {
            return Err(miette!(
                help = artifact_path_help(),
                "Task {} cannot leave state {}.\nOutput artifact '{}' expands to '{}' which escapes the workspace root",
                task_id,
                state_name,
                artifact.name,
                relative
            ));
        }
        if !path.exists() {
            // A caller aiming at a final state has a second way out the help
            // hides: abandon the step through its cancellation role. §FS-rhei-states.1.4
            let cancel_hint = if entering_final {
                " A transition into the reserved `cancelled` state skips this check; other abandonment terminals may declare `role: cancellation`."
            } else {
                ""
            };
            return Err(miette!(
                help = format!(
                    "the state's work is not finished until that file exists. Write it, then \
                     retry the transition. Expected at: {}{cancel_hint}",
                    path.display()
                ),
                "Task {} cannot leave state {}.\nMissing required output artifact: {} ({})",
                task_id,
                state_name,
                artifact.name,
                relative
            ));
        }
    }

    Ok(())
}

/// Execute the `transition` subcommand: atomic compare-and-swap state change.
///
/// Acquires the plan's exclusive sibling sidecar, verifies the task's current state matches
/// `from`, validates the transition against the state machine, rewrites the
/// `**State:**` line, and writes the file atomically (temp + rename).
#[allow(clippy::too_many_arguments)]
fn transition_command(
    input: &Path,
    rhei_scope: &[String],
    state_machine_path: Option<&Path>,
    task_id_str: &str,
    from: &str,
    to: &str,
    result_msg: Option<&str>,
    supervisor: Option<&str>,
    no_callbacks: bool,
) -> MietteResult<()> {
    let input_buf = normalize_workspace_input(input);
    let input = input_buf.as_path();
    let loaded = load_plan(input)?;
    // No `--rhei` on this command: the explicit ticket target is the scope,
    // narrowed by the rhei the invocation was pointed at. §FS-rhei-panta.6
    let scope = resolve_rhei_scope(&loaded, rhei_scope)?;
    let task_id_str = &resolve_cli_task_id(&loaded, task_id_str, &scope)?;
    let supervisor = supervisor
        .map(|id| resolve_cli_task_id(&loaded, id, &scope).map(|id| parse_task_id(&id)))
        .transpose()?;
    let resolved = resolve_state_machines_for_loaded_plan(input, &loaded, state_machine_path)?;
    let machines = ExecutionMachines::build(&resolved, input, &loaded)?;
    // The explicit ticket target's own machine and callback base govern.
    // §DA-per-rhei-state-machines
    let machine = machines.for_task_str(task_id_str);
    let callback_paths = machines.callbacks_for_str(task_id_str);

    // §FS-rhei-transition-cmd.2: accepting `--result ""` while ignoring it
    // would hide the exact thing §FS-rhei-states.3.3 refuses.
    let result_msg = require_non_blank_result(result_msg, "transition")?;

    let route = loaded.task_route(task_id_str, input);

    let effective_to = execute_transition_with_origin(
        TransitionFiles { task_file: &route.task_file, metadata_file: &route.metadata_file, metadata_id: &route.metadata_id, artifact_root: &route.execution_root, artifact_id: task_id_str },
        callback_paths,
        machine,
        &route.local_id,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            result_message: result_msg.map(str::to_string),
            supervisor,
            ..TransitionOrigin::default()
        },
        None,
    )?;

    println!("Task {} transitioned: '{}' → '{}'", task_id_str, from, effective_to);
    Ok(())
}

/// Reject a `--result` that carries no message.
///
/// The flag exists to record why a ticket ended where it did; taking it with an
/// empty value and moving on would write exactly the blank result the terminal
/// obligation refuses, only with the caller believing they had answered.
// §FS-rhei-transition-cmd.2 §FS-rhei-complete.4
fn require_non_blank_result<'a>(
    result_msg: Option<&'a str>,
    command: &str,
) -> MietteResult<Option<&'a str>> {
    match result_msg {
        Some(message) if message.trim().is_empty() => Err(miette!(
            help = format!(
                "say what happened in a sentence: rhei {command} … --result \"<what happened>\""
            ),
            "--result carries no message"
        )),
        other => Ok(other),
    }
}

/// Core transition logic shared by `transition` and `run` commands.
///
/// Validates states and transition legality, acquires the exclusive plan sidecar,
/// performs compare-and-swap verification, executes callbacks, and atomically
/// rewrites the plan file. Returns an error if any step fails.
///
/// `task_file` is the specific file whose sidecar is locked before rewriting (for directory
/// workspaces this is the file inside `tasks/` that contains the task;
/// for single-file plans it equals `plan_path`).
///
/// `plan_path` is the top-level plan path used in callback context.
///
/// `result_msg` is the message the caller carries into a `final: true` target.
/// It is appended to the ticket's result file once the move succeeds, and
/// satisfies the terminal-result obligation for a caller that knows the
/// outcome; `None` leaves the obligation to a result already on disk.
// §FS-rhei-states.3.3
#[allow(clippy::too_many_arguments)]
fn execute_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    result_msg: Option<&str>,
    no_callbacks: bool,
) -> MietteResult<String> {
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin {
            result_message: result_msg.map(str::to_string),
            ..TransitionOrigin::default()
        },
        None,
    )
}

/// Apply the setup edge and take ownership as one retry-safe `next` claim.
///
/// The ordinary transition wrappers deliberately cannot select this path;
/// claim rollback and its commit boundary belong only to `rhei next`.
// §FS-rhei-next.3.1
#[allow(clippy::too_many_arguments)]
fn execute_claim_transition(
    files: TransitionFiles<'_>,
    callback_paths: &CallbackPaths,
    machine: &rhei_validator::StateMachine,
    task_id_str: &str,
    from: &str,
    to: &str,
    no_callbacks: bool,
    claim: &ClaimEligibilityContext<'_>,
) -> MietteResult<String> {
    execute_transition_with_origin(
        files,
        callback_paths,
        machine,
        task_id_str,
        from,
        to,
        no_callbacks,
        TransitionOrigin { claim: true, ..TransitionOrigin::default() },
        Some(claim),
    )
}
