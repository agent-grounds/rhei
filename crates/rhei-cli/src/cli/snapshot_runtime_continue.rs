// State-local snapshot continuation is kept beside the general named preload
// pipeline so the exact implicit selector and its degradable outcomes remain a
// bounded unit. §FS-rhei-snapshots.4.7

/// Select and stage the exact previous visit's orchestrator-owned auto
/// snapshot. Every failure is local to this optional request and therefore
/// returns the otherwise usable cold-spawn carrier.
// §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.10.3
#[allow(clippy::too_many_arguments)]
fn preload_state_session_continuation(
    mut preload: SnapshotPreload,
    workspace_root: &Path,
    task: &rhei_core::ast::Task,
    current_state: &str,
    resolved: &ResolvedAgent,
    settings: &RheiSettings,
    visit_count: u64,
    target_slug: &str,
) -> MietteResult<SnapshotPreload> {
    if visit_count <= 1 {
        diag_warn!(
            "info: session continuation starts cold for state '{}': no previous visit exists; running cold",
            current_state
        );
        return Ok(preload);
    }

    let Some(session) = snapshot_session(resolved) else {
        diag_warn!(
            "warning: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    };
    if !snapshot_preload_session_supported(session) {
        diag_warn!(
            "warning: agent '{}' has no supported snapshot preload strategy; running cold",
            resolved.agent.id()
        );
        return Ok(preload);
    }

    let previous_visit = visit_count - 1;
    let cache_root = snapshot_cache_dir(settings, workspace_root);
    let source = match read_state_continuation_source(
        &cache_root,
        &task.id.to_string(),
        current_state,
        previous_visit,
        target_slug,
    ) {
        Ok(Some(source)) => source,
        Ok(None) => {
            diag_warn!(
                "warning: no current snapshot found for state '{}' visit {} target '{}'; running cold",
                current_state,
                previous_visit,
                target_slug
            );
            return Ok(preload);
        }
        Err(err) => {
            diag_warn!(
                "warning: previous session snapshot is unreadable ({}); running cold",
                err
            );
            return Ok(preload);
        }
    };
    if source.completion == "timeout" {
        diag_warn!(
            "warning: timed-out snapshot {} is not preloadable; running cold",
            source.display_ref()
        );
        return Ok(preload);
    }
    if let Some(reason) = snapshot_record_native_incompatibility(&source, resolved) {
        diag_warn!(
            "warning: preload skipped: incompatible snapshot {} ({}); running cold",
            source.display_ref(),
            reason
        );
        return Ok(preload);
    }

    let transcript_path = source.transcript_path();
    if let Err(err) = fs::File::open(&transcript_path) {
        diag_warn!(
            "warning: snapshot transcript '{}' is unreadable ({}); running cold",
            transcript_path.display(),
            err
        );
        return Ok(preload);
    }
    if let Some(session_dir) = preload.session_dir.as_ref() {
        let ext = source
            .manifest
            .get("session_layout")
            .and_then(snapshot_layout_ext)
            .unwrap_or_else(|| "jsonl".to_string());
        let target = session_dir.join(format!(
            "{}.{}",
            source
                .manifest
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("source"),
            ext
        ));
        if let Err(err) = fs::copy(&transcript_path, &target) {
            diag_warn!(
                "warning: snapshot transcript '{}' is unreadable ({}); running cold",
                transcript_path.display(),
                err
            );
            return Ok(preload);
        }
    }

    if let Some(flag) = snapshot_strategy_flag(session, "fork") {
        preload.extra_args.push(flag);
        preload.extra_args.push(transcript_path.display().to_string());
    } else if let Some(flag) = snapshot_strategy_flag(session, "resume") {
        let session_id = source
            .manifest
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        preload.extra_args.push(flag);
        preload.extra_args.push(session_id.to_string());
    }
    if let Some(reason) = snapshot_cache_benefit_reason(&source, resolved) {
        diag_warn!(
            "info: snapshot {} is native-compatible but may not be cache-beneficial: {}",
            source.display_ref(),
            reason
        );
    }
    preload.parent_ref = Some(snapshot_parent_ref(&source));
    Ok(preload)
}

/// Read only the identity selected by state-local continuation, following its
/// live pointer directly so an unrelated corrupt cache entry cannot poison an
/// otherwise usable continuation.
// §FS-rhei-snapshots.4.7 §FS-rhei-snapshots.7
fn read_state_continuation_source(
    cache_root: &Path,
    task_id: &str,
    state_name: &str,
    visit: u64,
    target_slug: &str,
) -> MietteResult<Option<SnapshotRecord>> {
    let identity_dir = cache_root
        .join(task_id)
        .join("_state")
        .join(state_name)
        .join(visit.to_string())
        .join(target_slug);
    let Some(current) = snapshot_current_target(&identity_dir) else {
        return Ok(None);
    };
    let generation_dir = if current.is_absolute() { current } else { identity_dir.join(current) };
    let manifest_path = generation_dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path)
        .map_err(|err| file_io_report(&manifest_path, "failed to read snapshot manifest", err))?;
    let manifest: serde_json::Value = serde_json::from_str(&raw).map_err(|err| {
        miette!(
            help = snapshot_corrupt_help(),
            "failed to parse snapshot manifest '{}': {err}",
            manifest_path.display()
        )
    })?;
    let record = snapshot_record_from_manifest(cache_root, &manifest_path, manifest)?;
    Ok(record.filter(|record| {
        record.task_id == task_id
            && record.snapshot_name == "_state"
            && record.emitting_state == state_name
            && record.visit == visit
            && record.target_slug == target_slug
            && record.produced_by == "orchestrator"
            && record.is_current
    }))
}
