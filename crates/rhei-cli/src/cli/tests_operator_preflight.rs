// Existing readers are control witnesses; new diagnostic assertions stay red.
// §FS-rhei-transition-cmd.6 §FS-rhei-recover.3

/// Probe itself is already read-only and releases exclusion immediately.
/// This control does not claim that the operator coordinator calls it yet.
/// §FS-rhei-run.2.6
#[test]
fn issue_757_existing_probe_is_read_only_and_noncreating() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert!(matches!(probe_run_lock(root), RunLockProbe::Missing(_)));
    assert!(!root.join(".rhei").exists());
    fs::create_dir(root.join(".rhei")).unwrap();
    let path = root.join(".rhei/run.lock");
    fs::write(&path, "unchanged owner bytes").unwrap();
    assert!(matches!(probe_run_lock(root), RunLockProbe::Free));
    assert_eq!(fs::read(&path).unwrap(), b"unchanged owner bytes");
    let file = fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    file.try_lock_exclusive().unwrap();
    assert!(matches!(probe_run_lock(root), RunLockProbe::Held));
    assert_eq!(read_run_lock_owner(&path).unwrap(), "unchanged owner bytes");
    fs2::FileExt::unlock(&file).unwrap();
    drop(file);
    fs::remove_file(&path).unwrap();
    fs::remove_dir(root.join(".rhei")).unwrap();
    // A non-directory parent makes opening the lock uncertain on every platform.
    fs::write(root.join(".rhei"), "not a directory").unwrap();
    assert!(matches!(probe_run_lock(root), RunLockProbe::Unknown(_)));
    assert_eq!(fs::read(root.join(".rhei")).unwrap(), b"not a directory");
}

fn issue_757_journal(root: &Path, names: Option<&str>) -> String {
    use std::fmt::Write as _;
    use rhei_tui::{RunEvent, RunProvenance, MessageLevel};
    let mut events = vec![RunEvent::RunStarted {
        run_id: "scope-run".into(), workspace: root.into(), parallel: 1, total_tasks: 2,
        provenance: RunProvenance::default(),
    }];
    if let Some(names) = names {
        events.push(RunEvent::Message { level: MessageLevel::Info,
            text: format!("Scope: narrowed to {names}") });
    }
    events.push(RunEvent::PassStarted { pass: 1, ready: vec![] });
    let mut journal = String::new();
    for (n, event) in events.iter().enumerate() {
        let value = rhei_tui::encode_event(Some(n as u64 + 1), event,
            std::time::SystemTime::UNIX_EPOCH, Some(root));
        writeln!(&mut journal, "{value}").unwrap();
    }
    journal
}

/// Whole/narrowed/repeated selection already comes from the startup journal,
/// while missing, torn, stale and noncontiguous prefixes remain unknown.
/// §FS-rhei-transition-cmd.3.4
#[test]
fn issue_757_existing_scope_reader_requires_verified_complete_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("runtime")).unwrap();
    let path = rhei_tui::event_log_path(root);
    assert_eq!(live_budget_scope(root, "scope-run"), None);
    for names in [None, Some("constants"), Some("constants, profile")] {
        let journal = issue_757_journal(root, names);
        fs::write(&path, &journal).unwrap();
        let expected = names.map(|names| names.split(", ").map(str::to_owned).collect());
        assert_eq!(live_budget_scope(root, "scope-run"), Some(expected));
        assert_eq!(fs::read_to_string(&path).unwrap(), journal);
        assert_eq!(live_budget_scope(root, "stale-run"), None);
        fs::write(&path, journal.trim_end_matches('\n')).unwrap();
        assert_eq!(live_budget_scope(root, "scope-run"), None);
        fs::write(&path, journal.replace("\"seq\":2", "\"seq\":20")).unwrap();
        assert_eq!(live_budget_scope(root, "scope-run"), None);
    }
    fs::write(&path, "{}\n").unwrap();
    assert_eq!(live_budget_scope(root, "scope-run"), None);
}

/// Refuse unverified owner records without promoting their id or pid to advice.
/// §FS-rhei-errors.1.2 §FS-rhei-recover.3
#[test]
fn issue_757_unverified_owner_diagnostics_are_safe() {
    let mut violations = Vec::new();
    for mode in ["malformed", "unsupported", "stale", "mismatched", "descriptor-mismatch", "missing", "operator"] {
        let (dir, _plan, _machine) = operator_fixture();
        let root = dir.path();
        let mut held = try_acquire_run_lock(root).unwrap().unwrap();
        let mut owner = serde_json::json!({"version":1, "id":"unverified-run",
            "pid":u32::MAX, "workspace":root, "process_start_ticks":0});
        match mode {
            "unsupported" => owner["version"] = 99.into(),
            "mismatched" => owner["workspace"] = root.join("elsewhere").display().to_string().into(),
            "descriptor-mismatch" => {
                owner["pid"] = std::process::id().into();
                let descriptor = operator_registry_descriptor(root);
                fs::create_dir_all(root.join("runtime")).unwrap();
                fs::write(root.join("runtime/run.json"), serde_json::to_vec(&descriptor).unwrap()).unwrap();
            }
            "operator" => owner = serde_json::json!({"operator":"other-operator",
                "operation":"forced-recovery","pid":1}),
            _ => {}
        }
        let body = match mode {
            "malformed" => "{malformed".to_owned(),
            "missing" => String::new(),
            _ => owner.to_string(),
        };
        held.file.rewind().unwrap();
        held.file.set_len(0).unwrap();
        held.file.write_all(body.as_bytes()).unwrap();
        held.file.flush().unwrap();
        let before = read_run_lock_owner(&root.join(".rhei/run.lock")).unwrap();
        let error = operator_run_locks(&[root.into()], "new-operator").err().unwrap();
        let text = format!("{error:?}");
        assert_eq!(read_run_lock_owner(&root.join(".rhei/run.lock")).unwrap(), before);
        assert!(!text.contains("rhei stop "), "{mode}: unsafe stop advice: {text}");
        if text.contains("unverified-run") || text.contains("4294967295")
            || text.contains("recorded owner:") {
            violations.push(format!("{mode}: raw/unverified ownership escaped: {text}"));
        }
        assert!(text.contains("wait"), "{mode}: missing safe waiting remedy: {text}");
        if mode == "operator" {
            assert!(text.contains("other-operator") && text.contains("operator"), "{text}");
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

fn issue_757_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() { walk(root, &path, files); }
            else {
                let bytes = if path.ends_with(".rhei/run.lock") {
                    read_run_lock_owner(&path).unwrap().into_bytes()
                } else { fs::read(&path).unwrap() };
                files.insert(path.strip_prefix(root).unwrap().into(), bytes);
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

/// Basin-parent contention must be seen from both force and marker-only replay.
/// The existing marker preview derives owners without parsing the plan.
/// §FS-rhei-transition-cmd.6 §FS-rhei-recover.3
#[test]
fn issue_757_basin_parent_and_member_preflight_handoff() {
    let mut missing = Vec::new();
    for replay in [false, true] {
        for parent in [true, false] {
            let (_dir, project, root, machine) = operator_basin_fixture();
            let request = operator_basin_request(&project, &machine);
            if replay {
                interrupt_force_at("marker-after");
                assert!(commit_confirmed_force(&request,
                    prepare_forced_transition(&request).unwrap(), "test").is_err());
                clear_force_interrupt();
            }
            let owner = if parent { &project } else { &root };
            let _held = operator_run_locks(std::slice::from_ref(owner), "other-operator").unwrap();
            let before = issue_757_snapshot(&project);
            let error = if replay { recover_command(&root).unwrap_err() } else {
                force_transition_command(&project, &[], Some(&machine), "basin.1.1",
                    "work", "done", "repair basin route", Some("fresh basin result")).unwrap_err()
            };
            assert_eq!(issue_757_snapshot(&project), before);
            let text = error.help().expect("complete operator handoff").to_string();
            let expected = if replay {
                let marker = recorded_marker(&root);
                format!("recover {} basin.1.1 work -> done rollback", marker.recovery_id)
            } else { "force basin.1.1 work -> done".to_owned() };
            assert!(text.contains(&format!("Type exactly: {expected}")), "{text}");
            if !text.contains("other-operator") || !text.contains(&owner.display().to_string()) {
                missing.push(format!("replay={replay} parent={parent}: held basin owner must be handed off before confirmation: {text}"));
            }
            assert!(!text.contains("rhei stop "), "operator is not a run: {text}");
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

include!("tests_operator_retry.rs");
