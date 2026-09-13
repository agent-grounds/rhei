#[derive(Clone)]
struct CallbackPaths {
    plan_path: PathBuf,
    state_machine_path: Option<PathBuf>,
    working_dir: PathBuf,
}

#[derive(Clone, Copy)]
struct TransitionFiles<'a> {
    task_file: &'a Path,
    metadata_file: &'a Path,
    /// `metadata.tasks.<id>` key inside `metadata_file`. Equal to the ticket's
    /// rhei-local id except for basin tickets, whose metadata shares the
    /// project manifest and is therefore qualified. See [`TaskRoute`].
    metadata_id: &'a str,
    /// Owning rhei's execution root for artifact resolution. §FS-rhei-panta.6.2
    artifact_root: &'a Path,
    /// Project-qualified ticket id rendered into `{task_id}` artifact
    /// templates and gate messages. §AR-rhei-panta.2
    artifact_id: &'a str,
}

fn resolve_callback_paths(
    state_machine_path: Option<&Path>,
    plan_path: &Path,
) -> MietteResult<CallbackPaths> {
    let plan_path = rhei_core::platform::canonical_path(plan_path).map_err(|err| {
        file_io_report(plan_path, "failed to resolve plan path for callbacks", err)
    })?;
    let state_machine_path = state_machine_path
        .map(|path| {
            rhei_core::platform::canonical_path(path).map_err(|err| {
                file_io_report(path, "failed to resolve state machine path for callbacks", err)
            })
        })
        .transpose()?;
    let base_dir = if let Some(path) = state_machine_path.as_deref() {
        rhei_core::workspace::plan_parent_dir(path)
    } else if plan_path.is_dir() {
        plan_path.as_path()
    } else {
        rhei_core::workspace::plan_parent_dir(&plan_path)
    };

    let working_dir = rhei_core::platform::canonical_path(base_dir).map_err(|err| {
        file_io_report(base_dir, "failed to resolve callback working directory", err)
    })?;

    Ok(CallbackPaths { plan_path, state_machine_path, working_dir })
}

/// The execution root every derived path hangs off: the plan's own directory,
/// or the directory workspace itself.
///
/// The parent goes through [`rhei_core::workspace::plan_parent_dir`] because
/// `rhei next plan.rhei.md` hands us a bare filename, whose raw parent is the
/// empty path — a root that joins correctly and prints as nothing.
// §FS-rhei-memory.3.4
fn execution_workspace_root(plan_path: &Path) -> PathBuf {
    if plan_path.is_dir() {
        plan_path.to_path_buf()
    } else {
        rhei_core::workspace::plan_parent_dir(plan_path).to_path_buf()
    }
}

/// Build the cross-language `TransitionContext` JSON payload that is
/// delivered to callbacks on stdin.
///
/// Shape matches the TransitionContext data structure.
/// The `triggered_by` field must be one of `"user" | "callback" | "system" | "engine"`.
/// `transition_data` seeds the `transitionData` slot; pass `serde_json::Value::Object(Map::new())`
/// for the initial `on_leave` call, and the accumulated data from `on_leave` for `on_enter`.
// §FS-rhei-transitions.1: TransitionContext callback payload.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn build_transition_context_json(
    plan: Option<&rhei_core::ast::Rhei>,
    plan_path: &Path,
    task_id_str: &str,
    qualified_id: &str,
    from_state: &str,
    to_state: &str,
    triggered_by: &str,
    transition_data: &serde_json::Value,
    working_dir: &Path,
) -> serde_json::Value {
    use serde_json::{json, Map, Value};

    // `plan` is the parsed owning rhei file, so the lookup key is the
    // rhei-local id; the emitted `id` is project-qualified. §FS-rhei-panta.6
    let task_node = plan.and_then(|rhei| find_task_by_id_str(&rhei.tasks, task_id_str));

    let task_json = match task_node {
        Some(task) => json!({
            "id": qualified_id,
            "localId": task_id_to_json(&task.id),
            "kind": task.kind,
            "title": task.title,
            "content": task.content,
            "metadata": task_metadata_json(plan, task_id_str, from_state),
            "children": task.children.iter().map(task_summary_json).collect::<Vec<_>>(),
        }),
        None => json!({
            "id": qualified_id,
            "localId": task_id_str,
            "metadata": Value::Object(Map::new()),
            "children": Value::Array(Vec::new()),
        }),
    };

    let rhei_json = match plan {
        Some(rhei) => json!({
            "title": rhei.title,
            "path": plan_path.display().to_string(),
            "tasks": rhei.tasks.iter().map(task_summary_json).collect::<Vec<_>>(),
        }),
        None => json!({
            "title": Value::Null,
            "path": plan_path.display().to_string(),
            "tasks": Value::Array(Vec::new()),
        }),
    };

    json!({
        "rhei": rhei_json,
        "task": task_json,
        "transition": {
            "from": from_state,
            "to": to_state,
            "triggeredBy": triggered_by,
            "timestamp": current_iso8601(),
        },
        "transitionData": transition_data,
        "environment": {
            "platform": "cli",
            "version": env!("CARGO_PKG_VERSION"),
            "workingDirectory": working_dir.display().to_string(),
        },
    })
}

fn task_id_to_json(id: &TaskId) -> serde_json::Value {
    serde_json::Value::String(id.to_string())
}

fn task_summary_json(task: &rhei_core::ast::Task) -> serde_json::Value {
    serde_json::json!({
        "id": task_id_to_json(&task.id),
        "kind": task.kind,
        "title": task.title,
        "state": task.state,
    })
}

fn task_metadata_json(
    plan: Option<&rhei_core::ast::Rhei>,
    task_id_str: &str,
    state: &str,
) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut out = Map::new();
    out.insert("state".to_string(), Value::String(state.to_string()));

    let target = parse_task_id(task_id_str);
    let task = plan.and_then(|rhei| find_task_by_id(&rhei.tasks, &target));
    if let Some(task) = task {
        out.insert(
            "dependsOn".to_string(),
            Value::Array(task.prior.iter().map(|id| Value::String(id.to_string())).collect()),
        );
    } else {
        out.insert("dependsOn".to_string(), Value::Array(Vec::new()));
    }

    // Merge frontmatter task metadata (`metadata.tasks.<id>`) if present.
    if let Some(rhei) = plan {
        if let Some(task_meta) = task_metadata_map(rhei.metadata.as_ref(), &target) {
            for (key, value) in task_meta {
                let Some(key_str) = key.as_str() else { continue };
                // Don't clobber canonical fields.
                if key_str == "state" || key_str == "dependsOn" {
                    continue;
                }
                if let Ok(json_value) = yaml_value_to_json(value) {
                    out.insert(key_str.to_string(), json_value);
                }
            }
        }
    }

    Value::Object(out)
}

fn yaml_value_to_json(value: &YamlValue) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::to_value(value)
}

fn find_task_by_id_str<'a>(
    tasks: &'a [rhei_core::ast::Task],
    task_id_str: &str,
) -> Option<&'a rhei_core::ast::Task> {
    let target = parse_task_id(task_id_str);
    find_task_by_id(tasks, &target)
}

fn find_task_by_id<'a>(
    tasks: &'a [rhei_core::ast::Task],
    target: &TaskId,
) -> Option<&'a rhei_core::ast::Task> {
    for task in tasks {
        if &task.id == target {
            return Some(task);
        }
        if let Some(found) = find_task_by_id(&task.children, target) {
            return Some(found);
        }
    }
    None
}

fn current_iso8601() -> String {
    let now =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let (year, month, day, hour, minute, second) = unix_to_utc_components(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

/// Minimal civil-calendar conversion from seconds-since-Unix-epoch to UTC
/// Y-M-D h:m:s components. Uses the Howard Hinnant algorithm. Good through
/// the full 64-bit range; we don't need leap seconds or time zones for a
/// transition timestamp.
fn unix_to_utc_components(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let time_of_day = (secs % 86_400) as u32;
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;

    // days since 1970-01-01 → civil date.
    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = y + if m <= 2 { 1 } else { 0 };
    (y as i32, m as u32, d as u32, hour, minute, second)
}

/// Write `content` atomically to `path` via a same-directory temp file.
fn write_file_atomic(path: &Path, content: &str) -> MietteResult<()> {
    write_file_atomic_locked(path, content, None)
}

/// `write_file_atomic` for a destination this process holds the lock on.
///
/// The handle is what lets the replace succeed where a lock refuses one; see
/// `persist_locked`.
fn write_file_atomic_locked(
    path: &Path,
    content: &str,
    locked: Option<&LockedPlanFile>,
) -> MietteResult<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| miette!(
            help = temp_write_help(),
            "failed to create temp file: {err}"
        ))?;
    tmp.write_all(content.as_bytes()).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to write temp file: {err}"
    ))?;
    persist_locked(tmp, path, locked).map_err(|err| miette!(
        help = temp_write_help(),
        "failed to persist temp file: {err}"
    ))?;
    Ok(())
}

fn record_poll_self_loop_if_needed(
    loaded: &LoadedPlan,
    input: &Path,
    machine: &rhei_validator::StateMachine,
    task: &rhei_core::ast::Task,
    current_state: &str,
    to_state: &str,
) -> MietteResult<bool> {
    if current_state != to_state {
        return Ok(false);
    }
    let Some(poll) = machine.states.get(current_state).and_then(|def| def.poll.as_ref()) else {
        return Ok(false);
    };
    let interval = rhei_validator::parse_duration_secs(&poll.interval).unwrap_or(0);
    // The attempt count reads from the merged graph (qualified keys).
    let next_attempt_count = current_state_visit_count(
        loaded.rhei.metadata.as_ref(),
        &task.id,
        current_state,
        task.state.as_str(),
        machine,
    )
    .saturating_add(1);
    // §FS-rhei-panta.6.1: the write lands in the owning rhei's metadata file
    // under that file's own key space — the same one the exit-time
    // clear_poll_state_metadata removes.
    let route = loaded.task_route(&task.id.to_string(), input);
    let metadata_key = parse_task_id(&route.metadata_id);
    // Poll bookkeeping is a metadata writer and follows the same stable
    // exclusion as task transitions. §AR-agent-orchestrator-workflow.3.3.1
    let metadata_lock = LockedPlanFile::open(&route.metadata_file)?;
    let raw = metadata_lock.read_to_string("failed to read plan metadata file")?;
    let on_disk = parse_metadata_from_raw(&route.metadata_file, &raw)?;
    let updated = set_poll_next_attempt_metadata(
        on_disk.as_ref(),
        &metadata_key,
        current_state,
        current_unix_secs().saturating_add(interval),
        next_attempt_count,
    );
    let rewritten = rewrite_frontmatter(&raw, &updated)?;
    write_file_atomic_locked(&route.metadata_file, &rewritten, Some(&metadata_lock))?;
    Ok(true)
}

/// Parse the frontmatter metadata mapping from a metadata file (a workspace
/// `index.rhei.md` manifest, a Panta `index.panta.md` manifest, or a
/// single-file plan). Keys are rhei-local, except in the project manifest,
/// where basin tickets are keyed by their qualified ids. See [`TaskRoute`].
fn parse_metadata_from_raw(path: &Path, raw: &str) -> MietteResult<Option<Metadata>> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some("index.rhei.md") | Some(workspace::PANTA_INDEX_FILE) => {
            Ok(parse_metadata_manifest(path, raw)?.metadata)
        }
        _ => {
            let rhei = rhei_core::parse(raw)
                .map_err(|err| miette!(
help = plan_authoring_help(),
"{}: {}", path.display(), err.message))?;
            Ok(rhei.metadata)
        }
    }
}

/// Structure and metadata shared by both manifest forms.
struct MetadataManifest {
    structure: rhei_core::ast::Structure,
    metadata: Option<Metadata>,
}

/// Parse the manifest that owns a ticket's runtime metadata, dispatching on
/// which index form `path` names. A Panta manifest reaches here only for basin
/// tickets, which have no index of their own. §FS-rhei-panta.6.1
fn parse_metadata_manifest(path: &Path, raw: &str) -> MietteResult<MetadataManifest> {
    if path.file_name().and_then(|name| name.to_str()) == Some(workspace::PANTA_INDEX_FILE) {
        let manifest = rhei_core::parser::parse_panta_manifest(raw).map_err(|err| {
            miette!(
                help = plan_authoring_help(),
                "failed to parse Panta manifest for transition metadata: {}",
                err.message
            )
        })?;
        return Ok(MetadataManifest {
            structure: manifest.structure,
            metadata: manifest.metadata,
        });
    }
    let index = rhei_core::parser::parse_workspace_index(raw).map_err(|err| {
        miette!(
            help = plan_authoring_help(),
            "failed to parse workspace index for transition metadata: {}",
            err.message
        )
    })?;
    Ok(MetadataManifest { structure: index.structure, metadata: index.metadata })
}

/// Merge `src` object keys into `dst` (last write wins). Non-object `src`
/// values are ignored. Used to accumulate `data` payloads across multiple
/// `on_leave` callbacks into a single `transitionData`.
fn merge_transition_data(dst: &mut serde_json::Value, src: &serde_json::Value) {
    use serde_json::Value;
    let (Value::Object(dst_map), Value::Object(src_map)) = (dst, src) else {
        return;
    };
    for (key, value) in src_map {
        dst_map.insert(key.clone(), value.clone());
    }
}
