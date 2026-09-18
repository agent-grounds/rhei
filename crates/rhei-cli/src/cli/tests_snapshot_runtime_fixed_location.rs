// agent-grounds/rhei#125: fixed-location session emit (`dir_template`, with and
// without `assign_id_flag`) — split from tests_snapshot_runtime.rs to stay
// under the file-size budget. §AR-source-file-size.3

/// RAII guard that removes `HOME` for the duration of the test, sharing
/// `TEST_HOME_LOCK` with `TempHome` so the two never race over the same
/// process-global env var.
struct NoHome {
    previous: Option<std::ffi::OsString>,
    previous_state: Option<std::ffi::OsString>,
    _state: tempfile::TempDir,
    _state_guard: std::sync::MutexGuard<'static, ()>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl NoHome {
    fn new() -> Self {
        let guard = TEST_HOME_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let state_guard = run_descriptor_tests::REGISTRY_GUARD
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let state = tempfile::tempdir().expect("state dir");
        let previous = std::env::var_os("HOME");
        let previous_state = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_STATE_HOME", state.path());
        std::env::remove_var("HOME");
        NoHome { previous, previous_state, _state: state, _state_guard: state_guard, _guard: guard }
    }
}

impl Drop for NoHome {
    fn drop(&mut self) {
        if let Some(prev) = self.previous.take() {
            std::env::set_var("HOME", prev);
        }
        match self.previous_state.take() {
            Some(prev) => std::env::set_var("XDG_STATE_HOME", prev),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
    }
}

// agent-grounds/rhei#125 R1-01: an unresolvable `dir_template` (no `HOME`) must
// degrade to no fixed-location tracking rather than fail the spawn, even on
// a state with no snapshot block, since preload runs for every spawn. §FS-rhei-snapshots.10.1
#[test]
fn snapshot_fixed_location_unresolvable_dir_template_runs_cold_instead_of_failing_spawn() {
    let _no_home = NoHome::new();
    let dir = snapshot_workspace();
    write_fixed_location_no_snapshot_machine(dir.path());
    let settings = fixed_location_settings("~/.fixed-agent/sessions", None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("an unresolvable dir_template must never fail the spawn");
    assert!(preload.fixed_session_dir.is_none());
    assert!(preload.fixed_session_id.is_none());
    assert!(preload.fixed_session_scan_floor.is_none());
    assert!(preload.extra_args.is_empty());
}

// agent-grounds/rhei#128: an unrecognized `{name}` placeholder must degrade to no
// fixed-location tracking rather than fail the spawn, and must never be read
// as a literal directory name. §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.1
#[test]
fn snapshot_fixed_location_unrecognized_placeholder_runs_cold_instead_of_failing_spawn() {
    let dir = snapshot_workspace();
    write_fixed_location_no_snapshot_machine(dir.path());
    let dir_template = dir.path().join("fixed-sessions").join("{typo}");
    let settings = fixed_location_settings(dir_template.to_str().expect("utf8 path"), None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("an unrecognized placeholder must never fail the spawn");
    assert!(preload.fixed_session_dir.is_none());
    assert!(preload.fixed_session_id.is_none());
    assert!(preload.fixed_session_scan_floor.is_none());
    assert!(preload.extra_args.is_empty());
}

// agent-grounds/rhei#128: pins the ticket's own correction — `.` dashes the same
// as `/`, so `/x/.claude-worktrees/y` dashes to `-x--claude-worktrees-y`, not
// the dot-preserving shape the ticket's reproduction showed. §FS-rhei-snapshots.9.1
#[test]
fn snapshot_dashed_spawn_working_dir_matches_claude_code_convention() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dotted = tmp.path().join(".claude-worktrees");
    let child = dotted.join("under_score.and-dash5");
    fs::create_dir_all(&child).expect("nested spawn dir");

    let dashed = dashed_spawn_working_dir(&child).expect("dash spawn dir");
    assert!(!dashed.contains('/'), "no path separator should survive dashing: {dashed}");
    assert!(!dashed.contains('.'), "no dot should survive dashing: {dashed}");
    assert!(!dashed.contains('_'), "no underscore should survive dashing: {dashed}");
    assert!(
        dashed.ends_with("--claude-worktrees-under-score-and-dash5"),
        "a dot right after a path separator must produce a double dash, and \
         alphanumerics/existing dashes must survive untouched: {dashed}"
    );
}

// agent-grounds/rhei#128: `dir_template: <parent>/{cwd_dashed}` resolves against
// the working directory this spawn is given — not the workspace root or the
// plan input path — canonicalized before dashing. §FS-rhei-snapshots.9.1
#[test]
fn snapshot_fixed_location_cwd_dashed_placeholder_resolves_against_spawn_working_dir() {
    let dir = snapshot_workspace();
    write_fixed_location_no_snapshot_machine(dir.path());
    let sessions_parent = dir.path().join("claude-projects");
    let settings =
        fixed_location_settings(&format!("{}/{{cwd_dashed}}", sessions_parent.display()), None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let spawn_dir = dir.path().join("checkout").join(".worktrees").join("issue-128");
    fs::create_dir_all(&spawn_dir).expect("spawn dir");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        &spawn_dir,
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("{cwd_dashed} placeholder resolves");

    let expected_child = dashed_spawn_working_dir(&spawn_dir).expect("dash spawn dir");
    assert_eq!(
        preload.fixed_session_dir.as_deref(),
        Some(sessions_parent.join(&expected_child).as_path())
    );
}

// agent-grounds/rhei#128: the ticket's own reproduction, turned positive — a
// `dir_template: <parent>/{cwd_dashed}` locates the transcript written under
// the cwd-derived child after the spawn floor. §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.2
#[test]
fn snapshot_fixed_location_cwd_dashed_dir_template_emits_scan_floor_transcript() {
    let dir = snapshot_workspace();
    write_fixed_location_emit_machine(dir.path());
    let sessions_parent = dir.path().join("claude-projects");
    let settings =
        fixed_location_settings(&format!("{}/{{cwd_dashed}}", sessions_parent.display()), None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let spawn_dir = dir.path().join("checkout");
    fs::create_dir_all(&spawn_dir).expect("spawn dir");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        &spawn_dir,
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("cwd-derived fixed-location profile preloads");
    let floor = preload.fixed_session_scan_floor.expect("scan floor recorded");
    let session_dir = preload.fixed_session_dir.clone().expect("fixed session dir resolved");

    fs::create_dir_all(&session_dir).expect("cwd-derived session dir");
    let transcript = session_dir.join("123e4567-e89b-12d3-a456-426614174000.jsonl");
    fs::write(&transcript, b"claude code transcript\n").expect("write transcript");
    set_file_mtime(&transcript, floor + std::time::Duration::from_secs(1));

    let log_path = dir.path().join("runtime/logs/task-1-pending.log");
    fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
    fs::write(&log_path, "log\n").expect("log");

    emit_snapshots_after_agent_exit(
        dir.path(),
        &machine,
        &settings,
        task,
        "pending",
        Some("done"),
        &resolved,
        &log_path,
        1,
        SnapshotCompletion::Success,
        &preload,
    )
    .expect("emit captures the cwd-derived child transcript");

    let records =
        read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
    let named = records
        .iter()
        .find(|record| record.snapshot_name == "impl")
        .expect("named snapshot written");
    assert_eq!(
        fs::read(named.transcript_path()).expect("transcript"),
        b"claude code transcript\n"
    );
}

fn write_fixed_location_no_snapshot_machine(dir: &Path) {
    fs::write(
        dir.join("states.yaml"),
        r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: fixed:openai:model
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
    )
    .expect("write states");
}

// agent-grounds/rhei#125: a fixed-location session with `assign_id_flag` gets
// the flag and a rhei-chosen id at spawn, and emit reads the exact
// `<dir>/<id>.<ext>` path afterward. §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.2
#[test]
fn snapshot_fixed_location_assign_id_flag_reads_exact_path_and_stamps_manifest() {
    let dir = snapshot_workspace();
    write_fixed_location_emit_machine(dir.path());
    let fixed_dir = dir.path().join("fixed-sessions");
    let settings =
        fixed_location_settings(fixed_dir.to_str().expect("utf8 path"), Some("--session-id"));
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("fixed-location profile with assign_id_flag preloads");
    assert_eq!(preload.fixed_session_dir.as_deref(), Some(fixed_dir.as_path()));
    assert!(preload.session_dir.is_none(), "assign_id_flag path never redirects");
    let assigned_id = preload.fixed_session_id.clone().expect("assigned id");
    assert_eq!(preload.extra_args, vec!["--session-id".to_string(), assigned_id.clone()]);

    fs::create_dir_all(&fixed_dir).expect("fixed dir");
    fs::write(fixed_dir.join(format!("{assigned_id}.jsonl")), b"assigned transcript\n")
        .expect("write transcript");

    let log_path = dir.path().join("runtime/logs/task-1-pending.log");
    fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
    fs::write(&log_path, "log\n").expect("log");

    emit_snapshots_after_agent_exit(
        dir.path(),
        &machine,
        &settings,
        task,
        "pending",
        Some("done"),
        &resolved,
        &log_path,
        1,
        SnapshotCompletion::Success,
        &preload,
    )
    .expect("emit reads the exact assigned-id path");

    let records =
        read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
    let named = records
        .iter()
        .find(|record| record.snapshot_name == "impl")
        .expect("named snapshot written");
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some(assigned_id.as_str())
    );
    assert_eq!(fs::read(named.transcript_path()).expect("transcript"), b"assigned transcript\n");
}

// Without `assign_id_flag`, emit scans for the newest file no earlier than
// the spawn — a pre-existing older transcript is not captured.
// §FS-rhei-snapshots.9.1 §FS-rhei-snapshots.10.2
#[test]
fn snapshot_fixed_location_without_assign_id_flag_ignores_transcripts_older_than_spawn() {
    let dir = snapshot_workspace();
    write_fixed_location_emit_machine(dir.path());
    let fixed_dir = dir.path().join("fixed-sessions");
    fs::create_dir_all(&fixed_dir).expect("fixed dir");
    let old_path = fixed_dir.join("old-session.jsonl");
    fs::write(&old_path, b"stale transcript\n").expect("old transcript");
    set_file_mtime(&old_path, std::time::SystemTime::now() - std::time::Duration::from_secs(3600));

    let settings = fixed_location_settings(fixed_dir.to_str().expect("utf8 path"), None);
    let (loaded, machine, resolved) = snapshot_preload_parts(dir.path(), &settings);
    let task = loaded.rhei.tasks.first().expect("task");

    let preload = preload_snapshot_inherit_before_spawn(
        dir.path(),
        single_root_preload(dir.path()),
        dir.path(),
        &machine,
        task,
        "pending",
        &resolved,
        &settings,
        1,
        None,
        &default_run_options(),
    )
    .expect("fixed-location profile without assign_id_flag preloads");
    assert!(preload.fixed_session_id.is_none());
    assert!(preload.extra_args.is_empty());
    let floor = preload.fixed_session_scan_floor.expect("scan floor recorded");

    let new_path = fixed_dir.join("new-session.jsonl");
    fs::write(&new_path, b"live transcript\n").expect("new transcript");
    set_file_mtime(&new_path, floor + std::time::Duration::from_secs(10));

    let log_path = dir.path().join("runtime/logs/task-1-pending.log");
    fs::create_dir_all(log_path.parent().expect("log parent")).expect("log dir");
    fs::write(&log_path, "log\n").expect("log");

    emit_snapshots_after_agent_exit(
        dir.path(),
        &machine,
        &settings,
        task,
        "pending",
        Some("done"),
        &resolved,
        &log_path,
        1,
        SnapshotCompletion::Success,
        &preload,
    )
    .expect("emit scans for the newest transcript since spawn");

    let records =
        read_snapshot_records(&snapshot_cache_dir(&settings, dir.path())).expect("records");
    let named = records
        .iter()
        .find(|record| record.snapshot_name == "impl")
        .expect("named snapshot written");
    assert_eq!(
        named.manifest.get("session_id").and_then(serde_json::Value::as_str),
        Some("new-session")
    );
    assert_eq!(fs::read(named.transcript_path()).expect("transcript"), b"live transcript\n");
}

fn write_fixed_location_emit_machine(dir: &Path) {
    fs::write(
        dir.join("states.yaml"),
        r#"name: snapshot-test
version: 1
states:
  pending:
    description: pending
    initial: true
    target: fixed:openai:model
    snapshot:
      emit:
        name: impl
        on: always
  done:
    description: done
    final: true
transitions:
  - from: pending
    to: done
"#,
    )
    .expect("write states");
}

/// `dir_template` is an absolute path into the test's own tempdir rather
/// than a `~/`-prefixed one, which pins tilde-free templates working
/// alongside the home-relative case covered by the ticket's own repro.
// §FS-rhei-snapshots.9.1
fn fixed_location_settings(dir_template: &str, assign_id_flag: Option<&str>) -> RheiSettings {
    let mut session = serde_json::json!({
        "layout": {
            "kind": "FlatById",
            "dir_template": dir_template,
            "ext": "jsonl"
        }
    });
    if let Some(flag) = assign_id_flag {
        session["assign_id_flag"] = serde_json::Value::String(flag.to_string());
    }
    let mut agents = BTreeMap::new();
    agents.insert(
        "fixed".to_string(),
        CustomAgentProfile {
            command: vec!["fixed".to_string()],
            prompt_flag: Some("-p".to_string()),
            session: Some(session),
            ..Default::default()
        },
    );
    RheiSettings { agents, ..Default::default() }
}

fn set_file_mtime(path: &Path, time: std::time::SystemTime) {
    let file = fs::OpenOptions::new().write(true).open(path).expect("open for mtime");
    file.set_modified(time).expect("set mtime");
}
