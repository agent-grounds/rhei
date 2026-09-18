
/// Orchestration hook for snapshot emission, invoked after the orchestrator
/// has selected the outgoing transition but before the transition is applied.
///
/// Per the run execution loop and snapshot emit contract, this is where the
/// orchestrator writes auto-emitted `_state` snapshots and any
/// matching named `snapshot.emit:` for agent-bearing states with supported
/// snapshot sessions. Poll self-loop attempts must not emit because they keep
/// the state visit open; only terminal poll exits emit.
// §FS-rhei-run.3 §FS-rhei-snapshots.10.2: Emit snapshots after transition selection.
///
/// The actual snapshot writes are owned by the impl-rhei-snapshots task; this
/// function is a deliberate no-op stub that pins the call site so the
/// orchestration ordering in rhei-run is encoded in code, not just in the
/// spec text. Once impl-rhei-snapshots delivers the snapshot module, the body
/// of this function calls into that module.
fn emit_snapshots_after_transition_selection(
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    selected_to_state: &str,
) {
    let _ = (machine, task, current_state, selected_to_state);
    // Suppression rule for poll self-loop attempts is honored by the snapshot
    // module by inspecting (current_state, selected_to_state); the call site
    // §FS-rhei-run.5.1 §FS-rhei-snapshots.10.3: Poll self-loops suppress emit.
}

#[allow(clippy::too_many_arguments)]
fn emit_snapshots_after_agent_exit(
    workspace_root: &Path,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    task: &rhei_core::ast::Task,
    current_state: &str,
    selected_to_state: Option<&str>,
    resolved: &ResolvedAgent,
    log_path: &Path,
    visit_count: u64,
    completion: SnapshotCompletion,
    preload: &SnapshotPreload,
) -> MietteResult<()> {
    let Some(state_def) = machine.states.get(current_state) else {
        return Ok(());
    };
    if state_def.terminal || state_def.gating || state_def.program.is_some() {
        return Ok(());
    }
    let emit = state_def.snapshot.as_ref().and_then(|snapshot| snapshot.emit.as_ref());
    let should_emit_named = emit.is_some_and(|emit| {
        let policy = emit.on.as_deref().unwrap_or("success");
        match policy {
            "success" => completion == SnapshotCompletion::Success,
            "failure" => {
                matches!(completion, SnapshotCompletion::Failure | SnapshotCompletion::Timeout)
            }
            "always" => true,
            _ => false,
        }
    });
    if selected_to_state == Some(current_state) && state_def.poll.is_some() {
        return Ok(());
    }
    let Some(target_slug) = resolved_agent_target_slug(resolved) else {
        if emit.is_some() {
            return Err(miette!(
                help = snapshot_key_help(),
                "snapshot-requires-target: agent '{}' does not resolve provider and model",
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "info: auto snapshot skipped for state '{}' because agent '{}' does not resolve provider and model",
            current_state,
            resolved.agent.id()
        );
        return Ok(());
    };
    let target_selector = snapshot_target_selector(resolved);
    let Some(session) = snapshot_session(resolved) else {
        if emit.is_some() {
            return Err(miette!(
                help = session_capture_help(),
                "unsupported-snapshot-session: state '{}' declares snapshot.emit but agent '{}' has no supported snapshot session layout",
                current_state,
                resolved.agent.id()
            ));
        }
        return Ok(());
    };
    let Some(layout) = snapshot_session_layout(session) else {
        if emit.is_some() {
            return Err(miette!(
                help = session_capture_help(),
                "unsupported-snapshot-session: state '{}' declares snapshot.emit but agent '{}' has no supported snapshot session layout",
                current_state,
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "info: auto snapshot skipped for state '{}' because agent '{}' has no supported snapshot session layout",
            current_state,
            resolved.agent.id()
        );
        return Ok(());
    };
    if !snapshot_emit_session_supported(session) {
        if emit.is_some() {
            return Err(miette!(
                help = session_capture_help(),
                "unsupported-snapshot-session: state '{}' declares snapshot.emit but agent '{}' has no supported snapshot session layout",
                current_state,
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "info: auto snapshot skipped for state '{}' because agent '{}' has no supported snapshot session layout",
            current_state,
            resolved.agent.id()
        );
        return Ok(());
    }
    let Some(session_layout) = snapshot_layout_manifest(session) else {
        return Err(miette!(
            help = session_capture_help(),
            "unsupported-snapshot-session: agent '{}' has an incomplete snapshot session layout",
            resolved.agent.id()
        ));
    };
    let Some((transcript_source, transcript_ext, session_id)) =
        transcript_source_for_snapshot(preload, layout)
    else {
        if should_emit_named {
            return Err(miette!(
                help = session_capture_help(),
                "unsupported-snapshot-session: state '{}' declares snapshot.emit but agent '{}' did not produce a supported native session transcript",
                current_state,
                resolved.agent.id()
            ));
        }
        diag_warn!(
            "info: auto snapshot skipped for state '{}' because agent '{}' did not produce a supported native session transcript",
            current_state,
            resolved.agent.id()
        );
        return Ok(());
    };
    let cache_root = snapshot_cache_dir(settings, workspace_root);
    let (observed_provider, observed_model) =
        observed_snapshot_target(resolved, &transcript_source, &transcript_ext);

    write_snapshot_generation_atomic(
        &cache_root,
        workspace_root,
        settings,
        &task.id.to_string(),
        "_state",
        current_state,
        visit_count,
        &target_slug,
        &target_selector,
        resolved,
        session_layout.clone(),
        &session_id,
        &transcript_source,
        &transcript_ext,
        &observed_provider,
        &observed_model,
        preload.parent_ref.as_ref(),
        completion,
        SnapshotProducedBy::Orchestrator,
        Some(log_path),
    )?;

    let Some(emit) = emit else {
        return Ok(());
    };
    if !should_emit_named {
        return Ok(());
    }

    write_snapshot_generation_atomic(
        &cache_root,
        workspace_root,
        settings,
        &task.id.to_string(),
        &emit.name,
        current_state,
        visit_count,
        &target_slug,
        &target_selector,
        resolved,
        session_layout,
        &session_id,
        &transcript_source,
        &transcript_ext,
        &observed_provider,
        &observed_model,
        preload.parent_ref.as_ref(),
        completion,
        SnapshotProducedBy::Orchestrator,
        Some(log_path),
    )?;
    Ok(())
}

fn snapshot_record_native_compatible(record: &SnapshotRecord, resolved: &ResolvedAgent) -> bool {
    snapshot_record_native_incompatibility(record, resolved).is_none()
}

fn snapshot_record_native_incompatibility(
    record: &SnapshotRecord,
    resolved: &ResolvedAgent,
) -> Option<String> {
    let manifest_agent = record
        .manifest
        .get("target")
        .and_then(|target| target.get("resolved"))
        .and_then(|resolved| resolved.get("agent"))
        .and_then(serde_json::Value::as_str);
    if manifest_agent != Some(resolved.agent.id()) {
        return Some(format!(
            "stored agent {}; current agent {}",
            manifest_agent.unwrap_or("<missing>"),
            resolved.agent.id()
        ));
    }
    let Some(session) = snapshot_session(resolved) else {
        return Some("current agent has no snapshot session profile".to_string());
    };
    let Some(inheritor_layout) = snapshot_layout_manifest(session) else {
        return Some("current agent has no supported session layout".to_string());
    };
    let snapshot_layout = record.manifest.get("session_layout").unwrap_or(&serde_json::Value::Null);
    if !snapshot_layout_matches(snapshot_layout, &inheritor_layout) {
        return Some(format!(
            "stored layout {}; current profile expects {}",
            snapshot_layout_label(snapshot_layout),
            snapshot_layout_label(&inheritor_layout)
        ));
    }
    None
}

fn snapshot_layout_label(layout: &serde_json::Value) -> String {
    let kind = snapshot_layout_kind(layout).unwrap_or_else(|| "unknown".to_string());
    let ext = snapshot_layout_ext(layout).unwrap_or_else(|| "unknown".to_string());
    let mut label = format!("{kind}/{ext}");
    if let Some(root_template) = layout.get("root_template").and_then(serde_json::Value::as_str) {
        label.push_str(&format!(" root_template={root_template}"));
    }
    if let Some(project_hash) = layout.get("project_hash").and_then(serde_json::Value::as_str) {
        label.push_str(&format!(" project_hash={project_hash}"));
    };
    label
}

fn snapshot_layout_matches(
    snapshot_layout: &serde_json::Value,
    inheritor_layout: &serde_json::Value,
) -> bool {
    let snapshot_kind = snapshot_layout_kind(snapshot_layout);
    let inheritor_kind = snapshot_layout_kind(inheritor_layout);
    if snapshot_kind != inheritor_kind {
        return false;
    }
    match snapshot_kind.as_deref() {
        Some("FlatById") => {
            snapshot_layout_ext(snapshot_layout) == snapshot_layout_ext(inheritor_layout)
        }
        Some("PerProjectJson") => {
            snapshot_layout_ext(snapshot_layout) == snapshot_layout_ext(inheritor_layout)
                && snapshot_layout.get("root_template") == inheritor_layout.get("root_template")
                && snapshot_layout.get("project_hash") == inheritor_layout.get("project_hash")
        }
        _ => false,
    }
}

fn snapshot_cache_benefit_reason(
    record: &SnapshotRecord,
    resolved: &ResolvedAgent,
) -> Option<String> {
    let observed_provider =
        record.manifest.get("observed_provider").and_then(serde_json::Value::as_str);
    let observed_model = record.manifest.get("observed_model").and_then(serde_json::Value::as_str);
    if observed_provider != resolved.model_provider.as_deref() {
        return Some("provider mismatch".to_string());
    }
    if observed_model != resolved.model_name.as_deref().or(resolved.model.as_deref()) {
        return Some("model mismatch".to_string());
    }
    None
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn resolve_inherit_snapshot_source(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
) -> MietteResult<Option<SnapshotRecord>> {
    resolve_inherit_snapshot_source_with_prior(
        cache_root,
        task,
        current_state,
        inherit,
        target_slug,
        visit_count,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_inherit_snapshot_source_with_prior(
    cache_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    visit_count: u64,
    prior_sources: &[String],
) -> MietteResult<Option<SnapshotRecord>> {
    let records = read_snapshot_records(cache_root)?
        .into_iter()
        .filter(|record| record.snapshot_name == inherit.name)
        .filter(|record| record.produced_by == "orchestrator")
        .collect::<Vec<_>>();
    let selected_state = inherit.select.as_ref().and_then(|select| select.state.as_deref());
    let selected_target = inherit.select.as_ref().and_then(|select| select.target.as_deref());
    let selected_visit = inherit.select.as_ref().and_then(|select| select.visit.as_ref());
    let selected_generation = inherit.select.as_ref().and_then(|select| select.generation.as_ref());

    if inherit.from_axis.as_deref() == Some("prior") {
        return resolve_prior_inherit_snapshot_source(
            records,
            inherit,
            target_slug,
            prior_sources,
            selected_state,
            selected_target,
            selected_visit,
            selected_generation,
        );
    }

    let mut scoped = match inherit.from_axis.as_deref().unwrap_or("self") {
        "self" => records
            .into_iter()
            .filter(|record| record.task_id == task.id.to_string())
            .filter(|record| {
                !(record.emitting_state == current_state && record.visit >= visit_count)
            })
            .collect::<Vec<_>>(),
        "ancestor" => {
            let mut ancestor_matches = Vec::new();
            for ancestor in ancestor_task_ids(&task.id.to_string()) {
                let matches = records
                    .iter()
                    .filter(|record| record.task_id == ancestor)
                    .filter(|record| {
                        selected_state.is_none_or(|state| record.emitting_state == state)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if !matches.is_empty() {
                    ancestor_matches = matches;
                    break;
                }
            }
            ancestor_matches
        }
        _ => Vec::new(),
    };
    scoped.retain(|record| selected_state.is_none_or(|state| record.emitting_state == state));
    scoped.retain(|record| match selected_target {
        Some("same") => record.target_slug == target_slug,
        Some(target) => record.target_slug == target,
        None => true,
    });
    if selected_target.is_none() {
        let targets = scoped.iter().map(|record| &record.target_slug).collect::<BTreeSet<_>>();
        if targets.len() > 1 {
            return Err(miette!(
                help = snapshot_ambiguous_help(),
                "ambiguous-lineage: snapshot.inherit '{}' matched multiple targets; add snapshot.inherit.select.target",
                inherit.name
            ));
        }
    }
    match selected_visit {
        Some(value) if yaml_selector_u64(value).is_some() => {
            let visit = yaml_selector_u64(value).unwrap_or(1);
            scoped.retain(|record| record.visit == visit);
        }
        Some(value) if yaml_selector_string(value) == Some("latest") => {
            if let Some(max_visit) = scoped.iter().map(|record| record.visit).max() {
                scoped.retain(|record| record.visit == max_visit);
            }
        }
        None => {
            if let Some(max_visit) = scoped.iter().map(|record| record.visit).max() {
                scoped.retain(|record| record.visit == max_visit);
            }
        }
        _ => {}
    }
    match selected_generation {
        Some(value) if yaml_selector_u64(value).is_some() => {
            let generation = yaml_selector_u64(value).unwrap_or(1);
            scoped.retain(|record| record.generation == generation);
        }
        Some(value) if yaml_selector_string(value) == Some("latest") => {
            if let Some(max_generation) = scoped.iter().map(|record| record.generation).max() {
                scoped.retain(|record| record.generation == max_generation);
            }
        }
        Some(value) if yaml_selector_string(value) == Some("current") => {
            scoped.retain(|record| record.is_current);
        }
        None => scoped.retain(|record| record.is_current),
        _ => {}
    }
    match scoped.len() {
        0 => Ok(None),
        1 => Ok(scoped.into_iter().next()),
        _ => Err(miette!(
            help = snapshot_ambiguous_help(),
            "ambiguous-lineage: snapshot.inherit '{}' matched multiple cached generations",
            inherit.name
        )),
    }
}

/// Reduce each declared predecessor independently before deciding whether the
/// cross-source result is missing, unique, or ambiguous.
// §FS-rhei-snapshots.4.3: Prior lineage never uses a global-newest tie-break.
#[allow(clippy::too_many_arguments)]
fn resolve_prior_inherit_snapshot_source(
    records: Vec<SnapshotRecord>,
    inherit: &rhei_validator::SnapshotInheritConfig,
    target_slug: &str,
    prior_sources: &[String],
    selected_state: Option<&str>,
    selected_target: Option<&str>,
    selected_visit: Option<&serde_yaml::Value>,
    selected_generation: Option<&serde_yaml::Value>,
) -> MietteResult<Option<SnapshotRecord>> {
    let mut survivors = Vec::new();
    for source in prior_sources {
        let mut scoped = records
            .iter()
            .filter(|record| record.task_id == *source)
            .filter(|record| selected_state.is_none_or(|state| record.emitting_state == state))
            .filter(|record| match selected_target {
                Some("same") => record.target_slug == target_slug,
                Some(target) => record.target_slug == target,
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();

        if selected_target.is_none() {
            let targets = scoped.iter().map(|record| &record.target_slug).collect::<BTreeSet<_>>();
            if targets.len() > 1 {
                survivors.extend(scoped);
                continue;
            }
        }

        match selected_visit {
            Some(value) if yaml_selector_u64(value).is_some() => {
                let visit = yaml_selector_u64(value).unwrap_or(1);
                scoped.retain(|record| record.visit == visit);
            }
            Some(value) if yaml_selector_string(value) == Some("latest") => {
                if let Some(latest) = scoped.iter().map(|record| record.visit).max() {
                    scoped.retain(|record| record.visit == latest);
                }
            }
            None => {
                if let Some(latest) = scoped.iter().map(|record| record.visit).max() {
                    scoped.retain(|record| record.visit == latest);
                }
            }
            _ => {}
        }
        match selected_generation {
            Some(value) if yaml_selector_u64(value).is_some() => {
                let generation = yaml_selector_u64(value).unwrap_or(1);
                scoped.retain(|record| record.generation == generation);
            }
            Some(value) if yaml_selector_string(value) == Some("latest") => {
                if let Some(latest) = scoped.iter().map(|record| record.generation).max() {
                    scoped.retain(|record| record.generation == latest);
                }
            }
            Some(value) if yaml_selector_string(value) == Some("current") => {
                scoped.retain(|record| record.is_current);
            }
            None => {
                scoped.retain(|record| record.is_current);
            }
            _ => {}
        }
        survivors.extend(scoped);
    }

    match survivors.len() {
        0 => Ok(None),
        1 => Ok(survivors.pop()),
        _ => {
            survivors.sort_by(|a, b| a.task_id.cmp(&b.task_id).then_with(|| a.display_ref().cmp(&b.display_ref())));
            let competing = survivors
                .iter()
                .map(|record| format!("{} ({})", record.task_id, record.display_ref()))
                .collect::<Vec<_>>()
                .join(", ");
            Err(miette!(
                help = snapshot_ambiguous_help(),
                "ambiguous-lineage: snapshot.inherit '{}' matched multiple declared Prior sources: {}",
                inherit.name,
                competing
            ))
        }
    }
}

fn ancestor_task_ids(task_id: &str) -> Vec<String> {
    let mut parts = task_id.split('.').collect::<Vec<_>>();
    let mut ancestors = Vec::new();
    while parts.len() > 1 {
        parts.pop();
        ancestors.push(parts.join("."));
    }
    ancestors
}
