// Same-task state handoffs: which previous state a handoff comes from, where
// that state's handoff artifact landed, and what to say when a required one is
// missing.
//
// Its own part because resolving a handoff walks the transition ledger and the
// declaring state's output contract — a different lookup from every other
// prompt section.

// §AR-source-file-size.3 §FS-rhei-states.3.2

fn last_recorded_source_state_for_current(
    workspace_root: &Path,
    task_id: &TaskId,
    current_state: &str,
    machine: &rhei_validator::StateMachine,
) -> MietteResult<Option<String>> {
    // §FS-rhei-states.3.2: transition.previous resolves from durable task
    // transition history, which lives in the central ledger. §FS-rhei-complete.3.1
    let task_id_str = task_id.to_string();
    let mut found = None;
    // §FS-rhei-complete.3.1: shared parsing rejects contradictory exceptional pairs.
    for (entry_task, from, to) in read_ledger(workspace_root)? {
        if entry_task != task_id_str { continue; }
        let from = normalized_state_name(&from, machine);
        let to = normalized_state_name(&to, machine);
        if machine.is_valid_state(&from) && to == current_state { found = Some(from); }
    }
    Ok(found)
}

/// Reuse the source identities resolved for exclusions, including task overrides
/// and settings. Empty policies preserve the existing handoff behavior.
// §FS-rhei-plan-language.3.13 §FS-rhei-states.3.2
fn source_handoff_contexts<'a>(
    context: &'a RuntimeTemplateContext<'_>,
    source_def: &'a rhei_validator::StateDef,
    source_state: &str,
) -> Vec<TransitionInvocationContext<'a>> {
    let invocations = context.memory
        .and_then(|memory| memory.exclusions.invocations.get(source_state))
        .map(Vec::as_slice).unwrap_or_default();
    transition_contexts_for_state(source_def, invocations)
}

/// Resolve one handoff artifact path under a single execution identity.
fn resolve_source_handoff_path(
    render_context: &RuntimeTemplateContext<'_>,
    artifact: &rhei_validator::StateArtifactDef,
    source_state: &str,
    visit_count: Option<u64>,
    identity: &TransitionInvocationContext<'_>,
) -> (String, PathBuf) {
    let (target, model, model_provider, model_name, agent, agent_mode) = *identity;
    resolve_artifact_path(
        render_context.workspace_root,
        artifact,
        &render_context.task.id.to_string(),
        source_state,
        visit_count,
        target,
        model,
        model_provider,
        model_name,
        agent,
        agent_mode,
    )
}

/// Every path the source state's handoff artifact could occupy, for the error
/// message when none of them holds content.
fn source_handoff_candidate_paths(
    render_context: &RuntimeTemplateContext<'_>,
    source_def: &rhei_validator::StateDef,
    artifact: &rhei_validator::StateArtifactDef,
    source_state: &str,
    visit_count: Option<u64>,
) -> Vec<String> {
    source_handoff_contexts(render_context, source_def, source_state)
        .iter()
        .map(|identity| {
            let (_, path) = resolve_source_handoff_path(
                render_context,
                artifact,
                source_state,
                visit_count,
                identity,
            );
            format!("  {}", path.display())
        })
        .collect()
}

/// Read a source state's handoff artifact, returning its trimmed content.
///
/// The path is resolved under the identities the **source** state declares,
/// not the successor's: a handoff path may template `{model}`, `{agent}`, or
/// `{target}`, and those belong to the invocation that wrote the file. A state
/// that fans out over `all_models` leaves one artifact per model, so each
/// candidate is tried in declaration order.
///
/// An empty artifact counts as no handoff. The `outputs:` contract is
/// existence-only, so an agent that creates the file and writes nothing would
/// otherwise hand its successor silence that looks exactly like success.
// §FS-rhei-states.3.2: a handoff resolves under the source state's identity.
fn read_source_state_handoff(
    render_context: &RuntimeTemplateContext<'_>,
    source_def: &rhei_validator::StateDef,
    artifact: &rhei_validator::StateArtifactDef,
    source_state: &str,
    visit_count: Option<u64>,
) -> MietteResult<Option<String>> {
    for identity in source_handoff_contexts(render_context, source_def, source_state) {
        let (_, path) = resolve_source_handoff_path(
            render_context,
            artifact,
            source_state,
            visit_count,
            &identity,
        );
        if !path.exists() {
            continue;
        }
        if !prompt_source_allowed(render_context, &path) {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|err| file_io_report(&path, "failed to read state handoff", err))?;
        if content.trim().is_empty() {
            continue;
        }
        return Ok(Some(content.trim().to_string()));
    }
    Ok(None)
}

fn resolve_state_handoff_sections(
    render_context: &RuntimeTemplateContext<'_>,
) -> MietteResult<Vec<PromptHandoffSection>> {
    // §FS-rhei-states.3.2: state handoffs are explicit output artifacts injected as context.
    let Some(state_def) = render_context.machine.states.get(render_context.state_name) else {
        return Ok(Vec::new());
    };
    let Some(handoff) = state_def.handoff.as_ref() else {
        return Ok(Vec::new());
    };

    let mut sections = Vec::new();
    for inherit in &handoff.inherit {
        if inherit.from_axis != "transition.previous" {
            continue;
        }
        let Some(source_state) = last_recorded_source_state_for_current(
            render_context.workspace_root,
            &render_context.task.id,
            render_context.state_name,
            render_context.machine,
        )?
        else {
            if inherit.required {
                return Err(miette!(
                    help = handoff_missing_source_help(),
                    "state '{}' requires a handoff from the previous transition, but no transition into this state was recorded for task {}",
                    render_context.state_name,
                    render_context.task.id
                ));
            }
            continue;
        };
        let Some(source_def) = render_context.machine.states.get(&source_state) else {
            if inherit.required {
                return Err(miette!(
                    help = handoff_missing_source_help(),
                    "state '{}' requires a handoff from previous state '{}', but that state is not in the machine",
                    render_context.state_name,
                    source_state
                ));
            }
            continue;
        };
        let mut artifacts = source_def
            .outputs
            .iter()
            .filter(|artifact| artifact.kind.as_deref() == Some("handoff"))
            .filter(|artifact| inherit.name.as_ref().is_none_or(|name| &artifact.name == name))
            .collect::<Vec<_>>();

        if artifacts.is_empty() {
            if inherit.required {
                return Err(miette!(
                    help = handoff_no_output_help(),
                    "state '{}' requires a handoff from previous state '{}', but no matching handoff output was declared",
                    render_context.state_name,
                    source_state
                ));
            }
            continue;
        }
        if artifacts.len() > 1 && inherit.merge.as_deref() != Some("all") {
            return Err(miette!(
                help = handoff_ambiguous_help(),
                "state '{}' handoff from previous state '{}' is ambiguous; select a name or set merge: all",
                render_context.state_name,
                source_state
            ));
        }

        for artifact in artifacts.drain(..) {
            let source_visit_count = Some(render_visit_count(
                render_context.metadata,
                &render_context.task.id,
                &source_state,
                &source_state,
                render_context.machine,
            ));
            let Some(content) = read_source_state_handoff(
                render_context,
                source_def,
                artifact,
                &source_state,
                source_visit_count,
            )?
            else {
                if inherit.required {
                    return Err(miette!(
                        help = handoff_empty_artifact_help(),
                        "state '{}' requires handoff '{}' from previous state '{}', but no \
                         handoff artifact with content was found. Looked for:\n{}",
                        render_context.state_name,
                        artifact.name,
                        source_state,
                        source_handoff_candidate_paths(
                            render_context,
                            source_def,
                            artifact,
                            &source_state,
                            source_visit_count,
                        )
                        .join("\n")
                    ));
                }
                continue;
            };
            sections.push(PromptHandoffSection { source_state: source_state.clone(), content });
        }
    }
    Ok(sections)
}
