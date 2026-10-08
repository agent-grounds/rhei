struct SnapshotCommandContext {
    workspace_root: PathBuf,
    plan_path: PathBuf,
    cache_root: PathBuf,
    loaded: LoadedPlan,
    machines: rhei_validator::MachineSet,
    settings: RheiSettings,
}

#[derive(Clone, Debug)]
struct SnapshotRecord {
    path: PathBuf,
    manifest: serde_json::Value,
    task_id: String,
    snapshot_name: String,
    emitting_state: String,
    visit: u64,
    target_slug: String,
    generation: u64,
    created_at: String,
    transcript_bytes: u64,
    completion: String,
    produced_by: String,
    is_current: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct SnapshotIdentity {
    task_id: String,
    snapshot_name: String,
    emitting_state: String,
    visit: u64,
    target_slug: String,
}

#[derive(Clone, Debug, Default)]
struct SnapshotPreload {
    parent_ref: Option<serde_json::Value>,
    extra_args: Vec<String>,
    session_dir: Option<PathBuf>,
    // Resolved `dir_template` directory (§FS-rhei-snapshots.9.1) — the
    // agent's own session storage, distinct from `session_dir`: rhei only
    // ever reads from it, never stages or copies a transcript into it.
    fixed_session_dir: Option<PathBuf>,
    // Set when the profile declares `assign_id_flag`: the id rhei generated
    // and passed to the agent, read back at `<fixed_session_dir>/<id>.<ext>`.
    fixed_session_id: Option<String>,
    // Spawn wall-clock floor for the no-`assign_id_flag` case: the newest
    // matching file in `fixed_session_dir` is only a candidate if it was
    // written at or after this instant, so a leftover transcript from an
    // earlier invocation is never mistaken for this one's.
    fixed_session_scan_floor: Option<std::time::SystemTime>,
    // The layout's optional locator keys, resolved at spawn with the directory
    // they search; the default is the v1 flat lookup.
    // §FS-rhei-snapshots.9.1.1 §FS-rhei-snapshots.10.1
    fixed_session_locator: SnapshotSessionLocator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapshotCompletion {
    Success,
    Failure,
    Timeout,
}

impl SnapshotCompletion {
    fn as_str(self) -> &'static str {
        match self {
            SnapshotCompletion::Success => "success",
            SnapshotCompletion::Failure => "failure",
            SnapshotCompletion::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum SnapshotProducedBy {
    Orchestrator,
    Operator,
}

impl SnapshotProducedBy {
    fn as_str(self) -> &'static str {
        match self {
            SnapshotProducedBy::Orchestrator => "orchestrator",
            SnapshotProducedBy::Operator => "operator",
        }
    }
}

impl SnapshotRecord {
    fn identity(&self) -> SnapshotIdentity {
        SnapshotIdentity {
            task_id: self.task_id.clone(),
            snapshot_name: self.snapshot_name.clone(),
            emitting_state: self.emitting_state.clone(),
            visit: self.visit,
            target_slug: self.target_slug.clone(),
        }
    }

    fn display_ref(&self) -> String {
        format!(
            "{}:{}:{}@{}:{}/g{}",
            self.task_id,
            self.snapshot_name,
            self.emitting_state,
            self.visit,
            self.target_slug,
            self.generation
        )
    }

    fn transcript_path(&self) -> PathBuf {
        let relative = self
            .manifest
            .get("transcript_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("transcript.jsonl");
        self.path.join(relative)
    }

    fn to_listing_json(&self, orphaned: bool) -> serde_json::Value {
        serde_json::json!({
            "task_id": self.task_id,
            "snapshot_name": self.snapshot_name,
            "emitting_state": self.emitting_state,
            "visit": self.visit,
            "target_slug": self.target_slug,
            "generation": self.generation,
            "created_at": self.created_at,
            "transcript_bytes": self.transcript_bytes,
            "completion": self.completion,
            "produced_by": self.produced_by,
            "current": self.is_current,
            "orphaned": orphaned,
            "path": self.path.display().to_string(),
        })
    }
}

fn snapshot_parent_ref(record: &SnapshotRecord) -> serde_json::Value {
    serde_json::json!({
        "task_id": record.task_id,
        "snapshot_name": record.snapshot_name,
        "emitting_state": record.emitting_state,
        "visit": record.visit,
        "target_slug": record.target_slug,
        "generation": record.generation,
    })
}

fn snapshot_session(resolved: &ResolvedAgent) -> Option<&serde_json::Value> {
    resolved.profile.session.as_ref()
}

fn snapshot_session_layout(session: &serde_json::Value) -> Option<&serde_json::Value> {
    session.get("layout")
}

fn snapshot_layout_kind(layout: &serde_json::Value) -> Option<String> {
    layout.get("kind").and_then(serde_json::Value::as_str).map(|kind| match kind {
        "flat_by_id" | "flat-by-id" | "FlatById" => "FlatById".to_string(),
        "per_project_json" | "per-project-json" | "PerProjectJson" => "PerProjectJson".to_string(),
        other => other.to_string(),
    })
}

fn snapshot_layout_ext(layout: &serde_json::Value) -> Option<String> {
    layout
        .get("ext")
        .or_else(|| layout.get("extension"))
        .and_then(serde_json::Value::as_str)
        .map(|ext| ext.trim_start_matches('.').to_string())
}

fn snapshot_session_string(session: &serde_json::Value, key: &str) -> Option<String> {
    session.get(key).and_then(serde_json::Value::as_str).map(str::to_string)
}

fn snapshot_strategy_flag(session: &serde_json::Value, key: &str) -> Option<String> {
    match session.get(key)? {
        serde_json::Value::String(value) if value != "none" && !value.trim().is_empty() => {
            Some(value.clone())
        }
        serde_json::Value::Object(map) => map
            .get("flag")
            .or_else(|| map.get("native").and_then(|value| value.get("flag")))
            .or_else(|| map.get("copy_and_resume").and_then(|value| value.get("flag")))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn snapshot_resume_supported(session: &serde_json::Value) -> bool {
    snapshot_strategy_flag(session, "resume").is_some()
}

fn snapshot_layout_manifest(session: &serde_json::Value) -> Option<serde_json::Value> {
    let layout = snapshot_session_layout(session)?;
    let kind = snapshot_layout_kind(layout)?;
    let ext = snapshot_layout_ext(layout)?;
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), serde_json::Value::String(kind));
    object.insert("ext".to_string(), serde_json::Value::String(ext));
    for key in ["dir_template", "root_template", "project_hash"] {
        if let Some(value) = layout.get(key) {
            object.insert(key.to_string(), value.clone());
        }
    }
    Some(serde_json::Value::Object(object))
}

// §FS-rhei-snapshots.9.1: Emit support = supported layout AND (session_dir_flag OR dir_template).
fn snapshot_emit_session_supported(session: &serde_json::Value) -> bool {
    snapshot_session_has_supported_layout(session)
        && (snapshot_session_string(session, "session_dir_flag").is_some()
            || snapshot_session_layout(session).and_then(snapshot_layout_dir_template).is_some())
}

fn snapshot_layout_dir_template(layout: &serde_json::Value) -> Option<String> {
    layout
        .get("dir_template")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|template| !template.is_empty())
        .map(str::to_string)
}

// §FS-rhei-snapshots.9.1: `~/` expands against the home directory,
// `{cwd_dashed}` against this spawn's own working directory; an
// unrecognized `{name}` placeholder is a resolution failure, not a literal.
fn resolve_snapshot_dir_template(
    template: &str,
    spawn_working_dir: &Path,
) -> MietteResult<PathBuf> {
    if template == "~" {
        return home_dir();
    }
    let (home, rest) = match template.strip_prefix("~/") {
        Some(rest) => (Some(home_dir()?), rest),
        None => (None, template),
    };
    let expanded = expand_snapshot_dir_template_placeholders(rest, spawn_working_dir)?;
    Ok(match home {
        Some(home) => home.join(expanded),
        None => PathBuf::from(expanded),
    })
}

// §FS-rhei-snapshots.9.1: Expands `{cwd_dashed}` in a `dir_template` tail;
// any other `{name}` token fails so a typo'd placeholder is never read as a
// literal directory name that quietly matches nothing.
fn expand_snapshot_dir_template_placeholders(
    template: &str,
    spawn_working_dir: &Path,
) -> MietteResult<String> {
    let mut expanded = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open + 1..].find('}') else {
            return Err(miette!(
                help = snapshot_help(),
                "dir_template '{}' has an unterminated placeholder",
                template
            ));
        };
        let close = open + 1 + close;
        expanded.push_str(&rest[..open]);
        match &rest[open + 1..close] {
            "cwd_dashed" => expanded.push_str(&dashed_spawn_working_dir(spawn_working_dir)?),
            other => {
                return Err(miette!(
                    help = snapshot_help(),
                    "dir_template placeholder '{{{}}}' is not recognized",
                    other
                ));
            }
        }
        rest = &rest[close + 1..];
    }
    expanded.push_str(rest);
    Ok(expanded)
}

// §FS-rhei-snapshots.9.1: Claude Code's convention — every character outside
// `[A-Za-z0-9-]` becomes `-` — applied to the canonicalized spawn working
// directory, so it matches the cwd the child process itself observes.
fn dashed_spawn_working_dir(spawn_working_dir: &Path) -> MietteResult<String> {
    let canonical = rhei_core::platform::canonical_path(spawn_working_dir).map_err(|err| {
        file_io_report(spawn_working_dir, "failed to canonicalize spawn working directory", err)
    })?;
    Ok(canonical
        .display()
        .to_string()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' { ch } else { '-' })
        .collect())
}

fn generate_snapshot_session_id() -> String {
    format!("rhei-{}", snapshot_nonce())
}

fn snapshot_session_has_supported_layout(session: &serde_json::Value) -> bool {
    let Some(layout) = snapshot_session_layout(session) else {
        return false;
    };
    snapshot_layout_ext(layout).is_some()
        && matches!(snapshot_layout_kind(layout).as_deref(), Some("FlatById"))
}

fn snapshot_preload_session_supported(session: &serde_json::Value) -> bool {
    snapshot_session_has_supported_layout(session)
        && (snapshot_resume_supported(session) || snapshot_strategy_flag(session, "fork").is_some())
}

fn snapshot_target_slug_or_err(resolved: &ResolvedAgent) -> MietteResult<String> {
    resolved_agent_target_slug(resolved).ok_or_else(|| {
        miette!(
            help = snapshot_key_help(),
            "snapshot-requires-target: agent '{}' does not resolve provider and model",
            resolved.agent.id()
        )
    })
}

fn snapshot_target_selector(resolved: &ResolvedAgent) -> String {
    resolved.target.as_ref().map(ExecutionTarget::selector).unwrap_or_else(|| {
        let provider = resolved.model_provider.as_deref().unwrap_or_default();
        let model =
            resolved.model_name.as_deref().or(resolved.model.as_deref()).unwrap_or_default();
        let mut selector = resolved.agent.id().to_string();
        if let Some(mode) = resolved.mode.as_deref() {
            selector.push('[');
            selector.push_str(mode);
            selector.push(']');
        }
        selector.push(':');
        selector.push_str(provider);
        selector.push(':');
        selector.push_str(model);
        selector
    })
}

fn snapshot_resolved_target_json(resolved: &ResolvedAgent) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("agent".to_string(), serde_json::Value::String(resolved.agent.id().to_string()));
    if let Some(mode) = resolved.mode.as_deref() {
        object.insert("mode".to_string(), serde_json::Value::String(mode.to_string()));
    }
    if let Some(provider) = resolved.model_provider.as_deref() {
        object.insert("provider".to_string(), serde_json::Value::String(provider.to_string()));
    }
    if let Some(model) = resolved.model_name.as_deref().or(resolved.model.as_deref()) {
        object.insert("model".to_string(), serde_json::Value::String(model.to_string()));
    }
    serde_json::Value::Object(object)
}

fn snapshot_session_dir(
    workspace_root: &Path,
    task_id: &str,
    state_name: &str,
    slug: &str,
) -> PathBuf {
    workspace_root
        .join("runtime")
        .join("snapshot-sessions")
        .join(format!("{task_id}-{state_name}-{slug}-{}", snapshot_nonce()))
}

fn snapshot_nonce() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

fn snapshot_declared_provider(resolved: &ResolvedAgent) -> &str {
    resolved.model_provider.as_deref().unwrap_or_default()
}

fn snapshot_declared_model(resolved: &ResolvedAgent) -> &str {
    resolved.model_name.as_deref().or(resolved.model.as_deref()).unwrap_or_default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(not(test))]
const SNAPSHOT_REDACTOR_TIMEOUT: Duration = Duration::from_secs(30);
// A margin around the test build's Python redactors, which no test means to fire,
// so longer than the shipped 30 s on purpose: a stalled runner can keep an
// interpreter from its first line for tens of seconds (§REQ-cross-platform.8).
#[cfg(test)]
const SNAPSHOT_REDACTOR_TIMEOUT: Duration = tests::FIXTURE_MARGIN;

/// Run the configured snapshot redactor over a transcript.
///
/// A supervised invocation like any other the run starts: its own process
/// group, the shared `SIGTERM` → grace → `SIGKILL` sequence, and one wait that
/// ends on exit, deadline, or the run's interruption. `label` names the
/// invocation that needed it, for the shutdown notice.
// §FS-rhei-run.3.2 §FS-rhei-snapshots
fn apply_snapshot_redactor(
    settings: &RheiSettings,
    workspace_root: &Path,
    transcript_bytes: Vec<u8>,
    log_path: Option<&Path>,
    label: &str,
) -> MietteResult<Vec<u8>> {
    let Some(snapshot_settings) = settings.snapshots.as_ref() else {
        return Ok(transcript_bytes);
    };
    let Some(redactor) = snapshot_settings.redactor.as_ref() else {
        return Ok(transcript_bytes);
    };
    let redactor_path =
        if redactor.is_absolute() { redactor.clone() } else { workspace_root.join(redactor) };
    let redactor_label = redactor_path.display().to_string();
    let mut command = std::process::Command::new(&redactor_path);
    command
        .current_dir(workspace_root)
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (key, value) in snapshot_redactor_default_env(workspace_root) {
        command.env(key, value);
    }
    for key in &snapshot_settings.redactor_env {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut supervised = match Supervised::spawn(&mut command, label) {
        Ok(supervised) => supervised,
        // Interrupted before it started. It propagates as the mid-run case
        // below does, and the durable log says so. §FS-rhei-run.3.2
        Err(err) if spawn_was_interrupted(&err) => {
            append_snapshot_redactor_diagnostic(
                log_path,
                &redactor_label,
                &never_started_status(),
                false,
                true,
                false,
                "<not started>",
            )?;
            return Err(miette!(
                help = snapshot_redactor_help(),
                "snapshot redactor '{}' interrupted by run shutdown before it started",
                redactor_label
            ));
        }
        Err(err) => {
            return Err(file_io_report(&redactor_path, "failed to spawn snapshot redactor", err))
        }
    };
    let child = &mut supervised.child;
    let mut stdin =
        child.stdin.take().ok_or_else(|| miette!(
            help = snapshot_redactor_help(),
            "failed to open snapshot redactor stdin"
        ))?;
    let mut stdout =
        child.stdout.take().ok_or_else(|| miette!(
            help = snapshot_redactor_help(),
            "failed to open snapshot redactor stdout"
        ))?;
    let mut stderr =
        child.stderr.take().ok_or_else(|| miette!(
            help = snapshot_redactor_help(),
            "failed to open snapshot redactor stderr"
        ))?;

    let writer = std::thread::spawn(move || stdin.write_all(&transcript_bytes));
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    // One wait for all three endings: exit, deadline, run interruption. The
    // last two end the redactor's whole group. §FS-rhei-run.3.2
    let ended = supervised
        .wait(Some(SNAPSHOT_REDACTOR_TIMEOUT), &INTERRUPT, &|text| diag_warn!("{text}"))
        .map_err(|err| miette!(
            help = snapshot_redactor_help(),
            "error waiting for snapshot redactor: {err}"
        ))?;
    let status = ended.status;
    let timed_out = ended.cause == EndCause::TimedOut;
    let interrupted = ended.cause == EndCause::Interrupted;

    let writer_result = writer
        .join()
        .map_err(|_| miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor stdin writer panicked"
        ))?;
    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor stdout reader panicked"
        ))?
        .map_err(|err| miette!(
            help = snapshot_redactor_help(),
            "failed to read snapshot redactor stdout: {err}"
        ))?;
    let stderr_bytes = stderr_reader
        .join()
        .map_err(|_| miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor stderr reader panicked"
        ))?
        .map_err(|err| miette!(
            help = snapshot_redactor_help(),
            "failed to read snapshot redactor stderr: {err}"
        ))?;
    let (stderr_summary, stderr_truncated) = snapshot_redactor_stderr_summary(&stderr_bytes);
    append_snapshot_redactor_diagnostic(
        log_path,
        &redactor_label,
        &status,
        timed_out,
        interrupted,
        stderr_truncated,
        &stderr_summary,
    )?;

    if timed_out {
        return Err(miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor '{}' timed out after {}s; stderr: {}",
            redactor_label,
            SNAPSHOT_REDACTOR_TIMEOUT.as_secs_f64(),
            stderr_summary
        ));
    }
    // Not "timed out": the redactor was ended by the shutdown, and the error
    // says which. It propagates exactly as the timeout error does — the caller
    // abandons the snapshot and the ticket keeps its state. §FS-rhei-run.3.2
    if interrupted {
        return Err(miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor '{}' interrupted by run shutdown; stderr: {}",
            redactor_label,
            stderr_summary
        ));
    }
    if !status.success() {
        return Err(miette!(
            help = snapshot_redactor_help(),
            "snapshot redactor '{}' exited with status {}; stderr: {}",
            redactor_label,
            status,
            stderr_summary
        ));
    }
    writer_result.map_err(|err| miette!(
        help = snapshot_redactor_help(),
        "failed to write snapshot redactor stdin: {err}"
    ))?;
    Ok(stdout_bytes)
}

fn snapshot_redactor_default_env(workspace_root: &Path) -> Vec<(&'static str, PathBuf)> {
    // env_clear() wipes the child env first, so anything omitted here is simply
    // unset for the redactor. Skip RHEI_EXECUTABLE_PATH / RHEI_GLOBAL_SETTINGS_PATH
    // when their source fails rather than passing an empty PathBuf, which would
    // appear to the redactor as a real-but-empty path.
    let mut env: Vec<(&'static str, PathBuf)> = Vec::with_capacity(4);
    if let Ok(executable) = std::env::current_exe() {
        env.push(("RHEI_EXECUTABLE_PATH", executable));
    }
    env.push(("RHEI_WORKSPACE_ROOT", workspace_root.to_path_buf()));
    env.push(("RHEI_PROJECT_SETTINGS_PATH", project_settings_path(workspace_root)));
    if let Ok(home) = home_dir() {
        env.push(("RHEI_GLOBAL_SETTINGS_PATH", home.join(".config/rhei/settings.json")));
    }
    env
}

/// `interrupted` is carried alongside `timed_out` because a redactor the run
/// shut down exits by signal and so has no code: without the cause the durable
/// log records an unexplained failure, while the agent and program footers
/// beside it say plainly that the run was interrupted.
// §FS-rhei-run.3.2 §FS-rhei-agents.8
#[allow(clippy::too_many_arguments)]
fn append_snapshot_redactor_diagnostic(
    log_path: Option<&Path>,
    redactor_path: &str,
    status: &std::process::ExitStatus,
    timed_out: bool,
    interrupted: bool,
    stderr_truncated: bool,
    stderr_summary: &str,
) -> MietteResult<()> {
    let Some(log_path) = log_path else {
        return Ok(());
    };
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| file_io_report(parent, "failed to create snapshot log dir", err))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|err| file_io_report(log_path, "failed to append snapshot redactor diagnostic", err))?;
    let summary = stderr_summary.replace('\n', "\\n").replace('\r', "\\r");
    writeln!(
        file,
        "snapshot redactor: path={} status={} timeout={} interrupted={} \
         stderr_truncated={} stderr={}",
        redactor_path, status, timed_out, interrupted, stderr_truncated, summary
    )
    .map_err(|err| file_io_report(log_path, "failed to write snapshot redactor diagnostic", err))
}

fn snapshot_redactor_stderr_summary(bytes: &[u8]) -> (String, bool) {
    const LIMIT: usize = 1024;
    if bytes.is_empty() {
        return ("<empty>".to_string(), false);
    }
    let clipped = &bytes[..bytes.len().min(LIMIT)];
    let mut summary = String::from_utf8_lossy(clipped).trim().to_string();
    if summary.is_empty() {
        summary = "<empty>".to_string();
    }
    (summary, bytes.len() > LIMIT)
}

#[allow(clippy::too_many_arguments)]
fn write_snapshot_generation_atomic(
    cache_root: &Path,
    workspace_root: &Path,
    settings: &RheiSettings,
    task_id: &str,
    snapshot_name: &str,
    emitting_state: &str,
    visit: u64,
    target_slug: &str,
    target_selector: &str,
    resolved: &ResolvedAgent,
    session_layout: serde_json::Value,
    session_id: &str,
    transcript_source: &Path,
    transcript_ext: &str,
    observed_provider: &str,
    observed_model: &str,
    parent_ref: Option<&serde_json::Value>,
    completion: SnapshotCompletion,
    produced_by: SnapshotProducedBy,
    redactor_log_path: Option<&Path>,
) -> MietteResult<SnapshotRecord> {
    let identity_dir = cache_root
        .join(task_id)
        .join(snapshot_name)
        .join(emitting_state)
        .join(visit.to_string())
        .join(target_slug);
    fs::create_dir_all(&identity_dir).map_err(|err| {
        file_io_report(&identity_dir, "failed to create snapshot identity dir", err)
    })?;
    let lock_path = identity_dir.join(".lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|err| file_io_report(&lock_path, "failed to open snapshot identity lock", err))?;
    lock.lock_exclusive()
        .map_err(|err| file_io_report(&lock_path, "failed to lock snapshot identity", err))?;

    let transcript_bytes = fs::read(transcript_source).map_err(|err| {
        file_io_report(transcript_source, "failed to read snapshot transcript source", err)
    })?;
    let transcript_bytes = apply_snapshot_redactor(
        settings,
        workspace_root,
        transcript_bytes,
        redactor_log_path,
        &format!("{task_id}@{emitting_state} redactor"),
    )?;
    let transcript_sha256 = sha256_hex(&transcript_bytes);
    let transcript_name = format!("transcript.{transcript_ext}");
    let mut generation = next_snapshot_generation(&identity_dir)?;

    loop {
        let nonce = snapshot_nonce();
        let tmp_dir = identity_dir.join(format!("g{generation}.tmp-{nonce}"));
        let generation_dir = identity_dir.join(format!("g{generation}"));
        fs::create_dir_all(&tmp_dir).map_err(|err| {
            file_io_report(&tmp_dir, "failed to create snapshot staging dir", err)
        })?;
        let transcript_path = tmp_dir.join(&transcript_name);
        fs::write(&transcript_path, &transcript_bytes).map_err(|err| {
            file_io_report(&transcript_path, "failed to write snapshot transcript", err)
        })?;
        let created_at = format_iso8601_utc(std::time::SystemTime::now());
        let manifest = serde_json::json!({
            "version": 1,
            "rhei_version": env!("CARGO_PKG_VERSION"),
            "snapshot_name": snapshot_name,
            "task_id": task_id,
            "emitting_state": emitting_state,
            "visit": visit,
            "generation": generation,
            "target": {
                "selector": target_selector,
                "slug": target_slug,
                "resolved": snapshot_resolved_target_json(resolved),
            },
            "declared_provider": snapshot_declared_provider(resolved),
            "declared_model": snapshot_declared_model(resolved),
            "observed_provider": observed_provider,
            "observed_model": observed_model,
            "session_id": session_id,
            "session_layout": session_layout,
            "transcript_path": transcript_name,
            "transcript_sha256": transcript_sha256,
            "transcript_bytes": transcript_bytes.len() as u64,
            "parent_ref": parent_ref.cloned().unwrap_or(serde_json::Value::Null),
            "created_at": created_at,
            "completion": completion.as_str(),
            "produced_by": produced_by.as_str(),
        });
        let manifest_text = serde_json::to_string_pretty(&manifest)
            .map_err(|err| miette!(
                help = internal_error_help(),
                "failed to serialize snapshot manifest: {err}"
            ))?;
        fs::write(tmp_dir.join("manifest.json"), manifest_text).map_err(|err| {
            file_io_report(&tmp_dir.join("manifest.json"), "failed to write snapshot manifest", err)
        })?;

        match fs::rename(&tmp_dir, &generation_dir) {
            Ok(()) => {
                if produced_by == SnapshotProducedBy::Orchestrator {
                    replace_current_pointer(
                        &identity_dir,
                        &format!("g{generation}"),
                        &nonce,
                    )?;
                }
                let _ = fs2::FileExt::unlock(&lock);
                return snapshot_record_from_manifest(
                    cache_root,
                    &generation_dir.join("manifest.json"),
                    manifest,
                )?
                .ok_or_else(|| {
                    miette!(
                        help = snapshot_help(),
                        "failed to read back snapshot generation '{}'",
                        generation_dir.display()
                    )
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_dir_all(&tmp_dir);
                generation = next_snapshot_generation(&identity_dir)?;
            }
            Err(err) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                let _ = fs2::FileExt::unlock(&lock);
                return Err(file_io_report(
                    &generation_dir,
                    "failed to finalize snapshot generation",
                    err,
                ));
            }
        }
    }
}

fn next_snapshot_generation(identity_dir: &Path) -> MietteResult<u64> {
    let mut generation = 1;
    if identity_dir.exists() {
        for entry in fs::read_dir(identity_dir).map_err(|err| {
            file_io_report(identity_dir, "failed to inspect snapshot identity dir", err)
        })? {
            let entry = entry.map_err(|err| {
                file_io_report(identity_dir, "failed to inspect snapshot identity entry", err)
            })?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.contains(".tmp-") {
                continue;
            }
            if let Some(value) = name.strip_prefix('g').and_then(|value| value.parse::<u64>().ok())
            {
                generation = generation.max(value.saturating_add(1));
            }
        }
    }
    Ok(generation)
}
